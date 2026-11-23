//! Init module for Scatterbase
//! 
//! This module provides functionality for initializing a Scatterbase instance.

use crate::cli::{
    prompt_for_domain, prompt_for_storage_file, prompt_for_env_file,
    prompt_for_server_uri
};
use scatterbase_kernel::constants::{DEFAULT_STORAGE_FILE, DEFAULT_ENV_FILE};
use crate::domain::create_env_file;
use scatterbase_kernel::domain::create_initial_claims;
use crate::config::Config;
use scatterbase_kernel::prefix::init_prefix_file;

/// Handles the "init" command
/// 
/// If `use_defaults` is true, all default values will be used without prompting.
pub fn handle_init_command(use_defaults: bool) {
    println!("Initializing a new Scatterbase instance...");

    // Get domain (prompt or use default)
    let domain = if use_defaults {
        let domain = scatterbase_kernel::storage::generate_default_domain();
        println!("Using domain: {}", domain);
        domain
    } else {
        let domain = prompt_for_domain();
        println!("Using domain: {}", domain);
        domain
    };

    // The domain is the subject of the first claim, so it must be an absolute URI
    if let Err(e) = scatterbase_kernel::uri::parse_iri(&domain) {
        println!("Invalid domain: {}", e);
        return;
    }

    // Get storage file (prompt or use default)
    let storage_file = if use_defaults {
        let storage_file = DEFAULT_STORAGE_FILE.to_string();
        println!("Using storage file: {}", storage_file);
        storage_file
    } else {
        let storage_file = prompt_for_storage_file();
        println!("Using storage file: {}", storage_file);
        storage_file
    };

    // Get .env file (prompt or use default)
    let env_file = if use_defaults {
        let env_file = DEFAULT_ENV_FILE.to_string();
        println!("Using .env file: {}", env_file);
        env_file
    } else {
        let env_file = prompt_for_env_file();
        println!("Using .env file: {}", env_file);
        env_file
    };

    // Get server URI (prompt or use default)
    let server_uri = if use_defaults {
        let server_uri = format!("{}/Server", domain);
        println!("Using server URI: {}", server_uri);
        server_uri
    } else {
        let server_uri = prompt_for_server_uri(&domain);
        println!("Using server URI: {}", server_uri);
        server_uri
    };

    // Create the .env file
    if let Err(e) = create_env_file(&env_file, &domain, &storage_file, &server_uri, use_defaults) {
        println!("Error creating .env file: {}", e);
        return;
    }

    // Create the first claims: the server's key and the domain
    let claimed = Config::load(&env_file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e))
        .and_then(|config| {
            let server_key = config.server_key()?;
            create_initial_claims(&config.instance(), &server_key)
        });
    if let Err(e) = claimed {
        println!("Error adding initial claim to storage: {}", e);
        return;
    }

    println!("Domain initialized with initial claim");

    // Initialize the prefix file with default prefixes
    if let Err(e) = init_prefix_file() {
        println!("Error initializing prefix file: {}", e);
        return;
    }

    println!("Scatterbase initialized successfully!");
    println!("Configuration stored in {}", env_file);
}
