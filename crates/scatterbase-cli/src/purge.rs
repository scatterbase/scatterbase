//! Purge module for Scatterbase
//! 
//! This module provides functionality for purging a Scatterbase instance.

use std::env;
use std::fs;

/// Handles the "purge" command
pub fn handle_purge_command(env_path: &str) {
    println!("WARNING: This will delete all Scatterbase data!");
    println!("Type 'yes' to confirm or anything else to cancel.");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    if input.trim() != "yes" {
        println!("Purge cancelled.");
        return;
    }

    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        println!("Error loading .env file at {}: {}", env_path, e);
        println!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("STORAGE_FILE not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Get prefix file from environment
    let prefix_file = match env::var("PREFIX_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("PREFIX_FILE not set in environment. Using default.");
            "data/prefixes.ttl".to_string()
        }
    };

    // Get key files from environment
    let public_key_file = match env::var("ED25519_PUBLIC_KEY_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("ED25519_PUBLIC_KEY_FILE not set in environment. Using default.");
            "keys/scatterbase.ed25519.public".to_string()
        }
    };

    let private_key_file = match env::var("ED25519_SECRET_KEY_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("ED25519_SECRET_KEY_FILE not set in environment. Using default.");
            "keys/scatterbase.ed25519.private".to_string()
        }
    };

    // Delete storage file if it exists
    if let Err(e) = fs::remove_file(&storage_file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            println!("Error deleting storage file: {}", e);
        } else {
            println!("Storage file not found: {}", storage_file);
        }
    } else {
        println!("Deleted storage file: {}", storage_file);
    }

    // Delete prefix file if it exists
    if let Err(e) = fs::remove_file(&prefix_file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            println!("Error deleting prefix file: {}", e);
        } else {
            println!("Prefix file not found: {}", prefix_file);
        }
    } else {
        println!("Deleted prefix file: {}", prefix_file);
    }

    // Delete public key file if it exists
    if let Err(e) = fs::remove_file(&public_key_file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            println!("Error deleting public key file: {}", e);
        } else {
            println!("Public key file not found: {}", public_key_file);
        }
    } else {
        println!("Deleted public key file: {}", public_key_file);
    }

    // Delete private key file if it exists
    if let Err(e) = fs::remove_file(&private_key_file) {
        if e.kind() != std::io::ErrorKind::NotFound {
            println!("Error deleting private key file: {}", e);
        } else {
            println!("Private key file not found: {}", private_key_file);
        }
    } else {
        println!("Deleted private key file: {}", private_key_file);
    }

    // Delete .env file at the end
    if let Err(e) = fs::remove_file(env_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            println!("Error deleting .env file: {}", e);
        } else {
            println!(".env file not found: {}", env_path);
        }
    } else {
        println!("Deleted .env file: {}", env_path);
    }

    println!("Purge completed.");
}