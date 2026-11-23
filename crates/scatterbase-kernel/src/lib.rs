// Scatterbase library modules
pub mod uri;
pub mod triple;
pub mod storage;
pub mod domain;
pub mod claim;
pub mod keys;
pub mod lookup;
pub mod prefix;
pub mod utils;
pub mod constants;
// term_command has been merged into term

// Tests
#[cfg(test)]
pub mod tests;

// Re-export commonly used items
pub use oxrdf::{Literal, NamedNode, Term, Triple};
pub use triple::TripleExt;
pub use storage::{Storage, NtFileStorage, generate_default_domain};
