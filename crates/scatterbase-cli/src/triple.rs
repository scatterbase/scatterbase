//! The `triple` command.

use std::env;

use oxrdf::NamedNode;
use oxrdf::Literal;
use oxrdf::Triple;
use scatterbase_kernel::triple::TripleExt;
use scatterbase_kernel::storage;

/// Handles the "triple write" command
pub fn handle_triple_write_command(subject: &str, property: &str, object: &str, env_path: &str) {
    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        eprintln!("Error loading .env file at {}: {}", env_path, e);
        eprintln!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            eprintln!("STORAGE_FILE not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Create URI objects
    let subject_uri = match NamedNode::new(subject) {
        Ok(uri) => uri,
        Err(e) => {
            eprintln!("Invalid subject URI: {}", e);
            return;
        }
    };

    let property_uri = match NamedNode::new(property) {
        Ok(uri) => uri,
        Err(e) => {
            eprintln!("Invalid property URI: {}", e);
            return;
        }
    };

    // Check if object is a literal (enclosed in quotes) or a URI
    let triple = if object.starts_with('"') && object.ends_with('"') {
        // It's a literal - remove the quotes and create a literal object
        let literal_value = &object[1..object.len()-1];
        let literal = Literal::new_simple_literal(literal_value);
        Triple::new(subject_uri, property_uri, literal)
    } else {
        // Try to parse as URI
        let object_uri = match NamedNode::new(object) {
            Ok(uri) => uri,
            Err(e) => {
                eprintln!("Invalid object URI: {}", e);
                return;
            }
        };
        Triple::new(subject_uri, property_uri, object_uri)
    };

    // Store the triple
    let stored = storage::open(&storage_file).and_then(|mut store| storage::insert(&mut store, [triple.clone()]));
    if let Err(e) = stored {
        eprintln!("Error storing triple: {}", e);
        return;
    }

    println!("Triple stored successfully:");
    println!("{}", triple.to_ntriples());
}
