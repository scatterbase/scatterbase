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
    /// The N-Quads file of the store, which is derived from the log.
    pub storage_file: String,
}

/// The graph of what actors other than the server claim.
pub const GRAPH_CLAIM: &str = "claim";
/// The graph of what the envelope and the server record about each claim.
pub const GRAPH_META: &str = "meta";
/// The graph of what the server itself claims: registered keys and the domain.
pub const GRAPH_SERVER: &str = "server";

impl Instance {
    /// An instance whose log is the directory `log` beside its storage file.
    pub fn new(domain: impl Into<String>, server: NamedNode, storage_file: impl Into<String>) -> Self {
        let storage_file = storage_file.into();
        let log_dir = Path::new(&storage_file).parent().unwrap_or(Path::new("")).join("log");
        Self { domain: domain.into(), server, log_dir, storage_file }
    }

    /// The IRI of one of the instance's named graphs, `{domain}/graph/{name}`.
    pub fn graph(&self, name: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}/graph/{name}", self.domain.trim_end_matches('/')))
    }

    /// The graph a claim's own triples go in: [`GRAPH_SERVER`] for a claim by the server,
    /// [`GRAPH_CLAIM`] for a claim by anyone else.
    pub fn content_graph(&self, attribution: &NamedNode) -> NamedNode {
        self.graph(if *attribution == self.server { GRAPH_SERVER } else { GRAPH_CLAIM })
    }
}
