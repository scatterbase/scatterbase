//! The `claim` commands: prepare, submit and write.

use std::io::{self, Read};
use std::fs;
use std::collections::{HashMap, HashSet};
use std::env;
use chrono::{Utc, SecondsFormat};

use oxrdf::Triple;
use scatterbase_kernel::triple::TripleExt;
use oxrdf::{NamedNode, NamedOrBlankNode};
use scatterbase_kernel::uri::{parse_iri, rdf};
use scatterbase_kernel::uri::prov;
use scatterbase_kernel::uri::scatter;
use oxrdf::Literal;
use oxrdf::Term;
use scatterbase_kernel::term::TermExt;
use scatterbase_kernel::crypto::{get_server_signing_key, sign_with_key, hash_string, derive_key_from_server};
use scatterbase_kernel::prefix::read_prefixes;
use scatterbase_kernel::storage::create_storage;
use scatterbase_kernel::claim::{
    ClaimEnvelope, SignedClaim, parse_turtle, hash_triple, generate_claim_hash,
    read_triples_from_file, read_triples_from_stdin, set_current_claim_hash,
    update_blank_node_uris, generate_unique_sequence_id,
};

/// Handles the "claim prepare" command
/// This command accepts a filename or stdin with NT or TTL triples,
/// and returns all the triples of the ClaimEnvelope in TTL form to stdout.
/// If a provenance URI is not specified, use domain/Server by default.
pub fn handle_claim_prepare_command(args: &[String], env_path: &str) {
    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        println!("Error loading .env file at {}: {}", env_path, e);
        println!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get domain from environment
    let domain = match env::var("DOMAIN") {
        Ok(domain) => domain,
        Err(_) => {
            println!("DOMAIN not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Get server URI from environment
    let server_uri = match env::var("SERVER_URI") {
        Ok(uri) => uri,
        Err(_) => {
            println!("SERVER_URI not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("STORAGE_FILE not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Parse command arguments
    let mut attribution = String::new();
    let mut prev_claim = String::new();
    let mut subject_prev = String::new();
    let mut object_prev = String::new();
    let mut input_file = String::new();
    let mut custom_domain = String::new();
    let mut private_key_file = String::new();
    let mut i = 3; // Start after "claim prepare"

    while i < args.len() {
        match args[i].as_str() {
            "--attribution" | "-a" => {
                if i + 1 < args.len() {
                    attribution = match parse_iri(&args[i + 1]) {
                        Ok(uri) => uri.into_string(),
                        Err(e) => {
                            println!("Invalid value for --attribution: {}", e);
                            return;
                        }
                    };
                    i += 2;
                } else {
                    println!("Missing value for --attribution");
                    return;
                }
            },
            "--subject-prev" => {
                if i + 1 < args.len() {
                    subject_prev = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --subject-prev");
                    return;
                }
            },
            "--object-prev" => {
                if i + 1 < args.len() {
                    object_prev = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --object-prev");
                    return;
                }
            },
            "--file" | "-f" => {
                if i + 1 < args.len() {
                    input_file = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --file");
                    return;
                }
            },
            "--domain" | "-d" => {
                if i + 1 < args.len() {
                    custom_domain = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --domain");
                    return;
                }
            },
            "--private-key" | "-k" => {
                if i + 1 < args.len() {
                    private_key_file = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --private-key");
                    return;
                }
            },
            "--env" => {
                // Skip the --env option and its value as it's already processed
                i += 2;
            },
            _ => {
                println!("Unknown option: {}", args[i]);
                return;
            }
        }
    }

    // If a custom domain was provided, use that instead of the environment variable
    let domain = if !custom_domain.is_empty() {
        custom_domain
    } else {
        domain
    };

    // If no attribution is provided, use domain/Server by default
    if attribution.is_empty() {
        attribution = format!("{}/Server", domain);
    }

    // If no previous claim hash is provided, use all zeros in Base32z format
    if prev_claim.is_empty() {
        // A SHA-256 hash is 32 bytes (256 bits)
        // In Base32z encoding, each 5 bits are represented by 1 character
        // So 32 bytes (256 bits) would be represented by ceil(256/5) = 52 characters
        // 'y' is the first character in the Base32z alphabet, representing 0
        prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    }

    // If no subject previous claim hash is provided, use all y's in Base32z format
    if subject_prev.is_empty() {
        subject_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    }

    // If no object previous claim hash is provided, use all y's in Base32z format
    if object_prev.is_empty() {
        object_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    }

    // Read prefixes
    let prefixes = read_prefixes();

    // Read triples from file or stdin
    let triples = if !input_file.is_empty() {
        match read_triples_from_file(&input_file, &prefixes) {
            Ok(triples) => triples,
            Err(e) => {
                println!("Error reading triples from file: {}", e);
                return;
            }
        }
    } else {
        match read_triples_from_stdin(&prefixes) {
            Ok(triples) => triples,
            Err(e) => {
                println!("Error reading triples from stdin: {}", e);
                return;
            }
        }
    };

    if triples.is_empty() {
        println!("No triples found in input.");
        return;
    }

    // Hash each triple
    let mut triple_hashes = Vec::new();
    for triple in &triples {
        triple_hashes.push(hash_triple(triple));
    }

    // Generate an initial claim hash (will be regenerated after finding previous claims)
    let initial_claim_hash = generate_claim_hash(&triple_hashes, &attribution, &prev_claim, "", "");

    // Set the current claim hash for blank node canonicalization
    set_current_claim_hash(&initial_claim_hash);

    // Update any blank node URIs in the triples to use the actual claim hash
    let updated_triples = update_blank_node_uris(triples, &initial_claim_hash);

    // Filter out duplicate triples
    let mut unique_triples = Vec::new();
    let mut seen_triples = HashSet::new();
    for triple in &updated_triples {
        let triple_str = triple.to_ntriples();
        if !seen_triples.contains(&triple_str) {
            seen_triples.insert(triple_str);
            unique_triples.push(triple.clone());
        }
    }

    // For each URI used in subject or object position, look up the claim hash of the latest claim to use that URI
    let mut uri_to_claim_hash = HashMap::new();
    for triple in &unique_triples {
        // Check subject
        if let Some(uri) = triple.subject.as_named_node() {
            let uri_str = uri.as_str().to_string();
            if !uri_to_claim_hash.contains_key(&uri_str) {
                match scatterbase_kernel::term::find_latest_claim_hash_for_term(&storage_file, &uri_str) {
                    Ok(claim_hash) => {
                        uri_to_claim_hash.insert(uri_str, claim_hash);
                    },
                    Err(e) => {
                        eprintln!("Error looking up claim hash for URI {}: {}", uri_str, e);
                        // Use a placeholder hash when the lookup fails
                        uri_to_claim_hash.insert(uri_str, "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string());
                    }
                }
            }
        }

        // Check object
        if let Some(uri) = triple.object.as_named_node() {
            let uri_str = uri.as_str().to_string();
            if !uri_to_claim_hash.contains_key(&uri_str) {
                match scatterbase_kernel::term::find_latest_claim_hash_for_term(&storage_file, &uri_str) {
                    Ok(claim_hash) => {
                        uri_to_claim_hash.insert(uri_str, claim_hash);
                    },
                    Err(e) => {
                        eprintln!("Error looking up claim hash for URI {}: {}", uri_str, e);
                        // Use a placeholder hash when the lookup fails
                        uri_to_claim_hash.insert(uri_str, "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string());
                    }
                }
            }
        }
    }

    // Use the latest claim hash as the priorClaim value
    // If multiple URIs are used, prioritize the domain URI
    if !uri_to_claim_hash.is_empty() {
        // First check if the domain URI is in the map (with or without angle brackets)
        let domain_with_brackets = format!("<{}>", domain);
        if let Some(domain_claim_hash) = uri_to_claim_hash.get(&domain) {
            if domain_claim_hash != "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" {
                prev_claim = domain_claim_hash.clone();
                eprintln!("Using claim hash for domain {}: {}", domain, prev_claim);
            }
        } else if let Some(domain_claim_hash) = uri_to_claim_hash.get(&domain_with_brackets) {
            if domain_claim_hash != "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" {
                prev_claim = domain_claim_hash.clone();
                eprintln!("Using claim hash for domain {}: {}", domain_with_brackets, prev_claim);
            }
        } else {
            // If domain URI is not found, use the first one
            let first_claim_hash = uri_to_claim_hash.values().next().unwrap().clone();
            if first_claim_hash != "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" {
                prev_claim = first_claim_hash;
                eprintln!("Using first claim hash found: {}", prev_claim);
            }
        }

        // Debug: Print all URIs and their claim hashes
        eprintln!("All URIs and their claim hashes:");
        for (uri, claim_hash) in &uri_to_claim_hash {
            eprintln!("  {}: {}", uri, claim_hash);
        }
    }

    // Hash each unique triple
    let mut triple_hashes = Vec::new();
    for triple in &unique_triples {
        triple_hashes.push(hash_triple(triple));
    }

    // Regenerate the claim hash with the updated prev_claim value
    let claim_hash = generate_claim_hash(&triple_hashes, &attribution, &prev_claim, "", "");

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Derive a key for this attribution
    let attribution_key = derive_key_from_server(&server_key, &attribution);

    // Look up if the attribution URI has a scatter:hasPublicKey triple in the database
    let mut has_public_key = false;
    match scatterbase_kernel::term::find_latest_public_key_for_attribution(&storage_file, &attribution) {
        Ok(Some(_)) => {
            // Attribution URI already has a public key
            has_public_key = true;
        },
        Ok(None) => {
            // Attribution URI doesn't have a public key, error and instruct the user to register one
            eprintln!("Error: No public key found for attribution URI {}.", attribution);
            eprintln!("Please register a key for this attribution URI first using:");
            eprintln!("  scatterbase key add --attribution {} [--file <keyfile>]", attribution);
            return;
        },
        Err(e) => {
            eprintln!("Error looking up public key for attribution URI {}: {}", attribution, e);
            return;
        }
    }

    // Create the ClaimEnvelope without a signature first
    let temp_envelope = ClaimEnvelope {
        triples: unique_triples.clone(),
        triple_hashes: triple_hashes.clone(),
        claim_hash: claim_hash.clone(),
        attribution: attribution.clone(),
        prev_claim: prev_claim.clone(),
        signature: String::new(), // Empty signature for now
        domain: domain.clone(),
        server_uri: server_uri.clone(),
        subject_prev: subject_prev.clone(),
        object_prev: object_prev.clone(),
        uri_to_claim_hash: uri_to_claim_hash.clone(),
    };

    // Get the canonical NT representation
    let canonical_nt = temp_envelope.to_ntriples();

    // Sign the canonical NT representation
    let signature = sign_with_key(&attribution_key, &canonical_nt);

    // Create the final ClaimEnvelope with the signature
    let envelope = ClaimEnvelope {
        triples: unique_triples,
        triple_hashes,
        claim_hash,
        attribution,
        prev_claim,
        signature,
        domain,
        server_uri,
        subject_prev,
        object_prev,
        uri_to_claim_hash,
    };

    // Output the ClaimEnvelope in TTL format
    print!("{}", envelope.to_ttl());
}

/// Handles the "claim submit" command
/// This command takes a file with a prepared claim envelope as a parameter or through stdin,
/// completes the process to make a full Signed Claim, and then saves everything to data storage.
pub fn handle_claim_submit_command(args: &[String], env_path: &str) {
    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        println!("Error loading .env file at {}: {}", env_path, e);
        println!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get domain from environment
    let domain = match env::var("DOMAIN") {
        Ok(domain) => domain,
        Err(_) => {
            println!("DOMAIN not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Get server URI from environment
    let server_uri = match env::var("SERVER_URI") {
        Ok(uri) => uri,
        Err(_) => {
            println!("SERVER_URI not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Parse command arguments
    let mut input_file = String::new();
    let mut i = 3; // Start after "claim submit"

    while i < args.len() {
        match args[i].as_str() {
            "--file" | "-f" => {
                if i + 1 < args.len() {
                    input_file = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --file");
                    return;
                }
            },
            "--env" => {
                // Skip the --env option and its value as it's already processed
                i += 2;
            },
            _ => {
                println!("Unknown option: {}", args[i]);
                return;
            }
        }
    }

    // Read the ClaimEnvelope from file or stdin
    let input = if !input_file.is_empty() {
        match fs::read_to_string(&input_file) {
            Ok(content) => content,
            Err(e) => {
                println!("Error reading file: {}", e);
                return;
            }
        }
    } else {
        let mut buffer = String::new();
        match io::stdin().read_to_string(&mut buffer) {
            Ok(_) => buffer,
            Err(e) => {
                println!("Error reading from stdin: {}", e);
                return;
            }
        }
    };

    // Parse the input to extract the ClaimEnvelope
    // For simplicity, we'll re-use the existing functions to parse the triples
    let prefixes = read_prefixes();
    let mut reader = io::Cursor::new(input);
    let triples = match parse_turtle(reader, &prefixes) {
        Ok(triples) => triples,
        Err(e) => {
            println!("Error parsing input: {}", e);
            return;
        }
    };

    // Extract the necessary information from the triples to reconstruct the ClaimEnvelope
    // This is a simplified approach; in a real implementation, you would need to parse the TTL
    // and extract all the necessary information to reconstruct the ClaimEnvelope

    // For now, we'll just use the first triple as an example
    if triples.is_empty() {
        println!("No triples found in input.");
        return;
    }

    // Generate a timestamp
    let now = Utc::now();
    let timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);

    // Generate a sequence ID
    let sequence_id = generate_unique_sequence_id(Some(now));

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("STORAGE_FILE not set in environment. Claim will not be stored.");
            return;
        }
    };

    // Create a storage instance
    let mut storage = create_storage(&storage_file);

    // Initialize the storage with the domain
    if let Err(e) = storage.init(&domain) {
        println!("Error initializing storage with domain: {}", e);
        return;
    }

    // Add the server-side triples (sequence ID, timestamp, server signature)
    // Extract the claim hash and attribution URI from the parsed triples
    let mut claim_hash_uri = None;
    let mut attribution_uri = None;
    for triple in &triples {
        // Look for triples with wasAttributedTo predicate, which should have the claim hash as subject
        if triple.predicate.as_str().to_string() == prov::WAS_ATTRIBUTED_TO {
            claim_hash_uri = Some(triple.subject.clone());
            attribution_uri = Some(triple.object.clone());
            break;
        }
    }

    // Check if the attribution URI has a public key associated with it
    if let Some(attribution_term) = &attribution_uri {
        let attribution_str = if let Term::NamedNode(uri) = attribution_term {
            uri.as_str().to_string()
        } else {
            // If the attribution is not a URI, use a default value
            format!("{}/Server", domain)
        };

        // Look up if the attribution URI has a scatter:hasPublicKey triple in the database
        match scatterbase_kernel::term::find_latest_public_key_for_attribution(&storage_file, &attribution_str) {
            Ok(Some(_)) => {
                // Attribution URI has a public key, proceed with submission
                println!("Found public key for attribution URI {}", attribution_str);
            },
            Ok(None) => {
                // Attribution URI doesn't have a public key, reject the submission
                println!("Error: No public key found for attribution URI {}.", attribution_str);
                println!("Claim submission rejected. Please register a public key for this attribution URI first.");
                return;
            },
            Err(e) => {
                println!("Error looking up public key for attribution URI {}: {}", attribution_str, e);
                println!("Claim submission rejected due to error checking public key.");
                return;
            }
        }
    } else {
        println!("Error: Could not find attribution URI in the claim.");
        println!("Claim submission rejected. Please ensure the claim has a valid attribution URI.");
        return;
    }

    // Extract the claim hash from the URI
    let claim_hash = if let Some(claim_hash_uri) = &claim_hash_uri {
        if let NamedOrBlankNode::NamedNode(uri) = claim_hash_uri {
            let uri_str = uri.as_str().to_string();
            if uri_str.starts_with("https://scatter.red/claim/") {
                uri_str["https://scatter.red/claim/".len()..].to_string()
            } else {
                // Fallback to using the sequence ID if we can't extract the claim hash
                sequence_id.clone()
            }
        } else {
            // Fallback to using the sequence ID if the claim hash URI is not a URI
            sequence_id.clone()
        }
    } else {
        // Fallback to using the sequence ID if we can't find the claim hash URI
        sequence_id.clone()
    };

    // Check if a claim with this hash already exists
    println!("handle_claim_submit_command: Checking if claim with hash '{}' exists", claim_hash);
    match storage.claim_exists(&claim_hash) {
        Ok(exists) => {
            println!("handle_claim_submit_command: Claim exists: {}", exists);
            if exists {
                println!("Error: A claim with the hash '{}' already exists in the database.", claim_hash);
                println!("Claim submission rejected.");
                return;
            }
        },
        Err(e) => {
            println!("Error checking if claim exists: {}", e);
            return;
        }
    }

    // Store all the triples
    for triple in &triples {
        if let Err(e) = storage.append(triple) {
            println!("Error storing triple: {}", e);
            return;
        }
    }

    // Generate the server signature
    let server_signature = if let Some(claim_hash_uri) = &claim_hash_uri {
        // Use the claim hash we already extracted

        // Extract the user signature from the triples
        let mut user_signature = String::new();
        for triple in &triples {
            if triple.predicate.as_str().to_string() == scatter::USER_SIGNATURE {
                if let Term::Literal(literal) = &triple.object {
                    let literal_str = literal.to_string();
                    if literal_str.starts_with("\"ed25519:") && literal_str.ends_with("\"") {
                        // Extract the signature part (remove the quotes and "ed25519:" prefix)
                        user_signature = literal_str[9..literal_str.len()-1].to_string();
                    }
                }
            }
        }

        // Create a temporary ClaimEnvelope
        let temp_envelope = ClaimEnvelope {
            triples: triples.clone(),
            triple_hashes: Vec::new(), // Not needed for this temporary object
            claim_hash: claim_hash,
            attribution: String::new(), // Not needed for this temporary object
            prev_claim: String::new(), // Not needed for this temporary object
            signature: user_signature, // Include the user signature
            domain: domain.clone(),
            server_uri: server_uri.clone(),
            subject_prev: String::new(), // Not needed for this temporary object
            object_prev: String::new(), // Not needed for this temporary object
            uri_to_claim_hash: HashMap::new(), // Not needed for this temporary object
        };

        // Create a temporary SignedClaim
        let temp_signed_claim = SignedClaim {
            envelope: temp_envelope,
            sequence_id: sequence_id.clone(),
            timestamp: timestamp.clone(),
            server_signature: String::new(), // Empty signature for now
        };

        // Get the canonical NT representation
        let canonical_nt = temp_signed_claim.to_ntriples();

        // Sign the canonical NT representation
        sign_with_key(&server_key, &canonical_nt)
    } else {
        // Fallback to signing the sequence ID if we can't find the claim hash URI
        sign_with_key(&server_key, &sequence_id)
    };

    if let Some(claim_hash_uri) = claim_hash_uri {
        // Use URI objects directly
        let seq_id_uri = scatter::SEQ_ID_URI.clone();
        let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
        let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();

        // Create literals for the values
        let seq_id_literal = Literal::new_simple_literal(&sequence_id);
        let timestamp_literal = Literal::new_simple_literal(&timestamp);
        let server_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", server_signature));

        // Create the triples
        let seq_id_triple = Triple::new(claim_hash_uri.clone(), seq_id_uri, seq_id_literal);
        let timestamp_triple = Triple::new(claim_hash_uri.clone(), generated_at_time_uri, timestamp_literal);
        let server_signature_triple = Triple::new(claim_hash_uri.clone(), server_signature_uri, server_signature_literal);

        // Store the triples
        if let Err(e) = storage.append(&seq_id_triple) {
            println!("Error storing sequence ID triple: {}", e);
            return;
        }

        if let Err(e) = storage.append(&timestamp_triple) {
            println!("Error storing timestamp triple: {}", e);
            return;
        }

        if let Err(e) = storage.append(&server_signature_triple) {
            println!("Error storing server signature triple: {}", e);
            return;
        }
    } else {
        println!("Warning: Could not find claim hash in the input. Server-side triples not added.");
    }

    println!("Claim submitted and stored successfully.");
    println!("  Sequence ID: {}", sequence_id);
    println!("  Timestamp: {}", timestamp);
    println!("  Server Signature: {}", server_signature);
}

/// Handles the "claim write" command
/// If a provenance URI is not specified, use domain/Server by default.
pub fn handle_claim_write_command(args: &[String], env_path: &str) {
    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        println!("Error loading .env file at {}: {}", env_path, e);
        println!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get domain from environment
    let domain = match env::var("DOMAIN") {
        Ok(domain) => domain,
        Err(_) => {
            println!("DOMAIN not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Get server URI from environment
    let server_uri = match env::var("SERVER_URI") {
        Ok(uri) => uri,
        Err(_) => {
            println!("SERVER_URI not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Parse command arguments
    let mut attribution = String::new();
    let mut prev_claim = String::new();
    let mut subject_prev = String::new();
    let mut object_prev = String::new();
    let mut input_file = String::new();
    let mut custom_domain = String::new();
    let mut i = 3; // Start after "claim write"

    while i < args.len() {
        match args[i].as_str() {
            "--attribution" | "-a" => {
                if i + 1 < args.len() {
                    attribution = match parse_iri(&args[i + 1]) {
                        Ok(uri) => uri.into_string(),
                        Err(e) => {
                            println!("Invalid value for --attribution: {}", e);
                            return;
                        }
                    };
                    i += 2;
                } else {
                    println!("Missing value for --attribution");
                    return;
                }
            },
            "--prev" | "-p" => {
                if i + 1 < args.len() {
                    prev_claim = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --prev");
                    return;
                }
            },
            "--subject-prev" => {
                if i + 1 < args.len() {
                    subject_prev = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --subject-prev");
                    return;
                }
            },
            "--object-prev" => {
                if i + 1 < args.len() {
                    object_prev = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --object-prev");
                    return;
                }
            },
            "--file" | "-f" => {
                if i + 1 < args.len() {
                    input_file = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --file");
                    return;
                }
            },
            "--domain" | "-d" => {
                if i + 1 < args.len() {
                    custom_domain = args[i + 1].clone();
                    i += 2;
                } else {
                    println!("Missing value for --domain");
                    return;
                }
            },
            "--env" => {
                // Skip the --env option and its value as it's already processed
                i += 2;
            },
            _ => {
                println!("Unknown option: {}", args[i]);
                return;
            }
        }
    }

    // If a custom domain was provided, use that instead of the environment variable
    let domain = if !custom_domain.is_empty() {
        custom_domain
    } else {
        domain
    };

    // If no attribution is provided, use domain/Server by default
    if attribution.is_empty() {
        attribution = format!("{}/Server", domain);
    }

    // If no previous claim hash is provided, use all zeros in Base32z format
    if prev_claim.is_empty() {
        // A SHA-256 hash is 32 bytes (256 bits)
        // In Base32z encoding, each 5 bits are represented by 1 character
        // So 32 bytes (256 bits) would be represented by ceil(256/5) = 52 characters
        // 'y' is the first character in the Base32z alphabet, representing 0
        prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    }

    // If no subject previous claim hash is provided, use all y's in Base32z format
    if subject_prev.is_empty() {
        subject_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    }

    // If no object previous claim hash is provided, use all y's in Base32z format
    if object_prev.is_empty() {
        object_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    }

    // Read prefixes
    let prefixes = read_prefixes();

    // Read triples from file or stdin
    let triples = if !input_file.is_empty() {
        match read_triples_from_file(&input_file, &prefixes) {
            Ok(triples) => triples,
            Err(e) => {
                println!("Error reading triples from file: {}", e);
                return;
            }
        }
    } else {
        match read_triples_from_stdin(&prefixes) {
            Ok(triples) => triples,
            Err(e) => {
                println!("Error reading triples from stdin: {}", e);
                return;
            }
        }
    };

    if triples.is_empty() {
        println!("No triples found in input.");
        return;
    }

    // Generate a timestamp
    let now = Utc::now();
    let timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);

    // Generate a sequence ID in format YYYYMMDDHHmmsssss00
    // The last two digits are 00 by default, but if two submissions come in at the same millisecond,
    // set the second one to be processed as 01, and so on.
    // Use the same timestamp as the one used for the timestamp triple
    let sequence_id = generate_unique_sequence_id(Some(now));

    // Hash each triple
    let mut triple_hashes = Vec::new();
    for triple in &triples {
        triple_hashes.push(hash_triple(triple));
    }

    // Generate the claim hash
    let claim_hash = generate_claim_hash(&triple_hashes, &attribution, &prev_claim, &timestamp, &sequence_id);

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Derive a key for this attribution
    let attribution_key = derive_key_from_server(&server_key, &attribution);

    // Sign the claim hash
    let signature = sign_with_key(&attribution_key, &claim_hash);

    // Output the claim
    println!("Claim generated:");
    println!("  Domain: {}", domain);
    println!("  Server: {}", server_uri);
    println!("  Attribution: {}", attribution);
    println!("  Timestamp: {}", timestamp);
    println!("  Sequence: {}", sequence_id);
    println!("  Previous: {}", prev_claim);
    println!("  Signature: {}", signature);
    println!("  Hash: {}", claim_hash);
    println!("  Triples: {}", triples.len());

    // Output each triple
    for triple in &triples {
        println!("    {}", triple.to_ntriples());
    }

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            println!("STORAGE_FILE not set in environment. Claim will not be stored.");
            return;
        }
    };

    // Create a storage instance
    let mut storage = create_storage(&storage_file);

    // Initialize the storage with the domain
    if let Err(e) = storage.init(&domain) {
        println!("Error initializing storage with domain: {}", e);
        return;
    }

    // Use URI objects directly
    let claimed_in_uri = scatter::CLAIMED_IN_URI.clone();
    let rdf_type_uri = rdf::TYPE_URI.clone();
    let triple_type_uri = scatter::TRIPLE_URI.clone();
    let signed_claim_type_uri = scatter::SIGNED_CLAIM_URI.clone();
    let includes_uri = scatter::INCLUDES_URI.clone();
    let was_attributed_to_uri = prov::WAS_ATTRIBUTED_TO_URI.clone();
    let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
    let seq_id_uri = scatter::SEQ_ID_URI.clone();
    let previous_claim_uri = scatter::PREVIOUS_CLAIM_URI.clone();
    let user_signature_uri = scatter::USER_SIGNATURE_URI.clone();
    let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();

    // Create URI for the claim hash with the specified format
    let claim_hash_uri = scatter::claim_hash_uri(&claim_hash);

    // Create URIs for the triple hashes with the specified format
    let mut triple_hash_uris = Vec::new();
    for hash in &triple_hashes {
        let hash_uri = scatter::triple_hash_uri(hash);
        triple_hash_uris.push(hash_uri);
    }

    // Create URI for the previous claim hash with the specified format
    let prev_claim_uri = scatter::claim_hash_uri(&prev_claim);

    // Create URI for the attribution
    let attribution_uri = match NamedNode::new(&attribution) {
        Ok(uri) => uri,
        Err(e) => {
            println!("Error creating attribution URI: {}", e);
            return;
        }
    };

    // Create literal values for timestamp and sequence ID
    let datatype_datetime = rdf::XSD_DATETIME_URI.clone();
    let timestamp_literal = Literal::new_typed_literal(&timestamp, datatype_datetime);

    let datatype_unsignedlong = rdf::XSD_UNSIGNED_LONG_URI.clone();
    let sequence_id_literal = Literal::new_typed_literal(&sequence_id, datatype_unsignedlong);

    // Create literal values for signatures
    let user_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", signature));
    let server_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", signature)); // Using the same signature for now

    // Store the original triples
    for triple in &triples {
        if let Err(e) = storage.append(triple) {
            println!("Error storing triple: {}", e);
            return;
        }
    }

    // Store the triples according to the required format
    for (i, triple_hash_uri) in triple_hash_uris.iter().enumerate() {
        // <tripleHash> <https://scatter.red/terms/v0/claimedIn> <ClaimHash> .
        let claimed_in_triple = Triple::new(triple_hash_uri.clone(), claimed_in_uri.clone(), claim_hash_uri.clone());
        if let Err(e) = storage.append(&claimed_in_triple) {
            println!("Error storing claimedIn triple: {}", e);
            return;
        }

        // <tripleHash> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/Triple> .
        let triple_type_triple = Triple::new(triple_hash_uri.clone(), rdf_type_uri.clone(), triple_type_uri.clone());
        if let Err(e) = storage.append(&triple_type_triple) {
            println!("Error storing triple type triple: {}", e);
            return;
        }
    }

    // <SignedClaimHash> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/SignedClaim> .
    let signed_claim_type_triple = Triple::new(claim_hash_uri.clone(), rdf_type_uri.clone(), signed_claim_type_uri);
    if let Err(e) = storage.append(&signed_claim_type_triple) {
        println!("Error storing signed claim type triple: {}", e);
        return;
    }

    // <SignedClaimHash> <https://scatter.red/terms/v0/includes> <tripleHash> .
    for triple_hash_uri in &triple_hash_uris {
        let includes_triple = Triple::new(claim_hash_uri.clone(), includes_uri.clone(), triple_hash_uri.clone());
        if let Err(e) = storage.append(&includes_triple) {
            println!("Error storing includes triple: {}", e);
            return;
        }
    }

    // <SignedClaimHash> <http://www.w3.org/ns/prov#wasAttributedTo> <User> .
    let was_attributed_to_triple = Triple::new(claim_hash_uri.clone(), was_attributed_to_uri, attribution_uri);
    if let Err(e) = storage.append(&was_attributed_to_triple) {
        println!("Error storing wasAttributedTo triple: {}", e);
        return;
    }

    // <SignedClaimHash> <http://www.w3.org/ns/prov#generatedAtTime> "Timestamp"^^<http://www.w3.org/2001/XMLSchema#dateTime> .
    let generated_at_time_triple = Triple::new(claim_hash_uri.clone(), generated_at_time_uri, timestamp_literal.clone());
    if let Err(e) = storage.append(&generated_at_time_triple) {
        println!("Error storing generatedAtTime triple: {}", e);
        return;
    }

    // <SignedClaimHash> <https://scatter.red/terms/v0/seqID> "SequenceID"^^<http://www.w3.org/2001/XMLSchema#unsignedLong> .
    let seq_id_triple = Triple::new(claim_hash_uri.clone(), seq_id_uri, sequence_id_literal.clone());
    if let Err(e) = storage.append(&seq_id_triple) {
        println!("Error storing seqID triple: {}", e);
        return;
    }

    // Create "ack" triples for each subject and object in the original triples
    let term_uri = scatter::TERM_URI.clone();
    let prior_claim_uri = scatter::PRIOR_CLAIM_URI.clone();
    let acknowledges_prior_use_uri = scatter::ACKNOWLEDGES_PRIOR_USE_URI.clone();

    // Process each original triple to create acknowledgment triples
    for (i, triple) in triples.iter().enumerate() {
        // Hash the subject term's canonical string representation
        let subject_term_hash = hash_string(&triple.subject.to_string());

        // Create acknowledgment for subject
        let subject_ack_uri = scatter::ack_uri(&claim_hash, &subject_term_hash);

        // Ensure domain ends with trailing slash if it's a standalone domain
        let subject_str = triple.subject.to_string();
        // Remove angle brackets if it's a URI
        let subject_str = if subject_str.starts_with('<') && subject_str.ends_with('>') {
            subject_str[1..subject_str.len()-1].to_string()
        } else {
            subject_str
        };

        let domain_part = if subject_str.contains('/') {
            subject_str.split('/').next().unwrap_or(&subject_str)
        } else {
            &subject_str
        };

        let subject_last_part = if domain_part == subject_str && !subject_str.ends_with('/') {
            format!("{}/", subject_str)
        } else {
            subject_str.clone()
        }.split('/').last().unwrap_or("Subject").to_string();

        let subject_prior_claim_uri = match NamedNode::new(&format!("https://scatter.red/claim/{}", subject_prev)) {
            Ok(uri) => uri,
            Err(e) => {
                println!("Error creating subject prior claim URI: {}", e);
                return;
            }
        };

        // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/term> <Subject> .
        let subject_term_triple = Triple::new(subject_ack_uri.clone(), term_uri.clone(), triple.subject.clone());
        if let Err(e) = storage.append(&subject_term_triple) {
            println!("Error storing subject term triple: {}", e);
            return;
        }

        // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/PriorClaimSubject> .
        let subject_prior_claim_triple = Triple::new(subject_ack_uri.clone(), prior_claim_uri.clone(), subject_prior_claim_uri);
        if let Err(e) = storage.append(&subject_prior_claim_triple) {
            println!("Error storing subject prior claim triple: {}", e);
            return;
        }

        // <https://scatter.red/claim/ClaimHash> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/ClaimHash/term/TermHash> .
        let subject_acknowledges_triple = Triple::new(claim_hash_uri.clone(), acknowledges_prior_use_uri.clone(), subject_ack_uri);
        if let Err(e) = storage.append(&subject_acknowledges_triple) {
            println!("Error storing subject acknowledges triple: {}", e);
            return;
        }

        // Create acknowledgment for object if it's a URI
        if let Some(object_uri) = triple.object.as_named_node() {
            // Hash the object term's canonical string representation
            let object_term_hash = hash_string(&object_uri.as_str().to_string());

            let object_ack_uri = match NamedNode::new(&format!("https://scatter.red/ack/{}/term/{}", claim_hash, object_term_hash)) {
                Ok(uri) => uri,
                Err(e) => {
                    println!("Error creating object acknowledgment URI: {}", e);
                    return;
                }
            };

            // Ensure domain ends with trailing slash if it's a standalone domain
            let object_str = object_uri.as_str();
            let domain_part = if object_str.contains('/') {
                object_str.split('/').next().unwrap_or(object_str)
            } else {
                object_str
            };

            let object_last_part = if domain_part == object_str && !object_str.ends_with('/') {
                format!("{}/", object_str)
            } else {
                object_str.to_string()
            }.split('/').last().unwrap_or("Object").to_string();

            let object_prior_claim_uri = match NamedNode::new(&format!("https://scatter.red/claim/{}", object_prev)) {
                Ok(uri) => uri,
                Err(e) => {
                    println!("Error creating object prior claim URI: {}", e);
                    return;
                }
            };

            // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/term> <Object> .
            let object_term_triple = Triple::new(object_ack_uri.clone(), term_uri.clone(), object_uri.clone());
            if let Err(e) = storage.append(&object_term_triple) {
                println!("Error storing object term triple: {}", e);
                return;
            }

            // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/PriorClaimObject> .
            let object_prior_claim_triple = Triple::new(object_ack_uri.clone(), prior_claim_uri.clone(), object_prior_claim_uri);
            if let Err(e) = storage.append(&object_prior_claim_triple) {
                println!("Error storing object prior claim triple: {}", e);
                return;
            }

            // <https://scatter.red/claim/ClaimHash> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/ClaimHash/term/TermHash> .
            let object_acknowledges_triple = Triple::new(claim_hash_uri.clone(), acknowledges_prior_use_uri.clone(), object_ack_uri);
            if let Err(e) = storage.append(&object_acknowledges_triple) {
                println!("Error storing object acknowledges triple: {}", e);
                return;
            }
        }
    }

    // <SignedClaimHash> <https://scatter.red/terms/v0/userSignature> "ed25519:UserSignatureB32z==" .
    let user_signature_triple = Triple::new(claim_hash_uri.clone(), user_signature_uri, user_signature_literal.clone());
    if let Err(e) = storage.append(&user_signature_triple) {
        println!("Error storing userSignature triple: {}", e);
        return;
    }

    // <SignedClaimHash> <https://scatter.red/terms/v0/serverSignature> "ed25519:ServerSignatureB32z==" .
    let server_signature_triple = Triple::new(claim_hash_uri.clone(), server_signature_uri, server_signature_literal.clone());
    if let Err(e) = storage.append(&server_signature_triple) {
        println!("Error storing serverSignature triple: {}", e);
        return;
    }

    println!("Claim committed to data storage successfully.");
}
