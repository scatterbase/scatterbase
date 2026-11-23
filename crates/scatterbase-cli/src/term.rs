//! The `term` command.

use std::io;
use std::path::Path;

use scatterbase_kernel::term::find_latest_claim_hash_for_term;

/// Handles the term latest command
/// 
/// This command returns the claim hash of the most recent claim to use the specified term.
/// With the NT file storage driver, the most recent additions are at the bottom of the file.
/// If the term has never been used in a claim before, the return is all zeros in Base32z encoding.
/// 
/// # Arguments
/// 
/// * `term` - The term to search for
/// * `env_path` - The path to the .env file
/// 
/// # Returns
/// 
/// The claim hash of the most recent claim to use the specified term, or all zeros if not found
pub fn handle_term_latest_command(term: &str, env_path: &str) -> io::Result<()> {
    // Load environment variables from the .env file
    dotenv::from_path(env_path).ok();

    // Get the storage file path from the environment
    let storage_file = match std::env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "STORAGE_FILE environment variable not found. Run 'scatterbase init' first."
            ));
        }
    };

    // Check if the storage file exists
    if !Path::new(&storage_file).exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Storage file not found: {}", storage_file)
        ));
    }

    // Find the most recent claim hash for the term
    match find_latest_claim_hash_for_term(&storage_file, term) {
        Ok(claim_hash) => {
            println!("{}", claim_hash);
            Ok(())
        },
        Err(err) => Err(err),
    }
}

/// Prints help information for the term command
pub fn print_term_help() {
    crate::cli::print_help_info(
        "term",
        "scatterbase term <subcommand> [options]",
        Some(vec![
            ("latest", "Returns the claim hash of the most recent claim to use the specified term"),
            ("help", "Displays this help message"),
        ]),
        Some(vec![
            "The term command provides operations related to terms in Scatterbase.",
            "Terms can be URIs, literals, or triples used in claims."
        ]),
        Some(vec![
            "scatterbase term latest <term>",
            "scatterbase term help"
        ]),
        true
    );
}
