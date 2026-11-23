//! The `key` and `actor` commands.

use std::fs;
use std::path::Path;

use ed25519_dalek::VerifyingKey;
use oxrdf::NamedNode;
use scatter_actors::signing::KeyRecord;
use scatter_actors::time::Timestamp;
use scatter_claims::id::zbase32;
use scatterbase_kernel::claim::erase_actor;
use scatterbase_kernel::keys;
use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::projection;

use crate::claim::Why;
use crate::config::Config;
use crate::error::{Context, Result};

/// A time written as RFC 3339, `2027-12-31T00:00:00Z`.
pub fn timestamp(text: &str) -> std::result::Result<Timestamp, String> {
    let at = chrono::DateTime::parse_from_rfc3339(text)
        .map_err(|e| format!("{e} (write a time as 2027-12-31T00:00:00Z)"))?;
    u64::try_from(at.timestamp_micros()).map(Timestamp).map_err(|_| "before 1970".to_string())
}

fn now() -> Timestamp {
    Timestamp(u64::try_from(chrono::Utc::now().timestamp_micros()).unwrap_or(0))
}

/// `key add`: registers a key for an actor: the key in the file named, or a new key, which
/// is written to the actor's key file. The registration is a key record the server writes
/// to the log.
pub fn add_command(config: &Config, attribution: &NamedNode, file: Option<&str>, expires: Option<Timestamp>) -> Result {
    let instance = config.instance();
    if *attribution == instance.server {
        return Err("The server signs with its own key, which the log registers. To replace it, use: scatterbase key rotate".into());
    }
    let mut log = ClaimLog::open(&instance)?;
    if !log.current_keys(attribution)?.is_empty() {
        return Err(format!(
            "A key is already registered for attribution URI {0}. To replace it, revoke it first using: scatterbase key revoke --attribution {0}",
            attribution.as_str(),
        ).into());
    }

    // The key to register: the one in the file named, or a new one. A new key each time:
    // one that was revoked is never registered again.
    let (signing_key, generated) = match file {
        Some(key_file) => {
            println!("Using key from file: {key_file}");
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

    let registered = config.server_key()
        .and_then(|server_key| Ok(log.register_key_until(attribution, &verifying_key, expires, &server_key)?));
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
        println!("Valid until: {until}");
    }
    if let Some(surrogate) = log.surrogate_of(attribution)? {
        println!("Known in the log as: actor:{surrogate}");
    }
    Ok(())
}

/// `key revoke`: revokes the keys registered for an actor: the one named, or all of them.
/// Claims the keys signed stay in the log; the keys sign no more.
pub fn revoke_command(config: &Config, attribution: &NamedNode, key_id: Option<&str>) -> Result {
    let instance = config.instance();
    let mut log = ClaimLog::open(&instance)?;
    let server_key = config.server_key()?;
    let revoked = match key_id {
        Some(key_id) => log.revoke_key(attribution, key_id, &server_key)?.into_iter().collect(),
        None => log.revoke_keys(attribution, &server_key)?,
    };
    projection::catch_up(&instance, &log)?;

    if revoked.is_empty() {
        match key_id {
            Some(key_id) => println!("{} has no key {key_id} now.", attribution.as_str()),
            None => println!("No key is registered for attribution URI: {}", attribution.as_str()),
        }
    }
    for record in revoked {
        println!("Revoked key {} of attribution URI: {}", record.key_id, attribution.as_str());
    }
    Ok(())
}

/// `key list`: lists the server's keys, or the keys of an actor.
pub fn list_command(config: &Config, attribution: Option<&NamedNode>) -> Result {
    let log = ClaimLog::open(&config.instance())?;
    let Some(actor) = attribution.filter(|actor| **actor != config.server) else {
        let keys = log.server_keys();
        println!("Keys of the server, {}:", config.server.as_str());
        for (i, (key_id, key)) in keys.iter().enumerate() {
            let status = if i + 1 == keys.len() { "current" } else { "replaced" };
            println!("  {key_id}  {status}  {}", keys::public_key_iri(key).as_str());
        }
        return Ok(());
    };

    // The latest record for each key ID says what has become of the key
    let mut latest: Vec<KeyRecord> = Vec::new();
    for record in log.key_records(actor)? {
        latest.retain(|key| key.key_id != record.key_id);
        latest.push(record);
    }
    if latest.is_empty() {
        println!("No key has been registered for {}.", actor.as_str());
        return Ok(());
    }
    let now = now();
    println!("Keys of {}:", actor.as_str());
    for key in latest {
        let status = if key.revoked {
            "revoked".to_string()
        } else if !key.is_valid_at(now) {
            "expired".to_string()
        } else {
            match key.valid_until {
                Some(until) => format!("current until {until}"),
                None => "current".to_string(),
            }
        };
        let public = VerifyingKey::from_bytes(&key.public_key.0).context("A key record does not hold a public key")?;
        println!("  {}  {status}  {}", key.key_id, keys::public_key_iri(&public).as_str());
    }
    Ok(())
}

/// `key rotate`: replaces the server's key with a new one. The log records the new key,
/// signed by the old one, and the new key takes the old key's file.
pub fn rotate_command(config: &Config) -> Result {
    let mut log = ClaimLog::open(&config.instance())?;
    let next_file = format!("{}.next", config.secret_key_file);
    let write_public = |key: &VerifyingKey| fs::write(&config.public_key_file, zbase32(&key.to_bytes()));

    // A rotation that the log recorded but that stopped before the new key took the old
    // key's file is finished; one that stopped before the log recorded it is forgotten
    if Path::new(&next_file).exists() {
        if keys::read_signing_key(&next_file)?.verifying_key() == log.server_key() {
            fs::rename(&next_file, &config.secret_key_file)?;
            write_public(&log.server_key())?;
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
    write_public(&new.verifying_key())?;

    println!("The server's key was replaced.");
    println!("New public key: {}", keys::public_key_iri(&new.verifying_key()).as_str());
    println!("The old key signs nothing more. What it signed still verifies, and its private half is gone.");
    Ok(())
}

/// `actor erase`: erases an actor's record and keys from the log, for good. The claims the
/// actor made are not touched.
pub fn erase_actor_command(config: &Config, attribution: &NamedNode, why: &Why) -> Result {
    match erase_actor(&config.instance(), attribution, why.reason(), &config.server_key()?).context("Not erased")? {
        None => println!("Nothing to erase: the log has no record of {}.", attribution.as_str()),
        Some(surrogate) => {
            println!("Erased {} from the log: its record and its keys, which were under actor:{surrogate}.", attribution.as_str());
            println!("This cannot be undone. The claims it made still name it; erase their attestations with 'scatterbase claim erase'.");
        },
    }
    Ok(())
}
