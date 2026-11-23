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
//!
//! Triples written to the file by other means (`triple write`, `domain init`) are in the
//! default graph and not in the log, so a rebuild does not bring them back.

use std::fs;
use std::io;
use std::path::PathBuf;

use ed25519_dalek::VerifyingKey;
use oxrdf::{NamedNode, Quad};
use scatter_actors::signing::{current_keys, KeyRecord};
use scatter_quadstore::{NQuadsFile, Pattern, QuadStore};

use crate::instance::{Instance, GRAPH_META, GRAPH_MUTABLE};
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
        for (offset, stored) in log.claims(from)? {
            let content = instance.content_graph(stored.signed.envelope().claim().attribution());
            storage::insert(&mut store, &stored.signed.to_quads(&content, &meta))?;
            // A claim written twice changes nothing, so the position may lag but never lead
            fs::write(position_file(instance), (offset + 1).to_string())?;
        }
    }
    project_keys(instance, log, &mut store)?;
    Ok(size)
}

/// Makes the `mutable` graph hold the registration of every key an actor has now, and
/// nothing else: a key that was revoked leaves it.
fn project_keys(instance: &Instance, log: &ClaimLog, store: &mut NQuadsFile) -> io::Result<()> {
    let graph = instance.graph(GRAPH_MUTABLE);

    let mut by_actor: Vec<(NamedNode, Vec<KeyRecord>)> = Vec::new();
    for (actor, record) in log.all_key_records()? {
        match by_actor.iter_mut().find(|(of, _)| *of == actor) {
            Some((_, records)) => records.push(record),
            None => by_actor.push((actor, vec![record])),
        }
    }
    let mut current: Vec<Quad> = Vec::new();
    for (actor, records) in &by_actor {
        for key in current_keys(records) {
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
