//! The quad store as a projection of the claim log.
//!
//! The store ([`crate::storage`]) holds the triples of every claim in the log, in the RDF
//! form of [`scatter_claims::rdf`]. It is derived: [`catch_up`] writes the claims the file does not
//! hold yet, and [`rebuild`] writes it again from the first claim. How far the file has
//! got is kept beside it, in `{storage file}.offset`, as the offset of the next claim to
//! write.
//!
//! Triples written to the file by other means (`triple write`, `domain init`) are not in
//! the log, so a rebuild does not bring them back.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::instance::Instance;
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

/// Writes to the store every claim of the log it does not hold yet. Returns the log's size.
pub fn catch_up(instance: &Instance, log: &ClaimLog) -> io::Result<u64> {
    let from = position(instance)?;
    let size = log.size()?;
    if from >= size {
        return Ok(size);
    }

    let mut store = storage::open(&instance.storage_file)?;
    for (offset, stored) in log.claims(from)? {
        storage::insert(&mut store, stored.signed.to_triples())?;
        // A claim written twice changes nothing, so the position may lag but never lead
        fs::write(position_file(instance), (offset + 1).to_string())?;
    }
    Ok(size)
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
