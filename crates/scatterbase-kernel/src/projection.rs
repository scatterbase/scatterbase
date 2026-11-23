//! The quad store as a projection of the claim log.
//!
//! The store ([`crate::storage`]) holds every claim in the log in the RDF form of
//! [`scatter_claims::rdf`]: a claim's own triples in the `server` graph if the server made
//! it and the `claim` graph otherwise, and the triples about it in the `meta` graph. The
//! `mutable` graph holds the registration of every key an actor has now, from the key
//! records of the log's mutable partition. It is derived: [`catch_up`] writes the claims the file does not
//! hold yet, and [`rebuild`] writes it again from the first claim. How far the file has
//! got is kept beside it, in `{storage file}.offset`, as the offset of the next claim to
//! write.

use std::fs;
use std::io;
use std::path::PathBuf;

use ed25519_dalek::VerifyingKey;
use oxrdf::{NamedNode, Quad};
use scatter_actors::signing::{current_keys, KeyRecord};
use scatter_actors::time::Timestamp;
use scatter_quadstore::{NQuadsFile, Pattern, QuadStore};

use crate::instance::{Instance, GRAPH_CLAIM, GRAPH_META, GRAPH_MUTABLE};
use crate::keys;
use crate::log::ClaimLog;
use crate::storage;

fn position_file(instance: &Instance) -> PathBuf {
    PathBuf::from(format!("{}.offset", instance.storage_file))
}

/// The offset of the next claim to write to the store.
pub fn position(instance: &Instance) -> io::Result<u64> {
    match fs::read_to_string(position_file(instance)) {
        Ok(text) => text.trim().parse().map_err(|_| io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} does not hold an offset", position_file(instance).display()),
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(e),
    }
}

/// Brings the store up to the log: writes every claim it does not hold yet, and makes the
/// `mutable` graph the registrations of the keys actors have now. Returns the number of
/// claims in the log.
pub fn catch_up(instance: &Instance, log: &ClaimLog) -> io::Result<u64> {
    let mut store = storage::open(&instance.storage_file)?;
    let from = position(instance)?;
    let size = log.size()?;

    if from < size {
        let meta = instance.graph(GRAPH_META);
        for stored in log.claims(from)? {
            // A claim whose attestation was erased is nobody's, and so not the server's
            let content = match &stored.claim.attestation {
                Some((attribution, _)) => instance.content_graph(attribution),
                None => instance.graph(GRAPH_CLAIM),
            };
            storage::insert(&mut store, &stored.claim.to_quads(&content, &meta))?;
        }
        // A claim written twice changes nothing, so the position may lag but never lead
        // Written whole or not at all: beside the file, then in its place
        let next = position_file(instance).with_extension("offset.next");
        fs::write(&next, size.to_string())?;
        fs::rename(&next, position_file(instance))?;
    }
    project_keys(instance, log, &mut store)?;
    Ok(size)
}

/// Makes the `mutable` graph hold the registration of every key an actor has now, and
/// nothing else: a key that was revoked or has expired leaves it.
fn project_keys(instance: &Instance, log: &ClaimLog, store: &mut NQuadsFile) -> io::Result<()> {
    let graph = instance.graph(GRAPH_MUTABLE);

    let mut by_actor: Vec<(NamedNode, Vec<KeyRecord>)> = Vec::new();
    for (actor, record) in log.all_key_records()? {
        match by_actor.iter_mut().find(|(of, _)| *of == actor) {
            Some((_, records)) => records.push(record),
            None => by_actor.push((actor, vec![record])),
        }
    }
    let now = Timestamp(u64::try_from(chrono::Utc::now().timestamp_micros()).unwrap_or(0));
    let mut current: Vec<Quad> = Vec::new();
    for (actor, records) in &by_actor {
        for key in current_keys(records).into_iter().filter(|key| key.is_valid_at(now)) {
            let key = VerifyingKey::from_bytes(&key.public_key.0)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            current.extend(keys::registration(actor, &key).into_iter().map(|triple| triple.in_graph(graph.clone())));
        }
    }

    let held = store.lookup(&Pattern::any().graph(&graph))?;
    let deleted: Vec<Quad> = held.iter().filter(|quad| !current.contains(quad)).cloned().collect();
    let inserted: Vec<Quad> = current.into_iter().filter(|quad| !held.contains(quad)).collect();
    Ok(store.apply(&deleted, &inserted)?)
}

/// Writes the store again from the first claim of the log. Returns the log's size.
pub fn rebuild(instance: &Instance, log: &ClaimLog) -> io::Result<u64> {
    for path in [PathBuf::from(&instance.storage_file), position_file(instance)] {
        match fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {},
        }
    }
    catch_up(instance, log)
}
