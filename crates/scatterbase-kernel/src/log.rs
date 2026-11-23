//! The instance's log: a server partition that founds and describes the instance, and a
//! claim partition that holds every accepted claim.
//!
//! The log is the instance's source of truth. It is a [`scatter_log`] file store, and both
//! partitions keep every record and are `logged`: after each append the server signs a
//! checkpoint of the partition's Merkle tree head, so a copy of the log can be shown to be
//! complete and in order ([`ClaimLog::verify`]). The quad store is derived from the claim
//! partition ([`crate::projection`]).
//!
//! # The server partition
//!
//! Partition 0 holds `scatter:v0/config` records, as a `config` partition does in
//! triplespace. [`ClaimLog::create`] writes its first records:
//!
//! | Offset | Key | Content |
//! |---|---|---|
//! | 0 | `key:{key ID}` | The server's public key. This is the instance's **founding record**: it is signed by the key it registers, and everything else in the log is checked against that key. |
//! | 1 | `graph:server` | The server partition itself: partition 0 and its policies |
//! | 2 | `graph:claim` | The claim partition: its ID, 64 random bits, and its policies |
//! | 3 | `graph:meta` | The `meta` graph, a projection |
//!
//! The `graph:` records are the instance's graph registry, filled in from the rows of
//! [`GRAPHS_TOML`]. Each is attested by the server under the founding record's authority.
//! The claim partition is found by its `graph:claim` record, and the server's keys by the
//! `key:` records, which form a chain: a later key must be signed by the one before it.
//!
//! # The claim record
//!
//! A claim is one record of payload type `scatter:v0/claim` in the claim partition. Its
//! header key is the claim ID in z-base-32 and its `appended_at` is the time the server
//! accepted it. Its offset is the claim's sequence ID. The body has five parts, each
//! erasable on its own:
//!
//! | # | Part | CBOR |
//! |---|---|---|
//! | 0 | The claim | `{"previous": bytes(32), "triples": [text, …]}`, each triple its N-Triples line, in the claim's order |
//! | 1 | The attribution | text, the actor's IRI |
//! | 2 | The client signature | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64)}`, `key` the public key it verified against |
//! | 3 | The server signature | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64), "sequence": text, "timestamp": text}` |
//! | 4 | The acknowledgments | `[[text, bytes(32)], …]`, term IRI and prior claim ID, in order of the IRI |

use std::io;

use ed25519_dalek::{SigningKey, VerifyingKey};
use oxrdf::{NamedNode, Triple};
use oxttl::NTriplesParser;
use pollster::block_on;
use scatter_claims::id::ntriples_line;
use scatter_claims::{Acknowledgments, Claim, ClaimId, Envelope, Signature, SignedClaim};
use scatter_integrity::attest::{founding_attestation, instance_attestation, Authority};
use scatter_integrity::checkpoint::{host_of, tenant_origin};
use scatter_integrity::{CheckpointStore, FileCheckpoints, KeyChain, Level, Verify};
use scatter_log::body::{Body, Part};
use scatter_log::cbor::Value;
use scatter_log::genesis::fresh_partition_id;
use scatter_log::header::CONFIG_PARTITION;
use scatter_log::record::Record;
use scatter_log::registry::{GraphEntry, GraphRegistry, KeyEntry, Kind, PAYLOAD_CONFIG};
use scatter_log::segments::SegmentStore;
use scatter_log::store::{Draft, LogStore, Slot};
use scatter_log::tree::Segments;

use crate::instance::{Instance, GRAPH_CLAIM, GRAPH_SERVER};

/// The graphs of a Scatterbase instance: the rows its `graph:` records are filled in from.
pub const GRAPHS_TOML: &str = include_str!("graphs.toml");
/// The payload type of a claim record.
pub const PAYLOAD_CLAIM: &str = "scatter:v0/claim";
/// The server partition, which founds the instance and registers its graphs.
pub const SERVER_PARTITION: u64 = CONFIG_PARTITION;
/// The segment exponent of both partitions: 2^16 records to a segment file.
const SEGMENT_EXPONENT: u8 = 16;

/// A claim as the log holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClaim {
    /// The signed claim.
    pub signed: SignedClaim,
    /// The public key the actor's signature verified against when the claim was accepted.
    pub actor_key: VerifyingKey,
    /// The public key of the server that accepted it.
    pub server_key: VerifyingKey,
}

