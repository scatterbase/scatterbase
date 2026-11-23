use std::io::{self, Write, BufRead, BufReader};
use std::fs::File;
use std::path::Path;
use std::collections::HashMap;
use std::env;
use dotenv::dotenv;

use scatter_vocab::{foaf, owl, rdf, rdfs, xsd};
use crate::utils::{ensure_directory_exists};
use crate::constants::DEFAULT_PREFIX_FILE;

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
    writeln!(file, "@prefix rdf: <{}> .", rdf::NS)?;
    writeln!(file, "@prefix rdfs: <{}> .", rdfs::NS)?;
    writeln!(file, "@prefix xsd: <{}> .", xsd::NS)?;
    writeln!(file, "@prefix owl: <{}> .", owl::NS)?;
    writeln!(file, "@prefix foaf: <{}> .", foaf::NS)?;
    writeln!(file, "@prefix dc: <{}> .", "http://purl.org/dc/elements/1.1/")?;

    println!("Initialized prefix file with standard RDF prefixes: {}", prefix_file);

    Ok(())
}
