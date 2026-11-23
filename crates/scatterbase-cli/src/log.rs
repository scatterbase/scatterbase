//! The `log` and `term` commands.

use std::path::Path;

use ed25519_dalek::VerifyingKey;
use oxrdf::NamedNode;
use scatterbase_kernel::keys;
use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::lookup::Snapshot;
use scatterbase_kernel::projection;

use crate::config::Config;
use crate::error::Result;

/// A public key written as its IRI, `urn:ed25519:{key}`.
pub fn public_key(text: &str) -> std::result::Result<VerifyingKey, String> {
    crate::iri(text).ok()
        .and_then(|iri| keys::public_key_from_iri(iri.as_ref()))
        .ok_or_else(|| "not a public key: write it as urn:ed25519:<key>".to_string())
}

fn print_graphs(log: &ClaimLog) -> Result {
    for (name, partition) in log.graphs()? {
        match partition {
            Some(partition) => println!("  graph {name}: partition {partition:016x}"),
            None => println!("  graph {name}: projection"),
        }
    }
    Ok(())
}

/// `log verify`: checks the log against the server's key.
pub fn verify_command(config: &Config) -> Result {
    let log = ClaimLog::open(&config.instance())?;
    let size = log.verify(&config.server_key()?.verifying_key())?;
    println!("Log verified: {size} claims under the checkpoint of {}", log.origin());
    print_graphs(&log)
}

/// `log verify --bundle`: checks a bundle, which needs no instance, against the key named,
/// or failing that against the key its own chain ends in.
pub fn verify_bundle_command(dir: &str, key: Option<&VerifyingKey>) -> Result {
    let bundle = ClaimLog::open_bundle(dir)?;
    let size = bundle.verify(key.unwrap_or(&bundle.server_key()))?;

    println!("Bundle verified: {size} claims under the checkpoint of {}", bundle.origin());
    println!("  server: {}", bundle.server().as_str());
    println!("  server key: {}", keys::public_key_iri(&bundle.server_key()).as_str());
    if key.is_none() {
        println!("  The bundle is consistent with that key. Whether it is the server's is not checked:");
        println!("  compare it with a key you trust, or pass one with --key.");
    }
    print_graphs(&bundle)
}

/// `log rebuild`: writes the store again from the claim log.
pub fn rebuild_command(config: &Config) -> Result {
    let instance = config.instance();
    let log = ClaimLog::open(&instance)?;
    let size = projection::rebuild(&instance, &log)?;
    println!("Rebuilt {} from the log, whose claim partition has {size} records", instance.storage_file);
    Ok(())
}

/// `log export`: writes a bundle of the log to a directory.
pub fn export_command(config: &Config, directory: &str, backup: bool) -> Result {
    let log = ClaimLog::open(&config.instance())?;
    let partitions = log.export(Path::new(directory), backup)?;

    println!("Wrote a bundle of the {} partitions to {directory}", partitions.join(", "));
    if backup {
        println!("This is a backup. It holds who the actors are and is not for publication.");
    } else {
        println!("It holds every claim and says nothing of who the actors are beyond what their claims say.");
    }
    println!("Verify it with: scatterbase log verify --bundle {directory} --key {}", keys::public_key_iri(&log.server_key()).as_str());
    Ok(())
}

/// `term latest`: prints the ID of the claim that last used a term, or the ID of no claim.
pub fn latest_command(config: &Config, term: &NamedNode) -> Result {
    if !Path::new(&config.storage_file).exists() {
        return Err(format!("Storage file not found: {}", config.storage_file).into());
    }
    println!("{}", Snapshot::load(&config.instance())?.latest_claim_using(term.as_ref()));
    Ok(())
}
