//! The instance's quad store: an N-Quads file, which the claim log is projected into.
//!
//! The store is a [`scatter_quadstore::NQuadsFile`]. What it holds is in four graphs:
//!
//! | Graph | Holds |
//! |---|---|
//! | `{domain}/graph/claim` | The triples of every claim by an actor other than the server |
//! | `{domain}/graph/server` | The triples of every claim by the server, such as the declaration of the domain |
//! | `{domain}/graph/meta` | Everything recorded about each claim: its reification, attribution, chain, acknowledgments, signatures, sequence ID and time |
//! | `{domain}/graph/mutable` | The registration of each key an actor has now |
//!
//! Only the projection of the log ([`crate::projection`]) writes to the named graphs. The
//! lookups ([`crate::lookup`]) read `meta` alone, and keys are read from the log, not from
//! the store. So nothing an actor claims is taken for a registered key or for what the
//! server recorded about a claim. Nothing is written to the default graph.

use std::io;

use oxrdf::Quad;
use scatter_quadstore::{NQuadsFile, QuadStore};

/// Opens the store kept in the storage file.
pub fn open(storage_file: &str) -> io::Result<NQuadsFile> {
    Ok(NQuadsFile::open(storage_file)?)
}

/// Inserts quads into a store. Quads it holds are left as they are.
pub fn insert(store: &mut impl QuadStore, quads: &[Quad]) -> io::Result<()> {
    store.apply(&[], quads).map_err(io::Error::other)
}

/// Generates a default domain for Scatterbase
///
/// The default domain is 16 randomly generated letters followed by ".sandbox"
pub fn generate_default_domain() -> String {
    use rand::{Rng, thread_rng};
    use rand::distributions::Alphanumeric;

    let random_string: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .filter(|c| c.is_alphabetic())
        .take(16)
        .collect();

    format!("http://{}.sandbox", random_string.to_lowercase())
}
