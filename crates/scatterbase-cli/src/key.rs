//! The `key` command.

use std::fs;
use std::io;
use std::path::Path;

use ed25519_dalek::VerifyingKey;
use oxrdf::NamedNode;
use scatter_actors::signing::KeyRecord;
use scatter_actors::time::Timestamp;
use scatter_claims::id::zbase32;
use scatterbase_kernel::constants::DEFAULT_PUBLIC_KEY_FILE;
use scatterbase_kernel::keys;
use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::projection;
use scatterbase_kernel::uri::parse_iri;

use crate::config::Config;

/// The options of the key commands.
#[derive(Default)]
struct Options {
    attribution: Option<NamedNode>,
    file: Option<String>,
    key_id: Option<String>,
    expires: Option<Timestamp>,
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

impl Options {
    fn parse(args: &[String], allowed: &[&str]) -> io::Result<Self> {
        let mut options = Self::default();
        let mut i = 3; // Start after "key <command>"
        while i < args.len() {
            let name = match args[i].as_str() {
                "-a" => "--attribution",
                "-f" => "--file",
                other => other,
            };
            if name != "--env" && !allowed.contains(&name) {
                return Err(invalid(format!("Unknown option: {}", args[i])));
            }
            let value = args.get(i + 1).ok_or_else(|| invalid(format!("Missing value for {}", name)))?;
            match name {
                "--attribution" => {
                    options.attribution = Some(parse_iri(value)
                        .map_err(|e| invalid(format!("Invalid value for --attribution: {}", e)))?);
                },
                "--file" => options.file = Some(value.clone()),
                "--key-id" => options.key_id = Some(value.clone()),
                "--expires" => {
                    let at = chrono::DateTime::parse_from_rfc3339(value)
                        .map_err(|e| invalid(format!("Invalid value for --expires: {} (write a time as 2027-01-31T00:00:00Z)", e)))?;
                    let micros = u64::try_from(at.timestamp_micros())
                        .map_err(|_| invalid("Invalid value for --expires: before 1970".to_string()))?;
                    options.expires = Some(Timestamp(micros));
                },
                // --env was read before the command was dispatched
                _ => {},
            }
            i += 2;
        }
        Ok(options)
    }

