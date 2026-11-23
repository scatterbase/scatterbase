//! The `domain` command, and writing the `.env` file `init` creates.

use std::io::{self, Write};
use std::fs::File;
use std::env;


use scatterbase_kernel::utils::ensure_directory_exists;
use scatterbase_kernel::constants::{DEFAULT_PUBLIC_KEY_FILE, DEFAULT_PREFIX_FILE, DEFAULT_PRIVATE_KEY_FILE};
use scatterbase_kernel::keys;
use scatter_claims::id::zbase32;
use scatterbase_kernel::storage;
use scatterbase_kernel::triple::TripleExt;
use scatterbase_kernel::domain::create_initial_triple;
use crate::cli::{prompt_for_domain, prompt_for_key_file};

/// Creates an .env file with the configuration
/// 
/// If `use_defaults` is true, all default values will be used without prompting.
pub fn create_env_file(env_file: &str, domain: &str, storage_file: &str, server_uri: &str, use_defaults: bool) -> io::Result<()> {
    // Generate ED25519 key pair
    let signing_key = keys::generate();
    let public_key_b32z = zbase32(&signing_key.verifying_key().to_bytes());

    // Get private key file (prompt or use default)
    let private_key_file = if use_defaults {
        DEFAULT_PRIVATE_KEY_FILE.to_string()
    } else {
        prompt_for_key_file()
    };
    println!("Using private key file: {}", private_key_file);

    // Write the secret key to its own file, readable by its owner only
    keys::write_signing_key(&private_key_file, &signing_key)?;

    // Write public key to separate file
    let public_key_file = DEFAULT_PUBLIC_KEY_FILE;
    ensure_directory_exists(&public_key_file)?;
    let mut public_key_file_handle = File::create(&public_key_file)?;
    public_key_file_handle.write_all(public_key_b32z.as_bytes())?;
    println!("Public key stored in {}", public_key_file);

    // Create content with domain, storage file, server URI, key file paths, and prefix file
    let content = format!(
        "DOMAIN={}\nSTORAGE_FILE={}\nSERVER_URI={}\nED25519_PUBLIC_KEY_FILE={}\nED25519_SECRET_KEY_FILE={}\nPREFIX_FILE={}\n",
        domain, storage_file, server_uri, public_key_file, private_key_file, DEFAULT_PREFIX_FILE
    );

    let mut file = File::create(env_file)?;
    file.write_all(content.as_bytes())?;

    println!("Generated ED25519 key pair for signing claims");
    println!("Secret key stored in {}", private_key_file);

    Ok(())
}

/// Handles the "domain init" command
pub fn handle_domain_init_command(env_path: &str, uri_param: Option<&str>) {
    println!("Initializing a new domain...");

    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        println!("Error loading .env file at {}: {}", env_path, e);
        println!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get domain - either from parameter, prompt, or environment
    let domain = if let Some(uri) = uri_param {
        uri.to_string()
    } else {
        // Prompt user for domain if not provided as parameter
        prompt_for_domain()
    };

    // The domain is the subject of the first claim, so it must be an absolute URI
    if let Err(e) = scatterbase_kernel::uri::parse_iri(&domain) {
        println!("Invalid domain: {}", e);
        return;
    }

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => Some(file),
        Err(_) => {
            println!("STORAGE_FILE not set in environment. Output will be sent to stdout.");
            None
        }
    };

    // Create the initial triple
    let triple = create_initial_triple(&domain);

    // Handle storage based on whether we have a storage file
    match &storage_file {
        Some(file_path) => {
            // Ensure the storage directory exists
            if let Err(e) = ensure_directory_exists(file_path) {
                println!("Error creating storage directory: {}", e);
                return;
            }

            // Append the triple to the storage file
            let stored = storage::open(file_path).and_then(|mut store| storage::insert(&mut store, [triple.clone()]));
            if let Err(e) = stored {
                println!("Error appending to storage file: {}", e);
                return;
            }

            println!("Domain initialized with initial triple:");
            println!("{}", triple.to_ntriples());
            println!("Triple appended to storage file: {}", file_path);
        },
        None => {
            // If no storage file is defined, write to stdout
            println!("Domain initialized with initial triple:");
            println!("{}", triple.to_ntriples());
        }
    }
}
