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
//! | 3 | `graph:mutable` | The mutable partition: its ID and its policies |
//! | 4 | `graph:meta` | The `meta` graph, a projection |
//!
//! The `graph:` records are the instance's graph registry, filled in from the rows of
//! [`GRAPHS_TOML`]. Each is attested by the server under the founding record's authority.
//! The claim partition is found by its `graph:claim` record, and the server's keys by the
//! `key:` records, which form a chain: a later key must be signed by the one before it
//! ([`ClaimLog::rotate_server_key`]). The last key of the chain is the server's key; the
//! earlier ones are remembered so that what they signed still verifies.
//!
//! # The mutable partition
//!
//! The mutable partition holds what can be taken back: who the actors are and the keys
//! they sign with. Its records are attested by the server under the founding record's
//! authority, so only the server writes them.
//!
//! **Actors.** A record header is never erased, so it must not say who an actor is. An
//! actor is therefore known in this partition by a **surrogate**, a number the server
//! mints in sequence the first time the actor is given a record, and written `actor:{n}`.
//! The one place that says which actor a surrogate stands for is its `scatter:v0/actor`
//! record, keyed `actor:{n}`, whose content is a [`scatter_actors::ActorRecord`] of kind
//! `federated` with the actor's IRI as its raw value. Erase that record's content and only
//! the number is left: the actor's key records can no longer be tied to an IRI, and the
//! IRI seen again gets a new surrogate.
//!
//! **Keys.** A key is registered by a `scatter:v0/key` record whose header key is the
//! actor's surrogate and whose content is a [`scatter_actors::signing::KeyRecord`]: the
//! key ID, the algorithm, the public key and the time it is valid from. A later record for
//! the same key ID with `revoked` set revokes it. An actor may have several keys at once,
//! and a claim's signature is accepted if any of them that is valid at the time verifies
//! it.
//!
//! # The claim record
//!
//! A claim is one record of payload type `scatter:v0/claim` in the claim partition. Its
//! header key is the claim ID in z-base-32 and its `appended_at` is the time the server
//! accepted it. Its offset is the claim's sequence ID. The body has five parts:
//!
//! | # | Part | CBOR | Erasable |
//! |---|---|---|---|
//! | 0 | `content`: the claim | `{"previous": bytes(32), "triples": [text, …]}`, each triple its N-Triples line, in the claim's order | yes |
//! | 1 | `comment` | `null` | |
//! | 2 | `attestation`: who claims, and their signature | `{"actor": text, "alg": "ed25519", "key": bytes(32), "sig": bytes(64)}`, `key` the public key the signature verified against | yes |
//! | 3 | `acknowledgments` | `[[text, bytes(32)], …]`, term IRI and prior claim ID, in order of the IRI | yes |
//! | 4 | `acceptance`: the server's | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64), "client": text, "sequence": text, "timestamp": text}`, `client` the digest of the actor's signature | never |
//!
//! # Erasure
//!
//! Three parts of a claim can be erased, each on its own ([`ClaimPart`]): what was claimed,
//! who claimed it, and what it acknowledged. The header and the acceptance are always
//! kept, and they name the claim and the actor's signature by hashes alone, so an erased
//! claim still has its ID, its place in the log, its time and a server's signature that
//! verifies. The claims after it, whose chains and acknowledgments point at it, still
//! point at something the log can prove was there.
//!
//! An actor is erased as a whole ([`ClaimLog::erase_actor`]): its actor record and every
//! key record under its surrogate, leaving the number. That does not touch the claims the
//! actor made, whose attestations are erased one by one.
//!
//! Every erasure is itself recorded, before it is done, as a `scatter:v0/erase` record in
//! the same partition, saying what was erased, why and on what authority, and attested by
//! the server. [`ClaimLog::verify`] accepts a missing part only where such a record covers
//! it.

use std::io;
use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};
use oxrdf::{NamedNode, Triple};
use oxttl::NTriplesParser;
use pollster::block_on;
use scatter_claims::id::ntriples_line;
use scatter_claims::{Acceptance, Acknowledgments, Claim, ClaimId, Redacted, Signature, SignedClaim};
use scatter_actors::signing::{current_keys, KeyRecord, PublicKey};
use scatter_actors::{ActorKey, ActorKind, ActorRecord, ActorSurrogates};
use scatter_actors::time::Timestamp;
use scatter_integrity::attest::{client_signature, founding_attestation, instance_attestation, Authority};
use scatter_integrity::checkpoint::{host_of, tenant_origin};
use scatter_integrity::bundle::{BundlePartition, PartitionRegistry};
use scatter_integrity::{Bundle, CheckpointStore, FileCheckpoints, KeyChain, Level, Verify};
use scatter_log::body::{Body, Part};
use scatter_log::cbor::Value;
use scatter_log::genesis::fresh_partition_id;
use scatter_log::header::CONFIG_PARTITION;
use scatter_log::record::Record;
use scatter_log::registry::{GraphEntry, GraphRegistry, KeyEntry, Kind, PAYLOAD_CONFIG};
use scatter_log::segments::SegmentStore;
use scatter_log::store::{Draft, LogStore, Slot};
use scatter_log::tree::Segments;

use crate::instance::{Instance, GRAPH_CLAIM, GRAPH_MUTABLE, GRAPH_SERVER};

/// The graphs of a Scatterbase instance: the rows its `graph:` records are filled in from.
pub const GRAPHS_TOML: &str = include_str!("graphs.toml");
/// The payload type of a claim record.
pub const PAYLOAD_CLAIM: &str = "scatter:v0/claim";
/// The payload type of an actor record, which says which actor a surrogate stands for.
pub const PAYLOAD_ACTOR: &str = "scatter:v0/actor";
/// The payload type of an actor's key record.
pub const PAYLOAD_KEY: &str = "scatter:v0/key";
/// The server partition, which founds the instance and registers its graphs.
pub const SERVER_PARTITION: u64 = CONFIG_PARTITION;
/// The segment exponent of both partitions: 2^16 records to a segment file.
const SEGMENT_EXPONENT: u8 = 16;

/// The payload type of a record of an erasure.
pub const PAYLOAD_ERASE: &str = scatter_integrity::verify::PAYLOAD_ERASE;

/// A part of a claim record that can be erased.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClaimPart {
    /// What was claimed: the triples and the previous claim.
    Content,
    /// Who claimed it: the actor, the actor's signature and the key it verified against.
    Attestation,
    /// What the claim acknowledged: the prior use of each of its terms.
    Acknowledgments,
}

impl ClaimPart {
    /// Every erasable part.
    pub const ALL: [ClaimPart; 3] = [ClaimPart::Content, ClaimPart::Attestation, ClaimPart::Acknowledgments];