/// The log of an instance.
pub struct ClaimLog {
    store: SegmentStore,
    checkpoints: FileCheckpoints,
    /// The claim partition, as its `graph:claim` record names it.
    partition: u64,
    /// The checkpoint origin of the claim partition.
    origin: String,
    /// The checkpoint origin of the server partition.
    server_origin: String,
    /// The server's keys, from the `key:` records of the server partition.
    chain: KeyChain,
}

fn other(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

fn corrupt(offset: u64, what: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("claim log, offset {offset}: {what}"))
}

fn invalid(what: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("log: {what}"))
}

fn origin_of(instance: &Instance, partition_name: &str) -> String {
    tenant_origin(&host_of(&instance.domain), partition_name)
}

/// The records of a partition, from its first.
fn records(store: &SegmentStore, partition: u64) -> io::Result<Vec<Record>> {
    let size = block_on(store.head(partition)).map_err(other)?.size;
    (0..size)
        .map(|offset| match block_on(store.read(partition, offset)).map_err(other)? {
            Slot::Record(record) => Ok(record),
            Slot::Compacted { .. } => Err(invalid(format!("partition {partition:016x}, offset {offset}: the record was compacted"))),
        })
        .collect()
}

impl ClaimLog {
    /// Creates the instance's log in its empty log directory: the server partition with its
    /// founding record and graph registry, and an empty claim partition.
    pub fn create(instance: &Instance, server_key: &SigningKey) -> io::Result<Self> {
        let mut store = SegmentStore::open(&instance.log_dir).map_err(other)?;
        if !block_on(store.partitions()).map_err(other)?.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already holds a log", instance.log_dir.display()),
            ));
        }
        let registry = GraphRegistry::parse(GRAPHS_TOML).map_err(other)?;
        let row = |name: &str| registry.lookup(name).map(|(graph, _)| graph)
            .ok_or_else(|| invalid(format!("the registry has no `{name}` graph")));
        let segments = Segments::new(SEGMENT_EXPONENT).expect("the exponent is below 64");
        let appended_at = u64::try_from(chrono::Utc::now().timestamp_micros()).map_err(other)?;
        let comment = Value::Null;
        let draft = |key: String, content: &Value, attestation: &Value| -> io::Result<Draft> {
            Ok(Draft {
                appended_at,
                payload_type: PAYLOAD_CONFIG.into(),
                key: Some(key),
                revid: None,
                logid: None,
                page_id: None,
                body: Body::core(content, &comment, attestation).map_err(other)?,
            })
        };

        // The founding record: the server's key, signed by itself
        let key = KeyEntry::ed25519(server_key.verifying_key().to_bytes());
        let content = key.to_value();
        let founding = founding_attestation(server_key, &content, &comment, None).map_err(|_| invalid("the founding record does not encode"))?;
        block_on(store.create_partition(SERVER_PARTITION, segments)).map_err(other)?;
        let founded = block_on(store.append(SERVER_PARTITION, draft(key.key(), &content, &founding)?)).map_err(other)?;
        let authority = Authority { partition: SERVER_PARTITION, offset: founded.offset, leaf: founded.leaf };

        // The graph registry: one record for each row, under the founding record's authority
        let claim_partition = fresh_partition_id();
        for graph in registry.graphs() {
            let entry = match (graph.kind, graph.name.as_str()) {
                (Kind::Source, GRAPH_SERVER) => GraphEntry::source(graph, graph.name.clone(), SERVER_PARTITION, SEGMENT_EXPONENT),
                (Kind::Source, GRAPH_CLAIM) => GraphEntry::source(graph, graph.name.clone(), claim_partition, SEGMENT_EXPONENT),
                (Kind::Source, name) => return Err(invalid(format!("no partition is made for the source graph `{name}`"))),
                (Kind::Projection, _) => GraphEntry {
                    kind: "graph".into(),
                    name: graph.name.clone(),
                    kind_of: graph.kind,
                    scope: graph.scope,
                    history: None,
                    integrity: None,
                    export: graph.export,
                    partition: None,
                    segment_exponent: None,
                    hash: None,
                    payload_types: Vec::new(),
                },
            };
            let content = entry.to_value().map_err(other)?;
            let attestation = instance_attestation(server_key, instance.server.as_str(), &authority, None, false, &content, &comment)
                .map_err(|_| invalid("a graph record does not encode"))?;
            block_on(store.append(SERVER_PARTITION, draft(entry.key(), &content, &attestation)?)).map_err(other)?;
        }
        // Both source rows were given partitions above, so this fails only on a registry
        // that lacks one of them
        row(GRAPH_SERVER)?;
        row(GRAPH_CLAIM)?;
        block_on(store.create_partition(claim_partition, segments)).map_err(other)?;

        let mut checkpoints = FileCheckpoints::new(&instance.log_dir);
        let server_origin = origin_of(instance, GRAPH_SERVER);
        let checkpoint = block_on(scatter_integrity::checkpoints::checkpoint(&store, SERVER_PARTITION, &server_origin, server_key)).map_err(other)?;
        block_on(checkpoints.put(SERVER_PARTITION, &checkpoint)).map_err(other)?;
        drop(store);
        Self::open(instance)
    }

    /// Opens the instance's log, which [`ClaimLog::create`] made.
    pub fn open(instance: &Instance) -> io::Result<Self> {
        let store = SegmentStore::open(&instance.log_dir).map_err(other)?;
        if !block_on(store.partitions()).map_err(other)?.contains(&SERVER_PARTITION) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} holds no server partition. Run 'scatterbase init' first.", instance.log_dir.display()),
            ));
        }
        let server_records = records(&store, SERVER_PARTITION)?;
        let chain = KeyChain::from_records(&server_records).map_err(invalid)?;
        if chain.current().is_none() {
            return Err(invalid("the server partition registers no key"));
        }

        // The claim partition is the one the latest `graph:claim` record names
        let key = format!("graph:{GRAPH_CLAIM}");
        let partition = server_records.iter().rev()
            .filter(|record| record.header().payload_type == PAYLOAD_CONFIG && record.header().key.as_deref() == Some(key.as_str()))
            .find_map(|record| GraphEntry::from_value(record.body().content().value().ok()??).ok()?.partition)
            .ok_or_else(|| invalid("the server partition registers no claim partition"))?;

        Ok(Self {
            store,
            checkpoints: FileCheckpoints::new(&instance.log_dir),
            partition,
            origin: origin_of(instance, GRAPH_CLAIM),
            server_origin: origin_of(instance, GRAPH_SERVER),
            chain,
        })
    }

    /// The number of claims in the log, which is the offset the next one takes.
    pub fn size(&self) -> io::Result<u64> {
        Ok(block_on(self.store.head(self.partition)).map_err(other)?.size)
    }

    /// The origin line of the claim partition's checkpoints, `{host}/log/claim`.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The server's current public key: the one its latest `key:` record registers.
    pub fn server_key(&self) -> VerifyingKey {
        self.chain.current()
            .and_then(|key| key.public_key())
            .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok())
            .expect("an open log has a current Ed25519 key")
    }

    /// The graphs the server partition registers, by name, each with its partition if it is
    /// a source.
    pub fn graphs(&self) -> io::Result<Vec<(String, Option<u64>)>> {
        let mut graphs: Vec<(String, Option<u64>)> = Vec::new();
        for record in records(&self.store, SERVER_PARTITION)? {
            let is_graph = record.header().key.as_deref().is_some_and(|key| key.starts_with("graph:"));
            if record.header().payload_type != PAYLOAD_CONFIG || !is_graph {
                continue;
            }
            let content = record.body().content().value().map_err(invalid)?
                .ok_or_else(|| invalid("a graph record was erased"))?;
            let entry = GraphEntry::from_value(content).map_err(invalid)?;
            graphs.retain(|(name, _)| *name != entry.name);
            graphs.push((entry.name, entry.partition));
        }
        Ok(graphs)
    }

    /// Appends an accepted claim and signs a checkpoint of the new head. Returns the offset.
    ///
    /// The claim's sequence ID must be the offset it takes, so that the server's signature
    /// commits to the claim's place in the log, and `server_key` must be the server's
    /// current key.
    pub fn append(&mut self, signed: &SignedClaim, actor_key: &VerifyingKey, server_key: &SigningKey) -> io::Result<u64> {
        if server_key.verifying_key() != self.server_key() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the signing key is not the key the server partition registers",
            ));
        }
        let offset = self.size()?;
        if signed.sequence_id() != offset.to_string() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("the claim has sequence ID {} but the log is at offset {offset}", signed.sequence_id()),
            ));
        }
        let appended_at = chrono::DateTime::parse_from_rfc3339(signed.timestamp())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("the claim's timestamp: {e}")))?
            .timestamp_micros();
        let draft = Draft {
            appended_at: u64::try_from(appended_at).map_err(other)?,
            payload_type: PAYLOAD_CLAIM.into(),
            key: Some(signed.id().to_string()),
            revid: None,
            logid: None,
            page_id: None,
            body: body_of(signed, actor_key, &server_key.verifying_key())?,
        };
        let appended = block_on(self.store.append(self.partition, draft)).map_err(other)?;

        let checkpoint = block_on(scatter_integrity::checkpoints::checkpoint(
            &self.store, self.partition, &self.origin, server_key,
        )).map_err(other)?;
        block_on(self.checkpoints.put(self.partition, &checkpoint)).map_err(other)?;
        Ok(appended.offset)
    }

    /// The claims from offset `from` to the head, each with its offset.
    pub fn claims(&self, from: u64) -> io::Result<Vec<(u64, StoredClaim)>> {
        let size = self.size()?;
        (from..size)
            .map(|offset| {
                let slot = block_on(self.store.read(self.partition, offset)).map_err(other)?;
                let Slot::Record(record) = slot else {
                    return Err(corrupt(offset, "the record was compacted"));
                };
                Ok((offset, claim_of(&record).map_err(|e| corrupt(offset, e))?))
            })
            .collect()
    }

    /// Verifies the whole log and returns the number of claims in it.
    ///
    /// `server_key` is the key the caller expects the server to have, and must be the
    /// current key of the server partition's chain. Both partitions are then checked by
    /// [`scatter_integrity::verify`]: headers, body commitments, the Merkle tree, the signed
    /// checkpoints, and in the server partition the founding record, the key chain and
    /// every attestation. Every claim must also read back as the claim its key names at the
    /// sequence ID of its offset, under a server key of the chain, and carry an actor's
    /// signature and a server's signature that verify.
    pub fn verify(&self, server_key: &VerifyingKey) -> io::Result<u64> {
        if *server_key != self.server_key() {
            return Err(invalid("the server partition's current key is not the expected server key"));
        }
        for (name, partition, origin, is_config) in [
            (GRAPH_SERVER, SERVER_PARTITION, &self.server_origin, true),
            (GRAPH_CLAIM, self.partition, &self.origin, false),
        ] {
            let report = block_on(scatter_integrity::verify(&self.store, &self.checkpoints, &Verify {
                partition,
                origin,
                chain: &self.chain,
                level: Level::Bodies,
                actor_keys: None,
                part_names: None,
                is_config,
                withheld: None,
            }));
            if !report.is_clean() {
                let problems: Vec<String> = report.problems.iter().map(|problem| format!("{problem:?}")).collect();
                return Err(invalid(format!("the {name} partition: {}", problems.join("; "))));
            }
            if report.size > 0 && block_on(self.checkpoints.latest(partition)).map_err(other)?.is_none_or(|c| c.checkpoint.size != report.size) {
                return Err(invalid(format!("the {name} partition has records its latest checkpoint does not cover")));
            }
        }

        for (offset, stored) in self.claims(0)? {
            if self.chain.lookup(&scatter_log::registry::key_id(stored.server_key.as_bytes())).is_none() {
                return Err(corrupt(offset, "accepted under a key that is not the server's"));
            }
            stored.signed.envelope().verify(&stored.actor_key).map_err(|e| corrupt(offset, e))?;
            stored.signed.verify(&stored.server_key).map_err(|e| corrupt(offset, e))?;
        }
        self.size()
    }
}