    fn required_attribution(&self) -> io::Result<NamedNode> {
        self.attribution.clone()
            .ok_or_else(|| invalid("Attribution URI is required. Use --attribution to specify.".to_string()))
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
    let options = Options::parse(args, &["--attribution", "--file", "--expires"])?;
    let (attribution, file) = (options.required_attribution()?, options.file.clone());
    let instance = config.instance();
    if attribution == instance.server {
        return Err(invalid("The server signs with its own key, which the log registers. To replace it, use: scatterbase key rotate".to_string()));
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

    // Determine the key to register: the one in the file named, or a new one. A new key
    // each time: one that was revoked is never registered again.
    let (signing_key, generated) = match &file {
        Some(key_file) => {
            println!("Using key from file: {}", key_file);
            (keys::read_signing_key(key_file)?, None)
        },
        None => {
            // The new key is written beside its file first, and takes the file's place
            // only once the log has registered it: a registration that fails leaves the
            // actor's key file as it was
            let path = keys::actor_key_file(config.key_dir(), attribution.as_ref());
            let next = path.with_extension("next");
            let key = keys::generate();
            keys::write_signing_key(&next, &key)?;
            (key, Some((next, path)))
        },
    };
    let verifying_key = signing_key.verifying_key();

    // The server writes the registration
    let registered = config.server_key()
        .and_then(|server_key| log.register_key_until(&attribution, &verifying_key, options.expires, &server_key));
    let record = match registered {
        Ok(record) => record,
        Err(e) => {
            if let Some((next, _)) = &generated {
                fs::remove_file(next).ok();
            }
            return Err(e);
        },
    };
    if let Some((next, path)) = &generated {
        fs::rename(next, path)?;
        println!("Generated a new key, stored in: {}", path.display());
    }
    projection::catch_up(&instance, &log)?;

    println!("Key registered for attribution URI: {}", attribution.as_str());
    println!("Public key: {}", keys::public_key_iri(&verifying_key).as_str());
    println!("Key ID: {}", record.key_id);
    if let Some(until) = record.valid_until {
        println!("Valid until: {}", until);
    }
    if let Some(surrogate) = log.surrogate_of(&attribution)? {
        println!("Known in the log as: actor:{}", surrogate);
    }

    Ok(())
}

/// Handles the "key revoke" command
/// This command revokes the keys registered for an attribution URI: the one named by
/// --key-id, or all of them. Claims the keys signed stay in the log; the keys sign no more.
pub fn handle_key_revoke_command(args: &[String], env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let options = Options::parse(args, &["--attribution", "--key-id"])?;
    let attribution = options.required_attribution()?;
    let instance = config.instance();

    let mut log = ClaimLog::open(&instance)?;
    let server_key = config.server_key()?;
    let revoked = match &options.key_id {
        Some(key_id) => log.revoke_key(&attribution, key_id, &server_key)?.into_iter().collect(),
        None => log.revoke_keys(&attribution, &server_key)?,
    };
    projection::catch_up(&instance, &log)?;

    if revoked.is_empty() {
        match &options.key_id {
            Some(key_id) => println!("{} has no key {} now.", attribution.as_str(), key_id),
            None => println!("No key is registered for attribution URI: {}", attribution.as_str()),
        }
    }
    for record in revoked {
        println!("Revoked key {} of attribution URI: {}", record.key_id, attribution.as_str());
    }
    Ok(())
}

/// Handles the "key list" command
/// This command lists the server's keys, and with --attribution the keys of an actor.
pub fn handle_key_list_command(args: &[String], env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let options = Options::parse(args, &["--attribution"])?;
    let log = ClaimLog::open(&config.instance())?;

    let Some(actor) = options.attribution.filter(|actor| *actor != config.server) else {
        let keys = log.server_keys();
        println!("Keys of the server, {}:", config.server.as_str());
        for (i, (key_id, key)) in keys.iter().enumerate() {
            let status = if i + 1 == keys.len() { "current" } else { "replaced" };
            println!("  {}  {}  {}", key_id, status, keys::public_key_iri(key).as_str());
        }
        return Ok(());
    };

    // The latest record for each key ID says what has become of the key
    let mut latest: Vec<KeyRecord> = Vec::new();
    for record in log.key_records(&actor)? {
        latest.retain(|key| key.key_id != record.key_id);
        latest.push(record);
    }
    if latest.is_empty() {
        println!("No key has been registered for {}.", actor.as_str());
        return Ok(());
    }
    let now = Timestamp(u64::try_from(chrono::Utc::now().timestamp_micros()).unwrap_or(0));
    println!("Keys of {}:", actor.as_str());
    for key in latest {
        let status = if key.revoked {
            "revoked".to_string()
        } else if !key.is_valid_at(now) {
            "expired".to_string()
        } else {
            match key.valid_until {
                Some(until) => format!("current until {}", until),
                None => "current".to_string(),
            }
        };
        let public = keys::public_key_iri(&VerifyingKey::from_bytes(&key.public_key.0)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?);
        println!("  {}  {}  {}", key.key_id, status, public.as_str());
    }
    Ok(())
}

/// Handles the "key rotate" command
/// This command replaces the server's key with a new one. The log records the new key,
/// signed by the old one, and the new key takes the old key's file.
pub fn handle_key_rotate_command(args: &[String], env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    Options::parse(args, &[])?;
    let mut log = ClaimLog::open(&config.instance())?;
    let next_file = format!("{}.next", config.secret_key_file);

    // A rotation that the log recorded but that stopped before the new key took the old
    // key's file is finished; one that stopped before the log recorded it is forgotten
    if Path::new(&next_file).exists() {
        if keys::read_signing_key(&next_file)?.verifying_key() == log.server_key() {
            fs::rename(&next_file, &config.secret_key_file)?;
            write_public_key(&log.server_key())?;
            println!("Finished a rotation that was interrupted: the server's key is {}", keys::public_key_iri(&log.server_key()).as_str());
            return Ok(());
        }
        fs::remove_file(&next_file)?;
    }

    let current = config.server_key()?;
    let new = keys::generate();
    // The new key is on disk before the log names it, so it cannot be lost in between
    keys::write_signing_key(&next_file, &new)?;
    log.rotate_server_key(&new, &current)?;
    fs::rename(&next_file, &config.secret_key_file)?;
    write_public_key(&new.verifying_key())?;

    println!("The server's key was replaced.");
    println!("New public key: {}", keys::public_key_iri(&new.verifying_key()).as_str());
    println!("The old key signs nothing more. What it signed still verifies, and its private half is gone.");
    Ok(())
}

/// Writes the server's public key to the file the configuration names for it.
fn write_public_key(key: &VerifyingKey) -> io::Result<()> {
    let file = std::env::var("ED25519_PUBLIC_KEY_FILE").unwrap_or_else(|_| DEFAULT_PUBLIC_KEY_FILE.to_string());
    fs::write(file, zbase32(&key.to_bytes()))
}

/// Prints help information for the key command
pub fn print_key_help() {
    crate::cli::print_help_info(
        "key",
        "scatterbase key <command> [options]",
        Some(vec![
            ("add", "Register a key for an attribution URI"),
            ("revoke", "Revoke the keys registered for an attribution URI, or one of them"),
            ("list", "List the server's keys, or the keys of an attribution URI"),
            ("rotate", "Replace the server's key with a new one"),
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
            "scatterbase key add --attribution https://example.org/carol --expires 2027-12-31T00:00:00Z",
            "scatterbase key revoke --attribution https://example.org/alice",
            "scatterbase key revoke --attribution https://example.org/alice --key-id <key ID>",
            "scatterbase key list --attribution https://example.org/alice",
            "scatterbase key list",
            "scatterbase key rotate"
        ]),
        true
    );
}
