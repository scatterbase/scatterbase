//! The `domain` command, and writing the `.env` file `init` creates.

use std::io::{self, Write};
use std::fs::{self, File, Permissions};
use std::env;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use scatterbase_kernel::utils::ensure_directory_exists;
use scatterbase_kernel::constants::{DEFAULT_PUBLIC_KEY_FILE, DEFAULT_PREFIX_FILE, DEFAULT_PRIVATE_KEY_FILE};
use scatterbase_kernel::crypto::{generate_ed25519_keypair, BASE32Z};
use scatterbase_kernel::storage::Storage;
use scatterbase_kernel::domain::create_initial_triple;
use crate::cli::{prompt_for_domain, prompt_for_key_file};

/// Creates an .env file with the configuration
/// 
/// If `use_defaults` is true, all default values will be used without prompting.
pub fn create_env_file(env_file: &str, domain: &str, storage_file: &str, server_uri: &str, use_defaults: bool) -> io::Result<()> {
    // Generate ED25519 key pair
    let (signing_key, verifying_key) = generate_ed25519_keypair();

    // Convert keys to Base32z for storage (both private and public keys)
    let secret_key_b32z = BASE32Z.encode(&signing_key.to_bytes());
    let public_key_b32z = BASE32Z.encode(&verifying_key.to_bytes());

    // Get private key file (prompt or use default)
    let private_key_file = if use_defaults {
        let private_key_file = DEFAULT_PRIVATE_KEY_FILE.to_string();
        println!("Using private key file: {}", private_key_file);
        private_key_file
    } else {
        let private_key_file = prompt_for_key_file();
        println!("Using private key file: {}", private_key_file);
        private_key_file
    };

    // Ensure directory exists
    ensure_directory_exists(&private_key_file)?;

    // Write secret key to separate file
    let mut private_key_file_handle = File::create(&private_key_file)?;
    private_key_file_handle.write_all(secret_key_b32z.as_bytes())?;

    // Set file permissions to 600 (read/write for owner only) if on Unix
    #[cfg(unix)]
    {
        let permissions = Permissions::from_mode(0o600);
        fs::set_permissions(&private_key_file, permissions)?;
        println!("Set permissions of {} to 600 (read/write for owner only)", private_key_file);
    }

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

            // Use the storage module to append the triple
            let mut storage = scatterbase_kernel::storage::NtFileStorage::new(file_path);

            // Initialize storage if needed
            if let Err(e) = storage.init(&domain) {
                println!("Error initializing storage: {}", e);
                return;
            }

            // Append the triple to the storage file
            if let Err(e) = storage.append(&triple) {
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
