//! The quad-store contract and two backends.
//!
//! A quad store holds a set of RDF quads and answers two calls ([`QuadStore`]):
//!
//! - [`lookup`](QuadStore::lookup): the quads matching a [`Pattern`], in which each of the
//!   subject, predicate, object and graph is either given or a wildcard;
//! - [`apply`](QuadStore::apply): a delta, the quads to delete and the quads to insert.
//!
//! That is the whole contract. Access paths particular to what the quads mean, such as
//! finding a claim or walking a chain, are built on `lookup` by the crates that need them
//! and are not added here.
//!
//! The backends:
//!
//! - [`MemoryStore`] keeps the quads in memory, in the order they were inserted, with an
//!   index on each of subject, predicate and object.
//! - [`NQuadsFile`] is a `MemoryStore` kept in an N-Quads file: opening reads the file, an
//!   insertion appends to it, and a deletion writes it again. A file whose quads are all in
//!   the default graph is also an N-Triples file.
//!
//! [`read_nquads`] and [`write_nquads`] read and write a dump. Terms are [`oxrdf`]'s.

mod file;
mod memory;
mod pattern;

use std::io::{self, BufRead, Write};

use oxrdf::Quad;
use oxttl::NQuadsParser;

pub use file::NQuadsFile;
pub use memory::MemoryStore;
pub use pattern::Pattern;

/// A set of quads that can be searched by pattern and changed by deltas.
pub trait QuadStore {
    /// Why a call failed.
    type Error: std::error::Error + Send + Sync + 'static;

    /// The quads that match the pattern. A store that keeps an order returns them in it.
    fn lookup(&self, pattern: &Pattern<'_>) -> Result<Vec<Quad>, Self::Error>;

    /// Applies a delta: deletes `deleted`, then inserts `inserted`.
    ///
    /// Deleting a quad the store does not hold and inserting one it holds change nothing, so
    /// applying the same delta twice leaves the store as applying it once does. A quad in
    /// both lists is held afterwards.
    fn apply(&mut self, deleted: &[Quad], inserted: &[Quad]) -> Result<(), Self::Error>;

    /// The number of quads held.
    fn len(&self) -> Result<u64, Self::Error>;

    /// Whether the store holds no quads.
    fn is_empty(&self) -> Result<bool, Self::Error> {
        Ok(self.len()? == 0)
    }

    /// Whether the store holds the quad.
    fn contains(&self, quad: &Quad) -> Result<bool, Self::Error> {
        Ok(!self.lookup(&Pattern::of(quad))?.is_empty())
    }
}

/// Why a quad store call failed.
#[derive(Debug, thiserror::Error)]
pub enum QuadStoreError {
    /// The file could not be read or written.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// The file is not N-Quads.
    #[error("not N-Quads: {0}")]
    Syntax(String),
}

impl From<QuadStoreError> for io::Error {
    fn from(error: QuadStoreError) -> Self {
        match error {
            QuadStoreError::Io(error) => error,
            syntax => io::Error::new(io::ErrorKind::InvalidData, syntax),
        }
    }
}

/// Reads an N-Quads dump.
pub fn read_nquads(reader: impl BufRead) -> Result<Vec<Quad>, QuadStoreError> {
    NQuadsParser::new()
        .for_reader(reader)
        .map(|quad| quad.map_err(|e| QuadStoreError::Syntax(e.to_string())))
        .collect()
}

/// Writes quads as an N-Quads dump, one to a line.
pub fn write_nquads<'a>(mut writer: impl Write, quads: impl IntoIterator<Item = &'a Quad>) -> io::Result<()> {
    for quad in quads {
        writeln!(writer, "{quad} .")?;
    }
    Ok(())
}
