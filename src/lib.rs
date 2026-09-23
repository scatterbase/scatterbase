// Scatterbase library modules
pub mod uri;
pub mod literal;
pub mod term;
pub mod triple;
pub mod storage;
pub mod cli;
pub mod domain;
pub mod claim;
pub mod crypto;
pub mod prefix;
pub mod utils;
pub mod init;
pub mod purge;
pub mod constants;
pub mod key;
// term_command has been merged into term

// Tests
#[cfg(test)]
pub mod tests;

// Re-export commonly used items
pub use uri::Uri;
pub use literal::Literal;
pub use term::Term;
pub use triple::Triple;
pub use storage::{Storage, NtFileStorage, generate_default_domain};
