//! The `key` command.

use std::io;

use scatterbase_kernel::claim::{prepare, submit};
use scatterbase_kernel::keys;
use scatterbase_kernel::lookup::Snapshot;
use scatterbase_kernel::uri::parse_iri;

use crate::config::Config;

/// Handles the "key add" command
/// This command registers a key for an attribution URI: the key in the file named by
/// --file, or a new key, which is written to the actor's key file. The registration is a
/// claim by the server.
///
/// # Arguments
///
/// * `args` - The command line arguments
/// * `env_path` - The path to the .env file
pub fn handle_key_add_command(args: &[String], env_path: &str) -> io::Result<()> {
    let invalid = |message: String| io::Error::new(io::ErrorKind::InvalidInput, message);
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;

    // Parse command arguments
    let mut attribution = None;
    let mut key_file = None;
    let mut i = 3; // Start after "key add"
    while i < args.len() {
        let value = || args.get(i + 1).ok_or_else(|| invalid(format!("Missing value for {}", args[i])));
        match args[i].as_str() {
            "--attribution" | "-a" => {
                attribution = Some(parse_iri(value()?)
                    .map_err(|e| invalid(format!("Invalid value for --attribution: {}", e)))?);
            },
            "--file" | "-f" => key_file = Some(value()?.clone()),
            // --env was read before the command was dispatched
            "--env" => {},
            _ => return Err(invalid(format!("Unknown option: {}", args[i]))),
        }
        i += 2;
    }
    let attribution = attribution
        .ok_or_else(|| invalid("Attribution URI is required. Use --attribution to specify.".to_string()))?;

    // Check if the attribution URI already has a key
    let store = Snapshot::load(&config.instance())?;
    if store.public_key(attribution.as_ref()).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("A key is already registered for attribution URI {}. To update the key, submit a key update via claim.", attribution.as_str()),
        ));
    }

    // Determine the key to register
    let signing_key = match &key_file {
        Some(key_file) => {
            println!("Using key from file: {}", key_file);
            keys::read_signing_key(key_file)?
        },
        None => {
            let path = keys::actor_key_file(config.key_dir(), attribution.as_ref());
            if path.exists() {
                println!("Using existing key file: {}", path.display());
                keys::read_signing_key(&path)?
            } else {
                let key = keys::generate();
                keys::write_signing_key(&path, &key)?;
                println!("Generated a new key, stored in: {}", path.display());
                key
            }
        },
    };
    let verifying_key = signing_key.verifying_key();

    // The server claims the registration
    let server_key = config.server_key()?;
    let envelope = prepare(
        keys::registration(&attribution, &verifying_key),
        config.server.clone(),
        &config.domain,
        &store,
        &server_key,
    ).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let signed = submit(envelope, &config.instance(), &server_key)?;

    println!("Key registered for attribution URI: {}", attribution.as_str());
    println!("Public key: {}", keys::public_key_iri(&verifying_key).as_str());
    println!("Claim: {}", signed.id());

    Ok(())
}

/// Prints help information for the key command
pub fn print_key_help() {
    crate::cli::print_help_info(
        "key",
        "scatterbase key <command> [options]",
        Some(vec![
            ("add", "Register a key for an attribution URI"),
        ]),
        Some(vec![
            "The key command provides operations for managing cryptographic keys.",
            "Keys are used to sign claims and verify their authenticity.",
            "Without --file, 'key add' generates a new key for the attribution URI and keeps it",
            "with the server's key; 'claim prepare' and 'claim write' then sign with it."
        ]),
        Some(vec![
            "scatterbase key add --attribution <uri> [--file <keyfile>]",
            "scatterbase key add --attribution https://example.org/alice",
            "scatterbase key add --attribution https://example.org/bob --file bob_key.private"
        ]),
        true
    );
}