fn bytes(bytes: &[u8]) -> Value {
    Value::Bytes(bytes.to_vec())
}

fn signature_value(signature: Signature, key: &VerifyingKey, more: Vec<(Value, Value)>) -> Value {
    let mut pairs = vec![
        (Value::text("alg"), Value::text("ed25519")),
        (Value::text("key"), bytes(key.as_bytes())),
        (Value::text("sig"), bytes(signature.as_bytes())),
    ];
    pairs.extend(more);
    Value::map(pairs)
}

/// The body of a claim record: the five parts of the module's table.
fn body_of(signed: &SignedClaim, actor_key: &VerifyingKey, server_key: &VerifyingKey) -> io::Result<Body> {
    let envelope = signed.envelope();
    let claim = envelope.claim();
    let parts = [
        Value::map(vec![
            (Value::text("previous"), bytes(claim.previous().as_bytes())),
            (Value::text("triples"), Value::Array(claim.triples().iter().map(|t| Value::Text(ntriples_line(t))).collect())),
        ]),
        Value::text(claim.attribution().as_str()),
        signature_value(envelope.signature(), actor_key, vec![]),
        signature_value(signed.server_signature(), server_key, vec![
            (Value::text("sequence"), Value::text(signed.sequence_id())),
            (Value::text("timestamp"), Value::text(signed.timestamp())),
        ]),
        Value::Array(envelope.acknowledgments().iter()
            .map(|(term, prior)| Value::Array(vec![Value::text(term.as_str()), bytes(prior.as_bytes())]))
            .collect()),
    ];
    let parts = parts.iter().map(Part::of).collect::<Result<Vec<Part>, _>>().map_err(other)?;
    Body::new(parts).map_err(other)
}

