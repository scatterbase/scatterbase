//! What the kernel needs to know about the instance it serves.

use std::path::{Path, PathBuf};

use oxrdf::NamedNode;

/// One Scatterbase instance: its names and where it keeps its data.
#[derive(Debug, Clone)]
pub struct Instance {
    /// The domain: the base of skolem IRIs, the host of checkpoint origins and the subject
    /// of the first claim about the instance.
    pub domain: String,
    /// The IRI the server claims as.
    pub server: NamedNode,
    /// The directory of the log, which holds every accepted claim.
    pub log_dir: PathBuf,
    /// The N-Triples file of the store, which is derived from the log.
    pub storage_file: String,
}

impl Instance {
    /// An instance whose log is the directory `log` beside its storage file.
    pub fn new(domain: impl Into<String>, server: NamedNode, storage_file: impl Into<String>) -> Self {
        let storage_file = storage_file.into();
        let log_dir = Path::new(&storage_file).parent().unwrap_or(Path::new("")).join("log");
        Self { domain: domain.into(), server, log_dir, storage_file }
    }
}
