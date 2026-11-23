//! The `log` command.

use std::io;

use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::projection;

use crate::config::Config;

/// Handles the "log verify" command: checks the log against the server's key.
pub fn handle_log_verify_command(env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let instance = config.instance();
    let log = ClaimLog::open(&instance)?;
    let size = log.verify(&config.server_key()?.verifying_key())?;
    println!("Log verified: {} claims under the checkpoint of {}", size, log.origin());
    for (name, partition) in log.graphs()? {
        match partition {
            Some(partition) => println!("  graph {}: partition {:016x}", name, partition),
            None => println!("  graph {}: projection", name),
        }
    }
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
        ]),
        Some(vec![
            "The log holds the server's key and graph registry in its server partition and every",
            "accepted claim in its claim partition. The storage file is derived from the claims.",
            "Triples written with 'triple write' or 'domain init' are not claims, so they are not",
            "in the log and a rebuild does not bring them back."
        ]),
        Some(vec![
            "scatterbase log verify",
            "scatterbase log rebuild"
        ]),
        true
    );
}
