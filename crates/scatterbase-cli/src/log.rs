//! The `log` command.

use std::io;

use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::projection;

use crate::config::Config;

/// Handles the "log verify" command: checks the claim log against the server's key.
pub fn handle_log_verify_command(env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let instance = config.instance();
    let log = ClaimLog::open(&instance.log_dir, &instance.domain)?;
    let size = log.verify(&config.server_key()?.verifying_key())?;
    println!("Claim log verified: {} claims under the checkpoint of {}", size, log.origin());
    Ok(())
}

/// Handles the "log rebuild" command: writes the store again from the claim log.
pub fn handle_log_rebuild_command(env_path: &str) -> io::Result<()> {
    let config = Config::load(env_path).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let instance = config.instance();
    let log = ClaimLog::open(&instance.log_dir, &instance.domain)?;
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
            ("verify", "Check the claim log: its signed checkpoint, every record and every signature"),
            ("rebuild", "Write the storage file again from the claim log"),
        ]),
        Some(vec![
            "The claim log holds every accepted claim and is what the storage file is derived from.",
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