fn array<const N: usize>(value: Option<&Value>, what: &str) -> Result<[u8; N], String> {
    value.and_then(Value::as_bytes)
        .and_then(|b| <[u8; N]>::try_from(b).ok())
        .ok_or_else(|| format!("{what} is not {N} bytes"))
}

fn text<'a>(value: Option<&'a Value>, what: &str) -> Result<&'a str, String> {
    value.and_then(Value::as_text).ok_or_else(|| format!("{what} is not text"))
}

fn signature_of(value: &Value, what: &str) -> Result<(Signature, VerifyingKey), String> {
    if text(value.get("alg"), what)? != "ed25519" {
        return Err(format!("{what} is not ed25519"));
    }
    let key = VerifyingKey::from_bytes(&array::<32>(value.get("key"), what)?)
        .map_err(|_| format!("{what} names a key that is not an Ed25519 public key"))?;
    Ok((Signature::from_bytes(array::<64>(value.get("sig"), what)?), key))
}

/// Reads a claim record back. Signatures are not checked.
fn claim_of(record: &Record) -> Result<StoredClaim, String> {
    let header = record.header();
    if header.payload_type != PAYLOAD_CLAIM {
        return Err(format!("a {} record, not a claim", header.payload_type));
    }
    let part = |index: usize, name: &str| -> Result<Value, String> {
        record.body().part(index)
            .ok_or_else(|| format!("no {name} part"))?
            .value().map_err(|e| format!("the {name} part: {e}"))?
            .ok_or_else(|| format!("the {name} part was erased"))
    };

    let content = part(0, "claim")?;
    let previous = ClaimId::from_bytes(array::<32>(content.get("previous"), "the previous claim")?);
    let Some(Value::Array(lines)) = content.get("triples") else {
        return Err("the claim has no triples".into());
    };
    let mut triples: Vec<Triple> = Vec::with_capacity(lines.len());
    for line in lines {
        let line = text(Some(line), "a triple")?;
        let mut parsed = NTriplesParser::new().for_slice(line);
        match (parsed.next(), parsed.next()) {
            (Some(Ok(triple)), None) => triples.push(triple),
            _ => return Err(format!("not one N-Triples line: {line}")),
        }
    }
    let attribution_value = part(1, "attribution")?;
    let attribution = NamedNode::new(text(Some(&attribution_value), "the attribution")?)
        .map_err(|e| format!("the attribution: {e}"))?;
    let claim = Claim::new(triples, attribution, previous).map_err(|e| e.to_string())?;
    if header.key.as_deref() != Some(claim.id().to_string().as_str()) {
        return Err(format!("the record's key is not its claim's ID, {}", claim.id()));
    }

    let Value::Array(pairs) = part(4, "acknowledgments")? else {
        return Err("the acknowledgments are not an array".into());
    };
    let mut acknowledgments = Acknowledgments::new();
    for pair in &pairs {
        let Value::Array(pair) = pair else { return Err("an acknowledgment is not a pair".into()) };
        let term = NamedNode::new(text(pair.first(), "an acknowledged term")?)
            .map_err(|e| format!("an acknowledged term: {e}"))?;
        acknowledgments.insert(term, ClaimId::from_bytes(array::<32>(pair.get(1), "a prior claim")?));
    }

    let (signature, actor_key) = signature_of(&part(2, "client signature")?, "the client signature")?;
    let envelope = Envelope::from_parts(claim, acknowledgments, signature).map_err(|e| e.to_string())?;

    let server = part(3, "server signature")?;
    let (server_signature, server_key) = signature_of(&server, "the server signature")?;
    let sequence_id = text(server.get("sequence"), "the sequence ID")?;
    if sequence_id != header.offset.to_string() {
        return Err(format!("the claim has sequence ID {sequence_id}"));
    }
    let signed = SignedClaim::from_parts(envelope, sequence_id, text(server.get("timestamp"), "the timestamp")?, server_signature)
        .map_err(|e| e.to_string())?;
    Ok(StoredClaim { signed, actor_key, server_key })
}
