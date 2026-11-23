// Scatterbase library modules
pub mod uri;
pub mod triple;
pub mod storage;
pub mod domain;
pub mod claim;
pub mod instance;
pub mod keys;
pub mod log;
pub mod lookup;
pub mod projection;
pub mod prefix;
pub mod utils;
pub mod constants;
// term_command has been merged into term

// Tests
#[cfg(test)]
pub mod tests;

// Re-export commonly used items
pub use oxrdf::{Literal, NamedNode, Term, Triple};
pub use instance::Instance;
pub use triple::TripleExt;
pub use storage::generate_default_domain;
