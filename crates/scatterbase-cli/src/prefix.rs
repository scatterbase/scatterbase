//! The `prefix` command.

use std::io::Write;
use std::fs;
use std::path::Path;
use std::env;
use dotenv::dotenv;

use scatter_vocab::rdf;
use scatterbase_kernel::utils::ensure_directory_exists;
use scatterbase_kernel::constants::DEFAULT_PREFIX_FILE;
use crate::cli::prompt_user;

/// Handles the "prefix add" command
pub fn handle_prefix_add_command(prefix: &str, uri: &str) {
    // Load environment variables
    dotenv().ok();

    // Get prefix file path from environment or use default
    let prefix_file = env::var("PREFIX_FILE").unwrap_or_else(|_| DEFAULT_PREFIX_FILE.to_string());

    // Ensure the directory exists
    if let Err(e) = ensure_directory_exists(&prefix_file) {
        println!("Error creating prefix file directory: {}", e);
        return;
    }

    // Check if the prefix file exists
    let file_exists = Path::new(&prefix_file).exists();

    // Open the file for writing (create if it doesn't exist)
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(true)
        .open(&prefix_file) {
        Ok(file) => file,
        Err(e) => {
            println!("Error opening prefix file: {}", e);
            return;
        }
    };

    // If the file is new, write the Turtle preamble
    if !file_exists {
        if let Err(e) = writeln!(file, "@prefix rdf: <{}> .", rdf::NS) {
            println!("Error writing to prefix file: {}", e);
            return;
        }
    }

    // Write the new prefix
    if let Err(e) = writeln!(file, "@prefix {}: <{}> .", prefix, uri) {
        println!("Error writing to prefix file: {}", e);
        return;
    }

    println!("Added prefix '{}' for URI '{}'", prefix, uri);
    println!("Prefixes stored in: {}", prefix_file);
}

/// Prompts for a prefix with an optional default
pub fn prompt_for_prefix(default: Option<&str>) -> String {
    prompt_user("Enter prefix (e.g., schema)", default, true)
}