    /// The part's name, as an erase record and the command line write it.
    pub const fn name(self) -> &'static str {
        match self {
            ClaimPart::Content => "content",
            ClaimPart::Attestation => "attestation",
            ClaimPart::Acknowledgments => "acknowledgments",
        }
    }

    /// The part with a name.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|part| part.name() == name)
    }

    /// The part's index in the body of a claim record.
    const fn index(self) -> usize {
        match self {
            ClaimPart::Content => 0,
            ClaimPart::Attestation => 2,
            ClaimPart::Acknowledgments => PART_ACKNOWLEDGMENTS,
        }
    }
}

/// The body parts a claim record has after the three every record has, by index from 3.
const CLAIM_PART_NAMES: [&str; 2] = ["acknowledgments", "acceptance"];
const PART_ACKNOWLEDGMENTS: usize = 3;
const PART_ACCEPTANCE: usize = 4;

/// Why something is erased, as its erase record says.
#[derive(Debug, Clone, Copy)]
pub struct Reason<'a> {
    /// The class of reason: `legal`, `privacy`, `upstream` or `operational`.
    pub class: &'a str,
    /// What authorizes the erasure, as text: a ticket, a notice, a request.
    pub authority: &'a str,
    /// The public reason, if one is given.
    pub comment: Option<&'a str>,
}

/// The classes of reason an erasure can have.
pub const REASON_CLASSES: [&str; 4] = ["legal", "privacy", "upstream", "operational"];

/// A claim as the log holds it: the signed claim, less any part that was erased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClaim {
    /// The claim's offset in the claim partition, which is its sequence ID.
    pub offset: u64,
    /// What is left of the signed claim.
    pub claim: Redacted,
    /// The public key the actor's signature verified against when the claim was accepted,
    /// unless the attestation was erased.
    pub actor_key: Option<VerifyingKey>,
    /// The public key of the server that accepted it.
    pub server_key: VerifyingKey,
}

impl StoredClaim {
    /// The signed claim, if no part of it was erased.
    pub fn signed(&self) -> Option<SignedClaim> {
        self.claim.whole().ok().flatten()
    }

    /// The parts that were erased.
    pub fn erased(&self) -> Vec<ClaimPart> {
        let mut erased = Vec::new();
        if self.claim.claim.is_none() {
            erased.push(ClaimPart::Content);
        }
        if self.claim.attestation.is_none() {
            erased.push(ClaimPart::Attestation);
        }
        if self.claim.acknowledgments.is_none() {
            erased.push(ClaimPart::Acknowledgments);
        }
        erased
    }
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
    /// The mutable partition, as its `graph:mutable` record names it, and its origin.
    /// A bundle written for publication does not hold it.
    mutable: u64,
    has_mutable: bool,
    mutable_origin: String,
    /// The server's keys, from the `key:` records of the server partition.
    chain: KeyChain,
    /// The IRI the server attests as, and the founding record its attestations cite.
    server: NamedNode,
    authority: Authority,
    /// The instance's domain, which a bundle does not say.
    domain: Option<String>,
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

/// The surrogate a header key `actor:{n}` names.
fn surrogate_of_key(key: &str) -> Option<u64> {
    let key = ActorKey::parse(key).ok()?;
    if key.is_surrogate() { key.numeric_subject() } else { None }
}

/// The surrogates of a log as [`scatter_actors`] asks for them: a value is looked up, and
/// minted with the server's key if it has none. The only kind is `federated`, an actor
/// known by its IRI.
pub struct Surrogates<'a> {
    /// The log whose mutable partition holds the actor records.
    pub log: &'a mut ClaimLog,
    /// The server's key, which attests a newly minted surrogate's record.
    pub server_key: &'a SigningKey,
}

impl Surrogates<'_> {
    fn actor(kind: ActorKind, raw: &str) -> io::Result<NamedNode> {
        if kind != ActorKind::Federated {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{} actors have no surrogates here", kind.name())));
        }
        NamedNode::new(raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
    }
}

impl ActorSurrogates for Surrogates<'_> {
    type Error = io::Error;

    fn surrogate(&mut self, kind: ActorKind, raw: &str) -> io::Result<u64> {
        self.log.mint_surrogate(&Self::actor(kind, raw)?, self.server_key)
    }

    fn lookup(&self, kind: ActorKind, raw: &str) -> io::Result<Option<u64>> {
        self.log.surrogate_of(&Self::actor(kind, raw)?)
    }

    fn raw(&self, surrogate: u64) -> io::Result<Option<(ActorKind, String)>> {
        Ok(self.log.surrogates()?.into_iter()
            .find(|(n, _)| *n == surrogate)
            .and_then(|(_, actor)| actor)
            .map(|actor| (ActorKind::Federated, actor.into_string())))
    }
}

