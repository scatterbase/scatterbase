//! The claim log: every accepted claim as a record of an append-only log.
//!
//! The log is the instance's source of truth. It is a [`scatter_log`] file store with one
//! partition, `claim`, which keeps every record and is `logged`: after each append the
//! server signs a checkpoint of the partition's Merkle tree head, so a copy of the log can
//! be shown to be complete and in order ([`ClaimLog::verify`]). The N-Triples store is
//! derived from the log ([`crate::projection`]).
//!
//! # The claim record
//!
//! A claim is one record of payload type `scatter:v0/claim`. Its header key is the claim ID
//! in z-base-32 and its `appended_at` is the time the server accepted it. Its offset is the
//! claim's sequence ID. The body has five parts, each erasable on its own:
//!
//! | # | Part | CBOR |
//! |---|---|---|
//! | 0 | The claim | `{"previous": bytes(32), "triples": [text, …]}`, each triple its N-Triples line, in the claim's order |
//! | 1 | The attribution | text, the actor's IRI |
//! | 2 | The client signature | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64)}`, `key` the public key it verified against |
//! | 3 | The server signature | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64), "sequence": text, "timestamp": text}` |
//! | 4 | The acknowledgments | `[[text, bytes(32)], …]`, term IRI and prior claim ID, in order of the IRI |
//!
//! The partition has no registry record yet: a log directory holds exactly one partition,
//! and that is the claim partition.

use std::io;
use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};
use oxrdf::{NamedNode, Triple};
use oxttl::NTriplesParser;
use pollster::block_on;
use scatter_claims::id::ntriples_line;
use scatter_claims::{Acknowledgments, Claim, ClaimId, Envelope, Signature, SignedClaim};
use scatter_integrity::checkpoint::{host_of, tenant_origin, verifier_for};
use scatter_integrity::{CheckpointStore, FileCheckpoints};
use scatter_log::body::{Body, Part};
use scatter_log::cbor::Value;
use scatter_log::genesis::fresh_partition_id;
use scatter_log::record::Record;
use scatter_log::segments::SegmentStore;
use scatter_log::store::{Draft, LogStore, Slot};
use scatter_log::tree::Segments;

/// The payload type of a claim record.
pub const PAYLOAD_CLAIM: &str = "scatter:v0/claim";
/// The name of the claim partition, as in its checkpoint origin `{host}/log/claim`.
pub const PARTITION_NAME: &str = "claim";
/// The claim partition's segment exponent: 2^16 records to a segment file.
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

/// The claim log of an instance.
pub struct ClaimLog {
    store: SegmentStore,
    checkpoints: FileCheckpoints,
    partition: u64,
    origin: String,
}

fn other(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

fn corrupt(offset: u64, what: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("claim log, offset {offset}: {what}"))
}

impl ClaimLog {
    /// Opens the log in `dir`, creating it with an empty claim partition if it is not there.
    /// `domain` gives the host that checkpoints are signed under.
    pub fn open(dir: impl AsRef<Path>, domain: &str) -> io::Result<Self> {
        let dir = dir.as_ref();
        let mut store = SegmentStore::open(dir).map_err(other)?;
        let partitions = block_on(store.partitions()).map_err(other)?;
        let partition = match partitions.as_slice() {
            [] => {
                let partition = fresh_partition_id();
                let segments = Segments::new(SEGMENT_EXPONENT).expect("the exponent is below 64");
                block_on(store.create_partition(partition, segments)).map_err(other)?;
                partition
            },
            [partition] => *partition,
            _ => return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} holds more than one partition", dir.display()),
            )),
        };
        Ok(Self {
            store,
            checkpoints: FileCheckpoints::new(dir),
            partition,
            origin: tenant_origin(&host_of(domain), PARTITION_NAME),
        })
    }

    /// The number of claims in the log, which is the offset the next one takes.
    pub fn size(&self) -> io::Result<u64> {
        Ok(block_on(self.store.head(self.partition)).map_err(other)?.size)
    }

    /// The origin line of the log's checkpoints, `{host}/log/claim`.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Appends an accepted claim and signs a checkpoint of the new head. Returns the offset.
    ///
    /// The claim's sequence ID must be the offset it takes, so that the server's signature
    /// commits to the claim's place in the log.
    pub fn append(&mut self, signed: &SignedClaim, actor_key: &VerifyingKey, server_key: &SigningKey) -> io::Result<u64> {
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

    /// Verifies the whole log against the server's public key and returns its size.
    ///
    /// The latest checkpoint must be signed by the server and commit to the log as it is:
    /// its size and its Merkle tree head. Every record must match the body commitment in
    /// its header, read back as the claim its key names at the sequence ID of its offset,
    /// and carry an actor's signature and a server's signature that verify.
    pub fn verify(&self, server_key: &VerifyingKey) -> io::Result<u64> {
        let invalid = |what: String| io::Error::new(io::ErrorKind::InvalidData, format!("claim log: {what}"));
        let head = block_on(self.store.head(self.partition)).map_err(other)?;

        match block_on(self.checkpoints.latest(self.partition)).map_err(other)? {
            Some(signed) => {
                let verifier = verifier_for(&self.origin, server_key.to_bytes()).map_err(other)?;
                signed.verify(&self.origin, &[verifier])
                    .map_err(|e| invalid(format!("the latest checkpoint: {e}")))?;
                if signed.checkpoint.size != head.size || signed.checkpoint.root != head.root {
                    return Err(invalid(format!(
                        "the latest checkpoint commits to {} records, and the log holds {} that do not match it",
                        signed.checkpoint.size, head.size,
                    )));
                }
            },
            None if head.size == 0 => {},
            None => return Err(invalid("there are records but no checkpoint".into())),
        }

        for offset in 0..head.size {
            let slot = block_on(self.store.read(self.partition, offset)).map_err(other)?;
            let Slot::Record(record) = slot else {
                return Err(corrupt(offset, "the record was compacted"));
            };
            record.body().verify(&record.header().commitment).map_err(|e| corrupt(offset, e))?;
            let stored = claim_of(&record).map_err(|e| corrupt(offset, e))?;
            if stored.server_key != *server_key {
                return Err(corrupt(offset, "accepted under another server key"));
            }
            stored.signed.envelope().verify(&stored.actor_key).map_err(|e| corrupt(offset, e))?;
            stored.signed.verify(server_key).map_err(|e| corrupt(offset, e))?;
        }
        Ok(head.size)
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
