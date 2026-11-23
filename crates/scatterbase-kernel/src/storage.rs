//! The instance's quad store: an N-Quads file, which the claim log is projected into.
//!
//! The store is a [`scatter_quadstore::NQuadsFile`]. Everything the kernel writes is in the
//! default graph, so the file is also N-Triples.

use std::io;

use oxrdf::{GraphName, Quad, Triple};
use scatter_quadstore::{NQuadsFile, QuadStore};

/// Opens the store kept in the storage file.
pub fn open(storage_file: &str) -> io::Result<NQuadsFile> {
    Ok(NQuadsFile::open(storage_file)?)
}

/// Inserts triples into a store, in the default graph. Triples it holds are left as they are.
pub fn insert(store: &mut impl QuadStore, triples: impl IntoIterator<Item = Triple>) -> io::Result<()> {
    let quads: Vec<Quad> = triples.into_iter().map(|triple| triple.in_graph(GraphName::DefaultGraph)).collect();
    store.apply(&[], &quads).map_err(io::Error::other)
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
