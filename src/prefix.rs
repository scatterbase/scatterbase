use std::io::{self, Write, BufRead, BufReader};
use std::fs::{self, File};
use std::path::Path;
use std::collections::HashMap;
use std::env;
use dotenv::dotenv;

use crate::uri::rdf;
use crate::uri::vocab;
use crate::utils::{ensure_directory_exists};
use crate::cli::{prompt_for_prefix_file, prompt_user};
use crate::constants::DEFAULT_PREFIX_FILE;

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
        if let Err(e) = writeln!(file, "@prefix rdf: <{}> .", rdf::TYPE.strip_suffix("type").unwrap_or(rdf::TYPE)) {
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

/// Reads prefixes from the prefix file
pub fn read_prefixes() -> HashMap<String, String> {
    let mut prefixes = HashMap::new();

    // Load environment variables
    dotenv().ok();

    // Get prefix file path from environment or use default
    let prefix_file = env::var("PREFIX_FILE").unwrap_or_else(|_| DEFAULT_PREFIX_FILE.to_string());

    // Check if the file exists
    if !Path::new(&prefix_file).exists() {
        return prefixes;
    }

    // Open the file for reading
    let file = match File::open(&prefix_file) {
        Ok(file) => file,
        Err(_) => return prefixes,
    };

    let reader = BufReader::new(file);

    // Parse each line for prefixes
    for line in reader.lines() {
        if let Ok(line) = line {
            if line.starts_with("@prefix") {
                // Parse the prefix line
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let prefix = parts[1].trim_end_matches(':');
                    let uri = parts[2].trim_start_matches('<').trim_end_matches('>');
                    prefixes.insert(prefix.to_string(), uri.to_string());
                }
            }
        }
    }

    prefixes
}

/// Expands a prefixed URI using the prefix map
pub fn expand_prefix(uri_str: &str, prefixes: &HashMap<String, String>) -> String {
    // Check if the URI is already a full URI
    if uri_str.starts_with("http://") || uri_str.starts_with("https://") {
        return uri_str.to_string();
    }

    // Check if it's a prefixed URI
    if let Some(colon_pos) = uri_str.find(':') {
        let prefix = &uri_str[0..colon_pos];
        let local_part = &uri_str[colon_pos + 1..];

        if let Some(prefix_uri) = prefixes.get(prefix) {
            return format!("{}{}", prefix_uri, local_part);
        }
    }

    // Return as is if no prefix found
    uri_str.to_string()
}

/// Initializes the prefix file with default prefixes
pub fn init_prefix_file() -> io::Result<()> {
    // Load environment variables
    dotenv().ok();

    // Get prefix file path from environment or use default
    let prefix_file = env::var("PREFIX_FILE").unwrap_or_else(|_| DEFAULT_PREFIX_FILE.to_string());

    // Ensure the directory exists
    ensure_directory_exists(&prefix_file)?;

    // Check if the prefix file already exists
    if Path::new(&prefix_file).exists() {
        // File already exists, no need to initialize
        return Ok(());
    }

    // Create the file
    let mut file = File::create(&prefix_file)?;

    // Write standard RDF prefixes
    writeln!(file, "@prefix rdf: <{}> .", rdf::TYPE.strip_suffix("type").unwrap_or(rdf::TYPE))?;
    writeln!(file, "@prefix rdfs: <{}> .", rdf::RDFS)?;
    writeln!(file, "@prefix xsd: <{}> .", rdf::XSD)?;
    writeln!(file, "@prefix owl: <{}> .", rdf::OWL)?;
    writeln!(file, "@prefix foaf: <{}> .", vocab::FOAF)?;
    writeln!(file, "@prefix dc: <{}> .", vocab::DC)?;

    println!("Initialized prefix file with standard RDF prefixes: {}", prefix_file);

    Ok(())
}
