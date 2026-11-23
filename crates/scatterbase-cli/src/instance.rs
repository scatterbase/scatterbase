//! The commands that set an instance up and take it down: `init`, `domain`, `prefix` and
//! `purge`.

use std::fs;
use std::path::Path;

use scatter_claims::id::zbase32;
use scatterbase_kernel::domain::{create_initial_claims, create_initial_triple};
use scatterbase_kernel::keys;
use scatterbase_kernel::prefix::{add_prefix, init_prefix_file};
use scatterbase_kernel::storage::generate_default_domain;

use crate::claim::{print_accepted, write_claim};
use crate::config::{Config, DEFAULT_PREFIX_FILE, DEFAULT_PRIVATE_KEY_FILE, DEFAULT_PUBLIC_KEY_FILE, DEFAULT_STORAGE_FILE};
use crate::error::{Context, Result};
use crate::prompt::{ask, confirm};

/// `init`: sets up a new instance: its configuration, the server's key, its log with the
/// first claim, and its prefix file. With `use_defaults` nothing is asked.
pub fn init_command(env_path: &str, use_defaults: bool) -> Result {
    if Path::new(env_path).exists() {
        return Err(format!(
            "{env_path} already exists, so an instance is set up here. To start again, remove it first with 'scatterbase purge'.",
        ).into());
    }
    println!("Initializing a new Scatterbase instance...");
    let choose = |question: &str, default: &str| -> Result<String> {
        let answer = if use_defaults { default.to_string() } else { ask(question, default)? };
        Ok(answer)
    };

    let domain = choose("Enter domain name (e.g., https://example.org)", &generate_default_domain())?;
    println!("Using domain: {domain}");
    // The domain is the subject of the first claim, so it must be an absolute URI
    crate::iri(&domain).context("Invalid domain")?;
    let storage_file = choose("Enter storage file path", DEFAULT_STORAGE_FILE)?;
    println!("Using storage file: {storage_file}");
    let server = choose("Enter server URI", &format!("{domain}/Server"))?;
    println!("Using server URI: {server}");
    let secret_key_file = choose("Enter private key file path", DEFAULT_PRIVATE_KEY_FILE)?;
    println!("Using private key file: {secret_key_file}");

    let config = Config {
        server: crate::iri(&server).context("Invalid server URI")?,
        domain,
        storage_file,
        secret_key_file,
        public_key_file: DEFAULT_PUBLIC_KEY_FILE.to_string(),
        prefix_file: DEFAULT_PREFIX_FILE.to_string(),
        log_dir: None,
    };
    if config.instance().log_dir.exists() {
        return Err(format!(
            "{} already holds a log. Remove it, or name another storage file.",
            config.instance().log_dir.display(),
        ).into());
    }

    // The server's key: the secret half readable by its owner only
    let server_key = keys::generate();
    keys::write_signing_key(&config.secret_key_file, &server_key)?;
    if let Some(parent) = Path::new(&config.public_key_file).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&config.public_key_file, zbase32(&server_key.verifying_key().to_bytes()))?;
    println!("Generated ED25519 key pair for signing claims");
    println!("Secret key stored in {}", config.secret_key_file);
    println!("Public key stored in {}", config.public_key_file);

    // The log, founded on that key, and the first claim: the domain
    create_initial_claims(&config.instance(), &server_key).context("Error adding initial claim to storage")?;
    println!("Domain initialized with initial claim");

    if init_prefix_file(&config.prefix_file).context("Error initializing prefix file")? {
        println!("Initialized prefix file with standard RDF prefixes: {}", config.prefix_file);
    }

    // The configuration is written last, so that a set-up that failed is not taken for one
    fs::write(env_path, config.to_env())?;
    println!("Scatterbase initialized successfully!");
    println!("Configuration stored in {env_path}");
    Ok(())
}

/// `domain init`: the server claims that a domain is a dataset.
pub fn domain_command(config: &Config, uri: Option<&str>) -> Result {
    let domain = match uri {
        Some(uri) => uri.to_string(),
        None => ask("Enter domain name (e.g., https://example.org)", &generate_default_domain())?,
    };
    // The domain is the subject of the claim, so it must be an absolute URI
    crate::iri(&domain).context("Invalid domain")?;

    let signed = write_claim(config, vec![create_initial_triple(&domain)], config.server.clone(), &config.domain, None)?;
    println!("Domain initialized with a claim by the server:");
    for triple in signed.envelope().claim().triples() {
        println!("{triple} .");
    }
    print_accepted(&signed);
    Ok(())
}

/// `prefix add`: adds a prefix that IRIs may be written with.
pub fn prefix_command(config: &Config, prefix: &str, uri: &str) -> Result {
    add_prefix(&config.prefix_file, prefix, uri).context("Error writing to prefix file")?;
    println!("Added prefix '{prefix}' for URI '{uri}'");
    println!("Prefixes stored in: {}", config.prefix_file);
    Ok(())
}

/// `purge`: deletes everything the instance keeps: its log, its store, its keys, its
/// prefixes and its configuration. Asks first unless told not to.
pub fn purge_command(config: &Config, env_path: &str, yes: bool) -> Result {
    if !yes && !confirm("WARNING: This will delete all Scatterbase data: the log, the store, every key and the configuration!")? {
        println!("Purge cancelled.");
        return Ok(());
    }
    let instance = config.instance();
    let remove = |what: &str, path: &Path| -> Result {
        let removed = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
        match removed {
            Ok(()) => println!("Deleted {what}: {}", path.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => println!("No {what} to delete: {}", path.display()),
            Err(e) => return Err(format!("Error deleting {what} {}: {e}", path.display()).into()),
        }
        Ok(())
    };

    remove("log", &instance.log_dir)?;
    remove("storage file", Path::new(&instance.storage_file))?;
    remove("storage position", Path::new(&format!("{}.offset", instance.storage_file)))?;
    remove("prefix file", Path::new(&config.prefix_file))?;
    remove("actor keys", &config.key_dir().join("actors"))?;
    remove("public key file", Path::new(&config.public_key_file))?;
    remove("private key file", Path::new(&config.secret_key_file))?;
    // The configuration goes last, so that a purge that failed can be run again
    remove("configuration", Path::new(env_path))?;
    println!("Scatterbase data purged.");
    Ok(())
}
