//! The N-Quads file backend.

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use oxrdf::Quad;

use crate::{read_nquads, write_nquads, MemoryStore, Pattern, QuadStore, QuadStoreError};

/// A quad store kept in an N-Quads file.
///
/// The quads are held in memory and searched there. The file is their durable copy, one
/// quad to a line in insertion order: a delta that only inserts appends the new quads, and
/// one that deletes writes the file again, to a temporary file that then replaces it.
///
/// One store has the file at a time: another's changes are not seen until it is opened
/// again.
#[derive(Debug)]
pub struct NQuadsFile {
    path: PathBuf,
    quads: MemoryStore,
}

impl NQuadsFile {
    /// Opens the store kept in the file at `path`. A file that does not exist is an empty
    /// store, and is created with its directory when quads are first inserted.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, QuadStoreError> {
        let path = path.into();
        let quads = if path.exists() {
            MemoryStore::from_quads(read_nquads(BufReader::new(File::open(&path)?))?)
        } else {
            MemoryStore::new()
        };
        Ok(Self { path, quads })
    }

    /// The file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The quads, in memory.
    pub fn quads(&self) -> &MemoryStore {
        &self.quads
    }

    /// The quads, in memory, leaving the file.
    pub fn into_quads(self) -> MemoryStore {
        self.quads
    }

    fn create_dir(&self) -> Result<(), QuadStoreError> {
        if let Some(parent) = self.path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl QuadStore for NQuadsFile {
    type Error = QuadStoreError;

    fn lookup(&self, pattern: &Pattern<'_>) -> Result<Vec<Quad>, QuadStoreError> {
        Ok(self.quads.matching(pattern))
    }

    fn apply(&mut self, deleted: &[Quad], inserted: &[Quad]) -> Result<(), QuadStoreError> {
        let mut removed = false;
        for quad in deleted {
            removed |= self.quads.remove(quad);
        }
        let added: Vec<&Quad> = inserted.iter().filter(|quad| self.quads.insert((*quad).clone())).collect();

        if removed {
            self.create_dir()?;
            let temporary = self.path.with_extension("tmp");
            let mut writer = BufWriter::new(File::create(&temporary)?);
            write_nquads(&mut writer, self.quads.iter())?;
            writer.into_inner().map_err(|e| e.into_error())?.sync_all()?;
            fs::rename(&temporary, &self.path)?;
        } else if !added.is_empty() {
            self.create_dir()?;
            let mut writer = BufWriter::new(OpenOptions::new().create(true).append(true).open(&self.path)?);
            write_nquads(&mut writer, added)?;
            writer.flush()?;
        }
        Ok(())
    }

    fn len(&self) -> Result<u64, QuadStoreError> {
        Ok(self.quads.len() as u64)
    }

    fn contains(&self, quad: &Quad) -> Result<bool, QuadStoreError> {
        Ok(self.quads.contains(quad))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::tests::{contract, sample};

    #[test]
    fn file_store_keeps_the_contract() {
        let dir = tempfile::tempdir().unwrap();
        contract(NQuadsFile::open(dir.path().join("store.nq")).unwrap());
    }

    #[test]
    fn the_file_is_the_stores_durable_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data").join("store.nq");
        let quads = sample();

        let mut store = NQuadsFile::open(&path).unwrap();
        assert!(!path.exists());
        store.apply(&[], &quads[..3]).unwrap();
        store.apply(&[], &quads[2..]).unwrap();

        // One quad to a line, in insertion order, the default graph left out
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "<http://example.org/a> <http://schema.org/name> \"A\" .");
        assert_eq!(lines[4], "<http://example.org/a> <http://schema.org/name> \"Ay\" <http://example.org/graph> .");

        // Opened again, it holds the same quads in the same order
        let reopened = NQuadsFile::open(&path).unwrap();
        assert_eq!(reopened.lookup(&Pattern::any()).unwrap(), quads);

        // A deletion rewrites the file
        let mut store = reopened;
        store.apply(&[quads[0].clone()], &[]).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 4);
        assert_eq!(NQuadsFile::open(&path).unwrap().into_quads().len(), 4);
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn a_file_that_is_not_nquads_does_not_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.nq");
        fs::write(&path, "this is not n-quads\n").unwrap();
        assert!(matches!(NQuadsFile::open(&path), Err(QuadStoreError::Syntax(_))));
    }
}
