//! Constants module for Scatterbase
//! 
//! This module provides centralized access to constants used throughout the Scatterbase project.

/// File path constants used throughout the application
pub mod paths {
    /// Default path for the environment file
    pub const DEFAULT_ENV_FILE: &str = ".env";
    
    /// Default path for the storage file
    pub const DEFAULT_STORAGE_FILE: &str = "data/scatterbase.nt";
    
    /// Default path for the prefix file
    pub const DEFAULT_PREFIX_FILE: &str = "data/prefixes.ttl";
    
    /// Default path for the private key file
    pub const DEFAULT_PRIVATE_KEY_FILE: &str = "keys/scatterbase.ed25519.private";
    
    /// Default path for the public key file
    pub const DEFAULT_PUBLIC_KEY_FILE: &str = "keys/scatterbase.ed25519.public";
}

/// Application information constants
pub mod app {
    /// Name of the program
    pub const PROGRAM_NAME: &str = "Scatterbase";
    
    /// Description for the environment option in CLI help
    pub const ENV_OPTION_DESC: &str = "  --env <path>       Specify a custom path to the .env file (default: .env)";
}

// Re-export all constants at the module level for backward compatibility
pub use self::paths::*;
pub use self::app::*;