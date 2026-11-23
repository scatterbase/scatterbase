//! The `key` command.

use std::io;

use oxrdf::NamedNode;
use scatterbase_kernel::keys;
use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::projection;
use scatterbase_kernel::uri::parse_iri;

use crate::config::Config;

/// The options of the key commands.
struct Options {
    attribution: NamedNode,
    file: Option<String>,
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl Options {
    fn parse(args: &[String]) -> io::Result<Self> {
        let mut attribution = None;
        let mut file = None;
        let mut i = 3; // Start after "key <command>"
        while i < args.len() {
            let value = || args.get(i + 1).ok_or_else(|| invalid(format!("Missing value for {}", args[i])));
            match args[i].as_str() {
                "--attribution" | "-a" => {
                    attribution = Some(parse_iri(value()?)
                        .map_err(|e| invalid(format!("Invalid value for --attribution: {}", e)))?);
                },
                "--file" | "-f" => file = Some(value()?.clone()),
                // --env was read before the command was dispatched
                "--env" => {},
                _ => return Err(invalid(format!("Unknown option: {}", args[i]))),
            }
            i += 2;
        }
        let attribution = attribution
            .ok_or_else(|| invalid("Attribution URI is required. Use --attribution to specify.".to_string()))?;
        Ok(Self { attribution, file })
    }
}

/// Handles the "key add" command
/// This command registers a key for an attribution URI: the key in the file named by
/// --file, or a new key, which is written to the actor's key file. The registration is a
/// key record the server writes to the log.
///
/// # Arguments
///
/// * `args` - The command line arguments
/// * `env_path` - The path to the .env file
pub fn handle_key_add_command(args: &[String], env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let Options { attribution, file } = Options::parse(args)?;
    let instance = config.instance();
    if attribution == instance.server {
        return Err(invalid("The server signs with its own key, which the log's founding record registers.".to_string()));
    }

    // Check if the attribution URI already has a key
    let mut log = ClaimLog::open(&instance)?;
    if !log.current_keys(&attribution)?.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "A key is already registered for attribution URI {}. To replace it, revoke it first using: scatterbase key revoke --attribution {}",
                attribution.as_str(), attribution.as_str(),
            ),
        ));
    }

    // Determine the key to register
    let signing_key = match &file {
        Some(key_file) => {
            println!("Using key from file: {}", key_file);
            keys::read_signing_key(key_file)?
        },
        None => {
            // A new key each time: one that was revoked is never registered again
            let path = keys::actor_key_file(config.key_dir(), attribution.as_ref());
            let key = keys::generate();
            keys::write_signing_key(&path, &key)?;
            println!("Generated a new key, stored in: {}", path.display());
            key
        },
    };
    let verifying_key = signing_key.verifying_key();

    // The server writes the registration
    let record = log.register_key(&attribution, &verifying_key, &config.server_key()?)?;
    projection::catch_up(&instance, &log)?;

    println!("Key registered for attribution URI: {}", attribution.as_str());
    println!("Public key: {}", keys::public_key_iri(&verifying_key).as_str());
    println!("Key ID: {}", record.key_id);
    if let Some(surrogate) = log.surrogate_of(&attribution)? {
        println!("Known in the log as: actor:{}", surrogate);
    }

    Ok(())
}

/// Handles the "key revoke" command
/// This command revokes every key registered for an attribution URI. Claims the keys
/// signed stay in the log; the keys sign no more.
pub fn handle_key_revoke_command(args: &[String], env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let Options { attribution, .. } = Options::parse(args)?;
    let instance = config.instance();

    let mut log = ClaimLog::open(&instance)?;
    let revoked = log.revoke_keys(&attribution, &config.server_key()?)?;
    projection::catch_up(&instance, &log)?;

    if revoked.is_empty() {
        println!("No key is registered for attribution URI: {}", attribution.as_str());
    }
    for record in revoked {
        println!("Revoked key {} of attribution URI: {}", record.key_id, attribution.as_str());
    }
    Ok(())
}

/// Prints help information for the key command
pub fn print_key_help() {
    crate::cli::print_help_info(
        "key",
        "scatterbase key <command> [options]",
        Some(vec![
            ("add", "Register a key for an attribution URI"),
            ("revoke", "Revoke the keys registered for an attribution URI"),
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
            "scatterbase key add --attribution https://example.org/bob --file bob_key.private",
            "scatterbase key revoke --attribution https://example.org/alice"
        ]),
        true
    );
}