fn now() -> Timestamp {
    Timestamp(u64::try_from(chrono::Utc::now().timestamp_micros()).unwrap_or(0))
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
        let mutable_partition = fresh_partition_id();
        for graph in registry.graphs() {
            let entry = match (graph.kind, graph.name.as_str()) {
                (Kind::Source, GRAPH_SERVER) => GraphEntry::source(graph, graph.name.clone(), SERVER_PARTITION, SEGMENT_EXPONENT),
                (Kind::Source, GRAPH_CLAIM) => GraphEntry::source(graph, graph.name.clone(), claim_partition, SEGMENT_EXPONENT),
                (Kind::Source, GRAPH_MUTABLE) => GraphEntry::source(graph, graph.name.clone(), mutable_partition, SEGMENT_EXPONENT),
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
        // Every source row was given a partition above, so this fails only on a registry
        // that lacks one of them
        for name in [GRAPH_SERVER, GRAPH_CLAIM, GRAPH_MUTABLE] {
            row(name)?;
        }
        block_on(store.create_partition(claim_partition, segments)).map_err(other)?;
        block_on(store.create_partition(mutable_partition, segments)).map_err(other)?;

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
        Self::load(
            store,
            &instance.log_dir,
            &|name| Ok(origin_of(instance, name)),
            Some(instance.server.clone()),
            Some(instance.domain.clone()),
        )
    }

    /// Opens a bundle that [`ClaimLog::export`] wrote, to read and verify it. Nothing is
    /// needed but the bundle: the origins of its partitions are in its manifest, and the
    /// server's keys and IRI are in its server partition.
    ///
    /// A bundle written for publication has no mutable partition, so it holds no actors
    /// and no keys of theirs.
    pub fn open_bundle(dir: impl AsRef<Path>) -> io::Result<Self> {
        let dir = dir.as_ref();
        // Opening checks the bundle's key chain, and each partition against the size and
        // tree head the manifest states for it
        let bundle = Bundle::open(dir).map_err(invalid)?;
        let manifest = bundle.manifest().clone();
        drop(bundle);
        let store = SegmentStore::open(dir).map_err(other)?;
        if !block_on(store.partitions()).map_err(other)?.contains(&SERVER_PARTITION) {
            return Err(invalid(format!("{} holds no server partition", dir.display())));
        }
        let origin = |name: &str| -> io::Result<String> {
            manifest.partitions.iter()
                .find(|partition| partition.name == name)
                .map(|partition| partition.origin.clone())
                .ok_or_else(|| invalid(format!("the bundle does not say the origin of its {name} partition")))
        };
        Self::load(store, dir, &origin, None, None)
    }

    /// Reads the server partition of a store and makes a log of it.
    fn load(
        store: SegmentStore,
        dir: &Path,
        origin: &dyn Fn(&str) -> io::Result<String>,
        server: Option<NamedNode>,
        domain: Option<String>,
    ) -> io::Result<Self> {
        let server_records = records(&store, SERVER_PARTITION)?;
        let chain = KeyChain::from_records(&server_records).map_err(invalid)?;
        if chain.current().is_none() {
            return Err(invalid("the server partition registers no key"));
        }

        // A partition is the one the latest `graph:` record of its name gives
        let partition_of = |name: &str| -> io::Result<u64> {
            let key = format!("graph:{name}");
            server_records.iter().rev()
                .filter(|record| record.header().payload_type == PAYLOAD_CONFIG && record.header().key.as_deref() == Some(key.as_str()))
                .find_map(|record| GraphEntry::from_value(record.body().content().value().ok()??).ok()?.partition)
                .ok_or_else(|| invalid(format!("the server partition registers no {name} partition")))
        };
        let founding = server_records.first().ok_or_else(|| invalid("the server partition is empty"))?;
        // The server's IRI, if it was not given, is the actor its attestations name
        let server = match server {
            Some(server) => server,
            None => server_records.iter()
                .find_map(|record| {
                    let attestation = record.body().attestation().value().ok()??;
                    NamedNode::new(attestation.get("actor")?.as_text()?).ok()
                })
                .ok_or_else(|| invalid("the server partition does not say who the server is"))?,
        };

        let mutable = partition_of(GRAPH_MUTABLE)?;
        let has_mutable = block_on(store.partitions()).map_err(other)?.contains(&mutable);
        Ok(Self {
            partition: partition_of(GRAPH_CLAIM)?,
            mutable,
            has_mutable,
            authority: Authority { partition: SERVER_PARTITION, offset: 0, leaf: founding.leaf() },
            store,
            checkpoints: FileCheckpoints::new(dir),
            origin: origin(GRAPH_CLAIM)?,
            mutable_origin: if has_mutable { origin(GRAPH_MUTABLE)? } else { String::new() },
            server_origin: origin(GRAPH_SERVER)?,
            chain,
            server,
            domain,
        })
    }

    /// The IRI the server claims and attests as.
    pub fn server(&self) -> &NamedNode {
        &self.server
    }

    /// Writes a bundle of the log to `dest`, which must not exist or must be empty: a
    /// directory that holds partitions of the log with every record, checkpoint and key,
    /// and that [`ClaimLog::open_bundle`] reads and verifies with nothing else.
    ///
    /// A bundle for publication holds the server and claim partitions, which is everything
    /// needed to check every claim. A backup also holds the mutable partition, which says
    /// who the actors are and is not for publication, and can take the log's place.
    /// Returns the names of the partitions written.
    pub fn export(&self, dest: &Path, backup: bool) -> io::Result<Vec<String>> {
        let domain = self.domain.as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "a bundle is exported from an instance's log, not from another bundle"))?;
        let mut names = vec![GRAPH_SERVER, GRAPH_CLAIM];
        if backup {
            names.push(GRAPH_MUTABLE);
        }
        let entries = self.graph_entries()?;
        let mut partitions = Vec::new();
        for name in &names {
            let entry = entries.iter().find(|entry| entry.name == *name)
                .ok_or_else(|| invalid(format!("the server partition registers no {name} graph")))?;
            let lower = |policy: String| policy.to_lowercase();
            partitions.push(BundlePartition {
                partition: entry.partition.ok_or_else(|| invalid(format!("the {name} graph has no partition")))?,
                name: name.to_string(),
                origin: match *name {
                    GRAPH_SERVER => self.server_origin.clone(),
                    GRAPH_CLAIM => self.origin.clone(),
                    _ => self.mutable_origin.clone(),
                },
                // A backup carries each partition's registry row, to be restored from
                registry: backup.then(|| PartitionRegistry {
                    tenant: None,
                    graph_iri: format!("{}/graph/{name}", domain.trim_end_matches('/')),
                    history: lower(entry.history.map(|h| format!("{h:?}")).unwrap_or_default()),
                    integrity: lower(entry.integrity.map(|i| format!("{i:?}")).unwrap_or_default()),
                    export: lower(format!("{:?}", entry.export)),
                    hash: entry.hash.clone().unwrap_or_default(),
                }),
                head: None,
            });
        }
        let key_records: Vec<Record> = records(&self.store, SERVER_PARTITION)?.into_iter()
            .filter(KeyChain::is_key_record)
            .collect();
        let created = now().as_micros();
        let scope = if backup { "backup" } else { "export" };
        block_on(scatter_integrity::bundle::export_scoped(
            &self.store, &self.checkpoints, &partitions, &key_records, created, dest, scope, &Default::default(),
        )).map_err(other)?;
        Ok(names.into_iter().map(str::to_string).collect())
    }

    /// The `graph:` entries the server partition registers, the latest for each name.
    fn graph_entries(&self) -> io::Result<Vec<GraphEntry>> {
        let mut entries: Vec<GraphEntry> = Vec::new();
        for record in records(&self.store, SERVER_PARTITION)? {
            let is_graph = record.header().key.as_deref().is_some_and(|key| key.starts_with("graph:"));
            if record.header().payload_type != PAYLOAD_CONFIG || !is_graph {
                continue;
            }
            let content = record.body().content().value().map_err(invalid)?
                .ok_or_else(|| invalid("a graph record was erased"))?;
            let entry = GraphEntry::from_value(content).map_err(invalid)?;
            entries.retain(|held| held.name != entry.name);
            entries.push(entry);
        }
        Ok(entries)
    }

    /// The size of the claim partition, which is the offset the next claim takes.
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

    /// Every key the server has had, oldest first: the chain of the server partition's
    /// `key:` records. The last is the server's current key.
    pub fn server_keys(&self) -> Vec<(String, VerifyingKey)> {
        self.chain.keys().iter()
            .filter_map(|key| Some((key.entry.key_id.clone(), VerifyingKey::from_bytes(&key.public_key()?).ok()?)))
            .collect()
    }

    /// Replaces the server's key: appends a `key:` record that registers `new`, signed by
    /// the current key, which the caller must hold as `current`.
    ///
    /// From then on `new` is the server's key: it attests records, signs checkpoints and
    /// accepts claims, and claims by the server are checked against it. What the earlier
    /// keys signed stays valid, since every key of the chain is known to have been the
    /// server's.
    pub fn rotate_server_key(&mut self, new: &SigningKey, current: &SigningKey) -> io::Result<()> {
        self.require_server_key(current)?;
        let entry = KeyEntry::ed25519(new.verifying_key().to_bytes());
        if self.chain.get(&entry.key_id).is_some() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "that key has been the server's before"));
        }
        let content = entry.to_value();
        let comment = Value::text("rotation");
        let current_id = scatter_log::registry::key_id(&current.verifying_key().to_bytes());
        let attestation = Value::map(vec![
            (Value::text("actor"), Value::text(self.server.as_str())),
            (Value::text("signature"), client_signature(
                current,
                &current_id,
                &scatter_log::cbor::encode(&content).map_err(other)?,
                &scatter_log::cbor::encode(&comment).map_err(other)?,
            )),
        ]);
        let draft = Draft {
            appended_at: now().as_micros(),
            payload_type: PAYLOAD_CONFIG.into(),
            key: Some(entry.key()),
            revid: None,
            logid: None,
            page_id: None,
            body: Body::core(&content, &comment, &attestation).map_err(other)?,
        };
        block_on(self.store.append(SERVER_PARTITION, draft)).map_err(other)?;
        self.chain = KeyChain::from_records(&records(&self.store, SERVER_PARTITION)?).map_err(invalid)?;
        self.checkpoint(SERVER_PARTITION, new)
    }

    /// The graphs the server partition registers, by name, each with its partition if it is
    /// a source.
    pub fn graphs(&self) -> io::Result<Vec<(String, Option<u64>)>> {
        Ok(self.graph_entries()?.into_iter().map(|entry| (entry.name, entry.partition)).collect())
    }

    fn require_server_key(&self, server_key: &SigningKey) -> io::Result<()> {
        if server_key.verifying_key() != self.server_key() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the signing key is not the key the server partition registers",
            ));
        }
        Ok(())
    }

    /// Appends a record to the mutable partition, attested by the server, and signs a
    /// checkpoint.
    fn append_mutable(&mut self, payload_type: &str, key: String, content: &Value, at: Timestamp, server_key: &SigningKey) -> io::Result<()> {
        self.require_server_key(server_key)?;
        let comment = Value::Null;
        let attestation = instance_attestation(server_key, self.server.as_str(), &self.authority, None, false, content, &comment)
            .map_err(|_| invalid("a record of the mutable partition does not encode"))?;
        let draft = Draft {
            appended_at: at.as_micros(),
            payload_type: payload_type.into(),
            key: Some(key),
            revid: None,
            logid: None,
            page_id: None,
            body: Body::core(content, &comment, &attestation).map_err(other)?,
        };
        block_on(self.store.append(self.mutable, draft)).map_err(other)?;
        let checkpoint = block_on(scatter_integrity::checkpoints::checkpoint(
            &self.store, self.mutable, &self.mutable_origin, server_key,
        )).map_err(other)?;
        block_on(self.checkpoints.put(self.mutable, &checkpoint)).map_err(other)?;
        Ok(())
    }

    /// Every surrogate minted, in order, each with the actor it stands for, or `None` if
    /// its actor record was erased.
    pub fn surrogates(&self) -> io::Result<Vec<(u64, Option<NamedNode>)>> {
        let mut surrogates: Vec<(u64, Option<NamedNode>)> = Vec::new();
        if !self.has_mutable {
            return Ok(surrogates);
        }
        for record in records(&self.store, self.mutable)? {
            if record.header().payload_type != PAYLOAD_ACTOR {
                continue;
            }
            let offset = record.header().offset;
            let bad = |what: String| invalid(format!("the mutable partition, offset {offset}: {what}"));
            let surrogate = record.header().key.as_deref()
                .and_then(surrogate_of_key)
                .ok_or_else(|| bad("the actor record is not keyed by a surrogate".into()))?;
            let actor = match record.body().content().value().map_err(|e| bad(e.to_string()))? {
                // Erased: the number stays taken and stands for nobody
                None => None,
                Some(content) => {
                    let actor: ActorRecord = scatter_log::cbor::from_value(content).map_err(|e| bad(e.to_string()))?;
                    actor.validate(&ActorKey::surrogate(surrogate)).map_err(|e| bad(e.to_string()))?;
                    let iri = actor.raw.as_deref().and_then(|raw| NamedNode::new(raw).ok())
                        .ok_or_else(|| bad("the actor record does not hold an IRI".into()))?;
                    Some(iri)
                },
            };
            // A later record for a surrogate replaces the earlier one
            match surrogates.iter_mut().find(|(n, _)| *n == surrogate) {
                Some(entry) => entry.1 = actor,
                None => surrogates.push((surrogate, actor)),
            }
        }
        Ok(surrogates)
    }

    /// The surrogate that stands for an actor, if one has been minted for it.
    pub fn surrogate_of(&self, actor: &NamedNode) -> io::Result<Option<u64>> {
        Ok(self.surrogates()?.into_iter().find(|(_, of)| of.as_ref() == Some(actor)).map(|(n, _)| n))
    }

    /// The surrogate that stands for an actor, minted and recorded if it has none.
    /// Surrogates are minted in sequence from 1 and a number is never used twice.
    pub fn mint_surrogate(&mut self, actor: &NamedNode, server_key: &SigningKey) -> io::Result<u64> {
        let surrogates = self.surrogates()?;
        if let Some((n, _)) = surrogates.iter().find(|(_, of)| of.as_ref() == Some(actor)) {
            return Ok(*n);
        }
        let surrogate = surrogates.iter().map(|(n, _)| *n).max().unwrap_or(0) + 1;
        let key = ActorKey::surrogate(surrogate);
        let record = ActorRecord::surrogate(ActorKind::Federated, actor.as_str());
        record.validate(&key).map_err(invalid)?;
        let content = scatter_log::cbor::to_value(&record).map_err(other)?;
        self.append_mutable(PAYLOAD_ACTOR, key.to_string(), &content, now(), server_key)?;
        Ok(surrogate)
    }

    /// Registers a public key for an actor, valid from now. Returns its key record.
    pub fn register_key(&mut self, actor: &NamedNode, key: &VerifyingKey, server_key: &SigningKey) -> io::Result<KeyRecord> {
        self.register_key_until(actor, key, None, server_key)
    }

    /// Registers a public key for an actor, valid from now until `valid_until` if that is
    /// given. Returns its key record.
    pub fn register_key_until(&mut self, actor: &NamedNode, key: &VerifyingKey, valid_until: Option<Timestamp>, server_key: &SigningKey) -> io::Result<KeyRecord> {
        let now = now();
        if valid_until.is_some_and(|until| until <= now) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the key would already have expired"));
        }
        let surrogate = self.mint_surrogate(actor, server_key)?;
        let record = KeyRecord::register(PublicKey(key.to_bytes()), now, valid_until).map_err(invalid)?;
        let content = scatter_log::cbor::to_value(&record).map_err(other)?;
        self.append_mutable(PAYLOAD_KEY, ActorKey::surrogate(surrogate).to_string(), &content, now, server_key)?;
        Ok(record)
    }

    /// Revokes every key an actor has. Returns the records that were revoked.
    pub fn revoke_keys(&mut self, actor: &NamedNode, server_key: &SigningKey) -> io::Result<Vec<KeyRecord>> {
        self.revoke(actor, None, server_key)
    }

    /// Revokes the key of an actor's with a key ID. Returns its record, or `None` if the
    /// actor has no such key now.
    pub fn revoke_key(&mut self, actor: &NamedNode, key_id: &str, server_key: &SigningKey) -> io::Result<Option<KeyRecord>> {
        Ok(self.revoke(actor, Some(key_id), server_key)?.pop())
    }

    fn revoke(&mut self, actor: &NamedNode, key_id: Option<&str>, server_key: &SigningKey) -> io::Result<Vec<KeyRecord>> {
        let Some(surrogate) = self.surrogate_of(actor)? else {
            return Ok(Vec::new());
        };
        let records = self.key_records(actor)?;
        let revoked: Vec<KeyRecord> = current_keys(&records).into_iter()
            .filter(|record| key_id.is_none_or(|id| record.key_id == id))
            .cloned()
            .collect();
        for record in &revoked {
            let content = scatter_log::cbor::to_value(&record.revoke()).map_err(other)?;
            self.append_mutable(PAYLOAD_KEY, ActorKey::surrogate(surrogate).to_string(), &content, now(), server_key)?;
        }
        Ok(revoked)
    }

    /// Every key record of the mutable partition, in log order, each with its actor. The
    /// key records of a surrogate whose actor record was erased belong to nobody and are
    /// left out.
    pub fn all_key_records(&self) -> io::Result<Vec<(NamedNode, KeyRecord)>> {
        let surrogates = self.surrogates()?;
        let mut keys = Vec::new();
        if !self.has_mutable {
            return Ok(keys);
        }
        for record in records(&self.store, self.mutable)? {
            if record.header().payload_type != PAYLOAD_KEY {
                continue;
            }
            let offset = record.header().offset;
            let bad = |what: String| invalid(format!("the mutable partition, offset {offset}: {what}"));
            let surrogate = record.header().key.as_deref()
                .and_then(surrogate_of_key)
                .ok_or_else(|| bad("the key record is not keyed by a surrogate".into()))?;
            let actor = surrogates.iter().find(|(n, _)| *n == surrogate)
                .ok_or_else(|| bad(format!("no actor record for actor:{surrogate}")))?;
            let Some(actor) = &actor.1 else { continue };
            let content = record.body().content().value().map_err(|e| bad(e.to_string()))?
                .ok_or_else(|| bad("the key record was erased".into()))?;
            let key: KeyRecord = scatter_log::cbor::from_value(content).map_err(|e| bad(e.to_string()))?;
            key.validate().map_err(|e| bad(e.to_string()))?;
            keys.push((actor.clone(), key));
        }
        Ok(keys)
    }

    /// The key records of one actor, in log order.
    pub fn key_records(&self, actor: &NamedNode) -> io::Result<Vec<KeyRecord>> {
        Ok(self.all_key_records()?.into_iter().filter(|(of, _)| of == actor).map(|(_, record)| record).collect())
    }

    /// The keys an actor has now: registered and not revoked.
    pub fn current_keys(&self, actor: &NamedNode) -> io::Result<Vec<KeyRecord>> {
        Ok(current_keys(&self.key_records(actor)?).into_iter().cloned().collect())
    }

    /// Appends an accepted claim and signs a checkpoint of the new head. Returns the offset.
    ///
    /// The claim's sequence ID must be the offset it takes, so that the server's signature
    /// commits to the claim's place in the log, and `server_key` must be the server's
    /// current key.
    pub fn append(&mut self, signed: &SignedClaim, actor_key: &VerifyingKey, server_key: &SigningKey) -> io::Result<u64> {
        self.require_server_key(server_key)?;
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

    /// The claims from offset `from` of the claim partition to its head. The records of
    /// erasures, which share the partition, are passed over.
    pub fn claims(&self, from: u64) -> io::Result<Vec<StoredClaim>> {
        let size = self.size()?;
        let mut claims = Vec::new();
        for offset in from..size {
            let slot = block_on(self.store.read(self.partition, offset)).map_err(other)?;
            let Slot::Record(record) = slot else {
                return Err(corrupt(offset, "the record was compacted"));
            };
            if record.header().payload_type == PAYLOAD_CLAIM {
                claims.push(claim_of(&record).map_err(|e| corrupt(offset, e))?);
            }
        }
        Ok(claims)
    }

    /// The claim with an ID, if the log holds it.
    pub fn claim(&self, id: ClaimId) -> io::Result<Option<StoredClaim>> {
        Ok(self.claims(0)?.into_iter().find(|stored| stored.claim.acceptance.claim == id))
    }

    /// Appends the record of an erasure to a partition, attested by the server.
    fn append_erasure(&mut self, partition: u64, key: Option<String>, targets: Value, parts: Option<Vec<&str>>, reason: Reason<'_>, server_key: &SigningKey) -> io::Result<()> {
        self.require_server_key(server_key)?;
        if !REASON_CLASSES.contains(&reason.class) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{}` is not a reason for an erasure: use one of {}", reason.class, REASON_CLASSES.join(", ")),
            ));
        }
        let mut pairs = vec![
            (Value::text("targets"), targets),
            (Value::text("reason"), Value::text(reason.class)),
            (Value::text("authority"), Value::text(reason.authority)),
        ];
        if let Some(parts) = parts {
            pairs.push((Value::text("parts"), Value::Array(parts.into_iter().map(Value::text).collect())));
        }
        let content = Value::map(pairs);
        let comment = reason.comment.map_or(Value::Null, Value::text);
        let attestation = instance_attestation(server_key, self.server.as_str(), &self.authority, None, false, &content, &comment)
            .map_err(|_| invalid("an erase record does not encode"))?;
        let draft = Draft {
            appended_at: now().as_micros(),
            payload_type: PAYLOAD_ERASE.into(),
            key,
            revid: None,
            logid: None,
            page_id: None,
            body: Body::core(&content, &comment, &attestation).map_err(other)?,
        };
        block_on(self.store.append(partition, draft)).map_err(other)?;
        Ok(())
    }

    fn checkpoint(&mut self, partition: u64, server_key: &SigningKey) -> io::Result<()> {
        let origin = if partition == SERVER_PARTITION {
            self.server_origin.clone()
        } else if partition == self.partition {
            self.origin.clone()
        } else {
            self.mutable_origin.clone()
        };
        let checkpoint = block_on(scatter_integrity::checkpoints::checkpoint(&self.store, partition, &origin, server_key)).map_err(other)?;
        block_on(self.checkpoints.put(partition, &checkpoint)).map_err(other)?;
        Ok(())
    }

    /// Erases parts of a claim, for good. Returns the parts that were erased by this call:
    /// those asked for that the record still had.
    ///
    /// The erasure is recorded first, as an erase record in the claim partition. The
    /// claim's header and the server's acceptance are kept ([`ClaimPart`]).
    pub fn erase_claim(&mut self, id: ClaimId, parts: &[ClaimPart], reason: Reason<'_>, server_key: &SigningKey) -> io::Result<Vec<ClaimPart>> {
        let stored = self.claim(id)?.ok_or_else(|| io::Error::new(
            io::ErrorKind::NotFound,
            format!("the log holds no claim {id}"),
        ))?;
        let already = stored.erased();
        let mut parts: Vec<ClaimPart> = parts.iter().copied().filter(|part| !already.contains(part)).collect();
        parts.sort();
        parts.dedup();
        if parts.is_empty() {
            return Ok(parts);
        }

        let targets = Value::map(vec![(Value::text("offsets"), Value::Array(vec![Value::Int(i128::from(stored.offset))]))]);
        self.append_erasure(self.partition, None, targets, Some(parts.iter().map(|part| part.name()).collect()), reason, server_key)?;
        let indices: Vec<usize> = parts.iter().map(|part| part.index()).collect();
        block_on(self.store.erase_parts(self.partition, stored.offset, &indices)).map_err(other)?;
        self.checkpoint(self.partition, server_key)?;
        Ok(parts)
    }

    /// Erases an actor from the mutable partition, for good: its actor record and every key
    /// record under its surrogate. Returns the surrogate, which stays taken and now stands
    /// for nobody, or `None` if the actor has no surrogate.
    ///
    /// The actor then has no keys, and the claims it made are not touched: their
    /// attestations still name it until they are erased ([`ClaimLog::erase_claim`]).
    pub fn erase_actor(&mut self, actor: &NamedNode, reason: Reason<'_>, server_key: &SigningKey) -> io::Result<Option<u64>> {
        let Some(surrogate) = self.surrogate_of(actor)? else {
            return Ok(None);
        };
        let key = ActorKey::surrogate(surrogate).to_string();
        let offsets: Vec<u64> = records(&self.store, self.mutable)?.iter()
            .filter(|record| record.header().key.as_deref() == Some(key.as_str()) && record.header().payload_type != PAYLOAD_ERASE)
            .map(|record| record.header().offset)
            .collect();

        let targets = Value::map(vec![(Value::text("key"), Value::text(&key))]);
        self.append_erasure(self.mutable, Some(key), targets, None, reason, server_key)?;
        for offset in offsets {
            block_on(self.store.erase_parts(self.mutable, offset, &[0, 1, 2])).map_err(other)?;
        }
        self.checkpoint(self.mutable, server_key)?;
        Ok(Some(surrogate))
    }

    /// Verifies the whole log and returns the number of claims in it.
    ///
    /// `server_key` is the key the caller expects the server to have, and must be the
    /// current key of the server partition's chain. Every partition is then checked by
    /// [`scatter_integrity::verify`]: headers, body commitments, the Merkle tree, the signed
    /// checkpoints, that every erased part is covered by an erase record, and in the server
    /// partition the founding record, the key chain and every attestation.
    ///
    /// Every claim must also read back as the claim its key names at the sequence ID of its
    /// offset, with an acceptance signed by a key of the server's. A claim with nothing
    /// erased must carry an actor's signature that verifies, under a key that was
    /// registered for the actor, unless an actor has been erased and with it the knowledge
    /// of whose its keys were, or the log is a bundle without the mutable partition.
    pub fn verify(&self, server_key: &VerifyingKey) -> io::Result<u64> {
        if *server_key != self.server_key() {
            return Err(invalid("the server partition's current key is not the expected server key"));
        }
        let part_names = |payload_type: &str| -> Vec<String> {
            if payload_type == PAYLOAD_CLAIM {
                CLAIM_PART_NAMES.iter().map(|name| name.to_string()).collect()
            } else {
                Vec::new()
            }
        };
        let mut partitions = vec![
            (GRAPH_SERVER, SERVER_PARTITION, &self.server_origin, true),
            (GRAPH_CLAIM, self.partition, &self.origin, false),
        ];
        if self.has_mutable {
            partitions.push((GRAPH_MUTABLE, self.mutable, &self.mutable_origin, false));
        }
        for (name, partition, origin, is_config) in partitions {
            let report = block_on(scatter_integrity::verify(&self.store, &self.checkpoints, &Verify {
                partition,
                origin,
                chain: &self.chain,
                level: Level::Bodies,
                actor_keys: None,
                part_names: Some(&part_names),
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

        let key_records = self.all_key_records()?;
        // Without the mutable partition, as in a bundle for publication, nothing says whose
        // a key was either
        let any_erased = !self.has_mutable || self.surrogates()?.iter().any(|(_, actor)| actor.is_none());
        let claims = self.claims(0)?;
        for stored in &claims {
            let offset = stored.offset;
            if self.chain.lookup(&scatter_log::registry::key_id(stored.server_key.as_bytes())).is_none() {
                return Err(corrupt(offset, "accepted under a key that is not the server's"));
            }
            stored.claim.acceptance.verify(&stored.server_key).map_err(|e| corrupt(offset, e))?;

            let Some(signed) = stored.claim.whole().map_err(|e| corrupt(offset, e))? else { continue };
            let actor_key = stored.actor_key.ok_or_else(|| corrupt(offset, "no key for the actor's signature"))?;
            // The key the actor's signature was checked against is one of the server's for
            // the server, and for anyone else one the server registered for that actor.
            // Once any actor has been erased, its key records say no longer whose they
            // were, so a key that is not found may have been one of those.
            let attribution = signed.envelope().claim().attribution();
            let known = if *attribution == self.server {
                self.chain.lookup(&scatter_log::registry::key_id(actor_key.as_bytes())).is_some()
            } else {
                any_erased
                    || key_records.iter().any(|(actor, record)| actor == attribution && record.public_key.0 == actor_key.to_bytes())
            };
            if !known {
                return Err(corrupt(offset, format!("signed with a key that was never registered for {attribution}")));
            }
            signed.envelope().verify(&actor_key).map_err(|e| corrupt(offset, e))?;
        }
        Ok(claims.len() as u64)
    }
}

fn bytes(bytes: &[u8]) -> Value {
    Value::Bytes(bytes.to_vec())
}

/// The body of a claim record: the five parts of the module's table.
fn body_of(signed: &SignedClaim, actor_key: &VerifyingKey, server_key: &VerifyingKey) -> io::Result<Body> {
    let envelope = signed.envelope();
    let claim = envelope.claim();
    let content = Value::map(vec![
        (Value::text("previous"), bytes(claim.previous().as_bytes())),
        (Value::text("triples"), Value::Array(claim.triples().iter().map(|t| Value::Text(ntriples_line(t))).collect())),
    ]);
    let attestation = Value::map(vec![
        (Value::text("actor"), Value::text(claim.attribution().as_str())),
        (Value::text("alg"), Value::text("ed25519")),
        (Value::text("key"), bytes(actor_key.as_bytes())),
        (Value::text("sig"), bytes(envelope.signature().as_bytes())),
    ]);
    let acknowledgments = Value::Array(envelope.acknowledgments().iter()
        .map(|(term, prior)| Value::Array(vec![Value::text(term.as_str()), bytes(prior.as_bytes())]))
        .collect());
    let acceptance = signed.acceptance();
    let acceptance = Value::map(vec![
        (Value::text("alg"), Value::text("ed25519")),
        (Value::text("key"), bytes(server_key.as_bytes())),
        (Value::text("sig"), bytes(acceptance.signature.as_bytes())),
        (Value::text("client"), Value::text(&acceptance.client_signature)),
        (Value::text("sequence"), Value::text(&acceptance.sequence_id)),
        (Value::text("timestamp"), Value::text(&acceptance.timestamp)),
    ]);

    let mut body = Body::core(&content, &Value::Null, &attestation).map_err(other)?;
    body.push(Part::of(&acknowledgments).map_err(other)?).map_err(other)?;
    body.push(Part::of(&acceptance).map_err(other)?).map_err(other)?;
    Ok(body)
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

/// Reads a claim record back, less any part that was erased. Signatures are not checked.
fn claim_of(record: &Record) -> Result<StoredClaim, String> {
    let header = record.header();
    if header.payload_type != PAYLOAD_CLAIM {
        return Err(format!("a {} record, not a claim", header.payload_type));
    }
    let id: ClaimId = header.key.as_deref()
        .and_then(|key| key.parse().ok())
        .ok_or("the record's key is not a claim ID")?;
    // A part's value, or `None` if the part was erased
    let part = |index: usize, name: &str| -> Result<Option<Value>, String> {
        record.body().part(index)
            .ok_or_else(|| format!("no {name} part"))?
            .value().map_err(|e| format!("the {name} part: {e}"))
    };

    let claim = match part(ClaimPart::Content.index(), "content")? {
        None => None,
        Some(content) => {
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
            Some((triples, previous))
        },
    };

    let (attestation, actor_key) = match part(ClaimPart::Attestation.index(), "attestation")? {
        None => (None, None),
        Some(attestation) => {
            let actor = NamedNode::new(text(attestation.get("actor"), "the actor")?)
                .map_err(|e| format!("the actor: {e}"))?;
            let (signature, key) = signature_of(&attestation, "the actor's signature")?;
            (Some((actor, signature)), Some(key))
        },
    };

    let acknowledgments = match part(PART_ACKNOWLEDGMENTS, "acknowledgments")? {
        None => None,
        Some(Value::Array(pairs)) => {
            let mut acknowledgments = Acknowledgments::new();
            for pair in &pairs {
                let Value::Array(pair) = pair else { return Err("an acknowledgment is not a pair".into()) };
                let term = NamedNode::new(text(pair.first(), "an acknowledged term")?)
                    .map_err(|e| format!("an acknowledged term: {e}"))?;
                acknowledgments.insert(term, ClaimId::from_bytes(array::<32>(pair.get(1), "a prior claim")?));
            }
            Some(acknowledgments)
        },
        Some(_) => return Err("the acknowledgments are not an array".into()),
    };

    let acceptance = part(PART_ACCEPTANCE, "acceptance")?.ok_or("the acceptance was erased")?;
    let (server_signature, server_key) = signature_of(&acceptance, "the server's signature")?;
    let sequence_id = text(acceptance.get("sequence"), "the sequence ID")?;
    if sequence_id != header.offset.to_string() {
        return Err(format!("the claim has sequence ID {sequence_id}"));
    }
    let acceptance = Acceptance {
        claim: id,
        client_signature: text(acceptance.get("client"), "the digest of the actor's signature")?.to_string(),
        sequence_id: sequence_id.to_string(),
        timestamp: text(acceptance.get("timestamp"), "the timestamp")?.to_string(),
        signature: server_signature,
    };

    // What is there must be what the header and the acceptance name
    if let Some((_, signature)) = &attestation {
        if signature.digest() != acceptance.client_signature {
            return Err("the actor's signature is not the one the server accepted".into());
        }
    }
    if let (Some((triples, previous)), Some((actor, _))) = (&claim, &attestation) {
        let made = Claim::new(triples.clone(), actor.clone(), *previous).map_err(|e| e.to_string())?.id();
        if made != id {
            return Err(format!("the record's key is not its claim's ID, {made}"));
        }
    }
    let claim = Redacted { acceptance, claim, attestation, acknowledgments };
    claim.whole().map_err(|e| e.to_string())?;
    Ok(StoredClaim { offset: header.offset, claim, actor_key, server_key })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys;

    fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    fn log() -> (tempfile::TempDir, ClaimLog, SigningKey) {
        let dir = tempfile::tempdir().unwrap();
        let instance = Instance::new("http://example.org", iri("http://example.org/Server"), dir.path().join("store.nq").to_str().unwrap());
        let server_key = keys::generate();
        let log = ClaimLog::create(&instance, &server_key).unwrap();
        (dir, log, server_key)
    }

    #[test]
    fn actors_are_known_in_headers_by_surrogates_alone() {
        let (_dir, mut log, server_key) = log();
        let alice = iri("http://example.org/alice");
        let bob = iri("http://example.org/bob");

        let alice_key = keys::generate().verifying_key();
        log.register_key(&alice, &alice_key, &server_key).unwrap();
        log.register_key(&bob, &keys::generate().verifying_key(), &server_key).unwrap();
        log.register_key(&alice, &keys::generate().verifying_key(), &server_key).unwrap();

        // One surrogate for each actor, minted in sequence, however many keys it has
        assert_eq!(log.surrogates().unwrap(), [(1, Some(alice.clone())), (2, Some(bob.clone()))]);
        assert_eq!(log.surrogate_of(&alice).unwrap(), Some(1));
        assert_eq!(log.surrogate_of(&iri("http://example.org/carol")).unwrap(), None);
        assert_eq!(log.key_records(&alice).unwrap().len(), 2);
        assert_eq!(log.key_records(&alice).unwrap()[0].public_key.0, alice_key.to_bytes());

        // An actor record and then its key record, each keyed by the surrogate
        let headers: Vec<(String, String)> = records(&log.store, log.mutable).unwrap().iter()
            .map(|record| (record.header().payload_type.clone(), record.header().key.clone().unwrap()))
            .collect();
        assert_eq!(headers, [
            (PAYLOAD_ACTOR.to_string(), "actor:1".to_string()),
            (PAYLOAD_KEY.to_string(), "actor:1".to_string()),
            (PAYLOAD_ACTOR.to_string(), "actor:2".to_string()),
            (PAYLOAD_KEY.to_string(), "actor:2".to_string()),
            (PAYLOAD_KEY.to_string(), "actor:1".to_string()),
        ]);
        for record in records(&log.store, log.mutable).unwrap() {
            assert!(!String::from_utf8_lossy(&record.header().encode()).contains("example.org"));
        }
        assert_eq!(log.verify(&server_key.verifying_key()).unwrap(), 0);
    }

    #[test]
    fn the_servers_key_is_rotated_by_a_record_the_old_key_signs() {
        let (_dir, mut log, first) = log();
        let alice = iri("http://example.org/alice");
        log.register_key(&alice, &keys::generate().verifying_key(), &first).unwrap();
        let second = keys::generate();

        // Only the current key can hand over, and not to a key the server had before
        let error = log.rotate_server_key(&second, &keys::generate()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(log.rotate_server_key(&first, &first).is_err());

        log.rotate_server_key(&second, &first).unwrap();
        assert_eq!(log.server_key(), second.verifying_key());
        assert_eq!(log.server_keys().iter().map(|(_, key)| *key).collect::<Vec<_>>(), [first.verifying_key(), second.verifying_key()]);

        // The old key writes no more; the new one does
        let bob = iri("http://example.org/bob");
        let error = log.register_key(&bob, &keys::generate().verifying_key(), &first).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        log.register_key(&bob, &keys::generate().verifying_key(), &second).unwrap();

        // What each key attested and checkpointed verifies under the chain, and the log
        // answers to its current key alone
        assert_eq!(log.verify(&second.verifying_key()).unwrap(), 0);
        assert!(log.verify(&first.verifying_key()).is_err());

        // A second rotation extends the chain
        let third = keys::generate();
        log.rotate_server_key(&third, &second).unwrap();
        assert_eq!(log.server_keys().len(), 3);
        assert_eq!(log.verify(&third.verifying_key()).unwrap(), 0);
    }

    #[test]
    fn keys_are_revoked_one_at_a_time_and_can_expire() {
        let (_dir, mut log, server_key) = log();
        let alice = iri("http://example.org/alice");
        let kept = log.register_key(&alice, &keys::generate().verifying_key(), &server_key).unwrap();
        let dropped = log.register_key(&alice, &keys::generate().verifying_key(), &server_key).unwrap();

        assert_eq!(log.revoke_key(&alice, "no such key", &server_key).unwrap(), None);
        assert_eq!(log.revoke_key(&alice, &dropped.key_id, &server_key).unwrap(), Some(dropped.clone()));
        assert_eq!(log.revoke_key(&alice, &dropped.key_id, &server_key).unwrap(), None);
        assert_eq!(log.current_keys(&alice).unwrap(), [kept.clone()]);

        // A key that expires is valid until then, and one already expired is not registered
        let soon = Timestamp(now().as_micros() + 3_600_000_000);
        let expiring = log.register_key_until(&alice, &keys::generate().verifying_key(), Some(soon), &server_key).unwrap();
        assert!(expiring.is_valid_at(now()));
        assert!(!expiring.is_valid_at(Timestamp(soon.as_micros() + 1)));
        assert!(log.register_key_until(&alice, &keys::generate().verifying_key(), Some(Timestamp(1)), &server_key).is_err());
        assert_eq!(log.current_keys(&alice).unwrap().len(), 2);
        assert_eq!(log.verify(&server_key.verifying_key()).unwrap(), 0);
    }

    #[test]
    fn surrogates_answer_the_scatter_actors_contract() {
        let (_dir, mut log, server_key) = log();
        let mut surrogates = Surrogates { log: &mut log, server_key: &server_key };

        assert_eq!(surrogates.lookup(ActorKind::Federated, "http://example.org/alice").unwrap(), None);
        assert_eq!(surrogates.surrogate(ActorKind::Federated, "http://example.org/alice").unwrap(), 1);
        assert_eq!(surrogates.surrogate(ActorKind::Federated, "http://example.org/bob").unwrap(), 2);
        assert_eq!(surrogates.surrogate(ActorKind::Federated, "http://example.org/alice").unwrap(), 1);
        assert_eq!(surrogates.lookup(ActorKind::Federated, "http://example.org/bob").unwrap(), Some(2));
        assert_eq!(surrogates.raw(2).unwrap(), Some((ActorKind::Federated, "http://example.org/bob".to_string())));
        assert_eq!(surrogates.raw(3).unwrap(), None);
        assert!(surrogates.surrogate(ActorKind::Anonymous, "192.0.2.1").is_err());
        assert!(surrogates.surrogate(ActorKind::Federated, "not an iri").is_err());
    }

    #[test]
    fn an_erased_actor_record_leaves_only_the_number() {
        let (_dir, mut log, server_key) = log();
        let alice = iri("http://example.org/alice");
        log.register_key(&alice, &keys::generate().verifying_key(), &server_key).unwrap();
        log.register_key(&iri("http://example.org/bob"), &keys::generate().verifying_key(), &server_key).unwrap();

        // Erase the content of Alice's actor record, at offset 0 of the partition
        block_on(log.store.erase_parts(log.mutable, 0, &[0])).unwrap();

        // The number is still taken and stands for nobody; her keys belong to nobody
        assert_eq!(log.surrogates().unwrap()[0], (1, None));
        assert_eq!(log.surrogate_of(&alice).unwrap(), None);
        assert!(log.current_keys(&alice).unwrap().is_empty());
        assert_eq!(log.all_key_records().unwrap().len(), 1);
        for record in records(&log.store, log.mutable).unwrap() {
            assert!(!String::from_utf8_lossy(&record.encode()).contains("example.org/alice"));
        }

        // An erasure with no record of it does not pass verification
        let error = log.verify(&server_key.verifying_key()).unwrap_err();
        assert!(error.to_string().contains("Erasure"), "{error}");

        // Seen again, she gets a new surrogate, and the key registered now is hers
        let key = keys::generate().verifying_key();
        log.register_key(&alice, &key, &server_key).unwrap();
        assert_eq!(log.surrogate_of(&alice).unwrap(), Some(3));
        assert_eq!(log.current_keys(&alice).unwrap().len(), 1);
        assert_eq!(log.current_keys(&alice).unwrap()[0].public_key.0, key.to_bytes());
    }
}
