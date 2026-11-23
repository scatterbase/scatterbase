//! The `log` command.

use std::io;
use std::path::Path;

use scatterbase_kernel::keys;
use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::uri::parse_iri;
use scatterbase_kernel::projection;

use crate::config::Config;

/// The value of an option of a log command, if it is given.
fn option<'a>(args: &'a [String], name: &str) -> io::Result<Option<&'a str>> {
    match args.iter().position(|arg| arg == name) {
        None => Ok(None),
        Some(i) => args.get(i + 1).map(|value| Some(value.as_str()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("Missing value for {}", name))),
    }
}

fn print_graphs(log: &ClaimLog) -> io::Result<()> {
    for (name, partition) in log.graphs()? {
        match partition {
            Some(partition) => println!("  graph {}: partition {:016x}", name, partition),
            None => println!("  graph {}: projection", name),
        }
    }
    Ok(())
}

/// Handles the "log verify" command: checks the log against the server's key, or with
/// --bundle a bundle, which needs no instance.
pub fn handle_log_verify_command(args: &[String], env_path: &str) -> io::Result<()> {
    if let Some(dir) = option(args, "--bundle")? {
        return verify_bundle(dir, option(args, "--key")?);
    }
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let instance = config.instance();
    let log = ClaimLog::open(&instance)?;
    let size = log.verify(&config.server_key()?.verifying_key())?;
    println!("Log verified: {} claims under the checkpoint of {}", size, log.origin());
    print_graphs(&log)
}

/// Verifies a bundle against the key named, or failing that against the key its own chain
/// ends in, which then has to be compared with one that is trusted.
fn verify_bundle(dir: &str, key: Option<&str>) -> io::Result<()> {
    let bundle = ClaimLog::open_bundle(dir)?;
    let expected = match key {
        Some(key) => {
            let iri = parse_iri(key).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            Some(keys::public_key_from_iri(iri.as_ref()).ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a public key. Write it as urn:ed25519:<key>.", key),
            ))?)
        },
        None => None,
    };
    let size = bundle.verify(&expected.unwrap_or_else(|| bundle.server_key()))?;

    println!("Bundle verified: {} claims under the checkpoint of {}", size, bundle.origin());
    println!("  server: {}", bundle.server().as_str());
    println!("  server key: {}", keys::public_key_iri(&bundle.server_key()).as_str());
    if expected.is_none() {
        println!("  The bundle is consistent with that key. Whether it is the server's is not checked:");
        println!("  compare it with a key you trust, or pass one with --key.");
    }
    print_graphs(&bundle)
}

/// Handles the "log export" command: writes a bundle of the log to a directory.
pub fn handle_log_export_command(args: &[String], env_path: &str) -> io::Result<()> {
    let dest = args.get(3).filter(|dest| !dest.starts_with("--")).ok_or_else(|| io::Error::new(
        io::ErrorKind::InvalidInput,
        "A directory is required: scatterbase log export <directory> [--backup]",
    ))?;
    let backup = args.iter().any(|arg| arg == "--backup");
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let log = ClaimLog::open(&config.instance())?;
    let partitions = log.export(Path::new(dest), backup)?;

    println!("Wrote a bundle of the {} partitions to {}", partitions.join(", "), dest);
    if backup {
        println!("This is a backup. It holds who the actors are and is not for publication.");
    } else {
        println!("It holds every claim and says nothing of who the actors are beyond what their claims say.");
    }
    println!("Verify it with: scatterbase log verify --bundle {} --key {}", dest, keys::public_key_iri(&log.server_key()).as_str());
    Ok(())
}

/// Handles the "log rebuild" command: writes the store again from the claim log.
pub fn handle_log_rebuild_command(env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let instance = config.instance();
    let log = ClaimLog::open(&instance)?;
    let size = projection::rebuild(&instance, &log)?;
    println!("Rebuilt {} from {} claims", instance.storage_file, size);
    Ok(())
}

/// Prints help information for the log command
pub fn print_log_help() {
    crate::cli::print_help_info(
        "log",
        "scatterbase log <command> [options]",
        Some(vec![
            ("verify", "Check the log: the server's key chain, the signed checkpoints, every record and every signature"),
            ("rebuild", "Write the storage file again from the claim log"),
            ("export", "Write a bundle of the log that can be verified on its own"),
        ]),
        Some(vec![
            "The log holds the server's key and graph registry in its server partition and every",
            "accepted claim in its claim partition. The storage file is derived from the claims.",
            "Parts of claims that were erased are gone from the log, and a rebuild leaves them out.",
            "A bundle is a copy of the log's server and claim partitions that anyone can verify with",
            "nothing but the server's public key. With --backup it also holds the mutable partition,",
            "which says who the actors are, and is not for publication."
        ]),
        Some(vec![
            "scatterbase log verify",
            "scatterbase log rebuild",
            "scatterbase log export ./bundle",
            "scatterbase log export ./backup --backup",
            "scatterbase log verify --bundle ./bundle --key urn:ed25519:<the server's public key>"
        ]),
        true
    );
}
