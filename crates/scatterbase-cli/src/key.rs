use std::fs;
use std::io::{self};
use ed25519_dalek::SigningKey;
use oxrdf::NamedNode;
use scatterbase_kernel::uri::rdf;
use scatterbase_kernel::uri::scatter;
use oxrdf::Triple;
use scatterbase_kernel::triple::TripleExt;
use scatterbase_kernel::crypto::{get_server_signing_key, derive_key_from_server, BASE32Z};
use scatterbase_kernel::claim::{ClaimEnvelope, hash_triple, generate_claim_hash};
use std::collections::HashMap;

/// Handles the "key add" command
/// This command registers a key for an attribution URI
/// 
/// # Arguments
/// 
/// * `args` - The command line arguments
/// * `env_path` - The path to the .env file
pub fn handle_key_add_command(args: &[String], env_path: &str) -> io::Result<()> {
    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error loading .env file at {}: {}", env_path, e)
        ));
    }

    // Get domain from environment
    let domain = match std::env::var("DOMAIN") {
        Ok(domain) => domain,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "DOMAIN not set in environment. Run 'scatterbase init' first."
            ));
        }
    };

    // Get server URI from environment
    let server_uri = match std::env::var("SERVER_URI") {
        Ok(uri) => uri,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "SERVER_URI not set in environment. Run 'scatterbase init' first."
            ));
        }
    };

    // Get storage file from environment
    let storage_file = match std::env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "STORAGE_FILE not set in environment. Run 'scatterbase init' first."
            ));
        }
    };

    // Parse command arguments
    let mut attribution = String::new();
    let mut key_file = String::new();
    let mut i = 3; // Start after "key add"

    while i < args.len() {
        match args[i].as_str() {
            "--attribution" | "-a" => {
                if i + 1 < args.len() {
                    attribution = scatterbase_kernel::uri::parse_iri(&args[i + 1])
                        .map_err(|e| io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("Invalid value for --attribution: {}", e)
                        ))?
                        .into_string();
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --attribution"
                    ));
                }
            },
            "--file" | "-f" => {
                if i + 1 < args.len() {
                    key_file = args[i + 1].clone();
                    i += 2;
                } else {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Missing value for --file"
                    ));
                }
            },
            "--env" => {
                // Skip the --env option and its value as it's already processed
                i += 2;
            },
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unknown option: {}", args[i])
                ));
            }
        }
    }

    // Check if attribution is provided
    if attribution.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Attribution URI is required. Use --attribution to specify."
        ));
    }

    // Check if the attribution URI already has a key
    match scatterbase_kernel::term::find_latest_public_key_for_attribution(&storage_file, &attribution) {
        Ok(Some(_)) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("A key is already registered for attribution URI {}. To update the key, submit a key update via claim.", attribution)
            ));
        },
        Ok(None) => {
            // No key registered, proceed with registration
        },
        Err(e) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Error looking up public key for attribution URI {}: {}", attribution, e)
            ));
        }
    }

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Determine the key to use
    let verifying_key = if !key_file.is_empty() {
        // Use the provided key file
        println!("Using key from file: {}", key_file);
        match fs::read_to_string(&key_file) {
            Ok(key_data) => {
                // Parse the key
                match BASE32Z.decode(key_data.trim().as_bytes()) {
                    Ok(key_bytes) => {
                        if key_bytes.len() == 32 {
                            // Create a signing key from the bytes
                            let signing_key = SigningKey::from_bytes(&key_bytes.try_into().unwrap());
                            signing_key.verifying_key()
                        } else {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("Invalid key length: {}. Expected 32 bytes.", key_bytes.len())
                            ));
                        }
                    },
                    Err(e) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("Error decoding key: {:?}", e)
                        ));
                    }
                }
            },
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("Error reading key file: {}", e)
                ));
            }
        }
    } else {
        // Derive a key from the server key and attribution URI
        println!("Deriving key from server key using attribution URI: {}", attribution);
        let attribution_key = derive_key_from_server(&server_key, &attribution);
        attribution_key.verifying_key()
    };

    // Create the public key URI
    let pubkey_uri = format!("urn:ed25519:{}", BASE32Z.encode(&verifying_key.to_bytes()));

    // Create the attribution URI object
    let attribution_uri_obj = NamedNode::new(&attribution).unwrap();
    let pubkey_uri_obj = NamedNode::new(&pubkey_uri).unwrap();

    // <attribution> scatter:hasPublicKey <urn:ed25519> .
    let has_pubkey_triple = Triple::new(
        attribution_uri_obj.clone(),
        scatter::HAS_PUBLIC_KEY_URI.clone(),
        pubkey_uri_obj.clone()
    );

    // <urn:ed25519> a scatter:PublicKey .
    let pubkey_type_triple = Triple::new(
        pubkey_uri_obj.clone(),
        rdf::TYPE_URI.clone(),
        scatter::PUBLIC_KEY_URI.clone()
    );

    // <urn:ed25519> scatter:keyType scatter:ed25519 .
    let key_type_triple = Triple::new(
        pubkey_uri_obj.clone(),
        scatter::KEY_TYPE_URI.clone(),
        scatter::ED25519_URI.clone()
    );

    // <urn:ed25519> scatter:Owner <attribution> .
    let owner_triple = Triple::new(
        pubkey_uri_obj.clone(),
        scatter::OWNER_URI.clone(),
        attribution_uri_obj.clone()
    );

    // Collect all triples for the claim
    let triples = vec![
        has_pubkey_triple,
        pubkey_type_triple,
        key_type_triple,
        owner_triple,
    ];

    // Hash each triple
    let mut triple_hashes = Vec::new();
    for triple in &triples {
        triple_hashes.push(hash_triple(triple));
    }

    // For the initial claim, there is no previous claim, so use all zeros in Base32z format
    let prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    let subject_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    let object_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();

    // Generate the claim hash
    let claim_hash = generate_claim_hash(&triple_hashes, &server_uri, &prev_claim, "", "");

    // Create the ClaimEnvelope without a signature first
    let temp_envelope = ClaimEnvelope {
        triples: triples.clone(),
        triple_hashes: triple_hashes.clone(),
        claim_hash: claim_hash.clone(),
        attribution: server_uri.clone(),
        prev_claim: prev_claim.clone(),
        signature: String::new(), // Empty signature for now
        domain: domain.clone(),
        server_uri: server_uri.clone(),
        subject_prev: subject_prev.clone(),
        object_prev: object_prev.clone(),
        uri_to_claim_hash: HashMap::new(),
    };

    // Get the canonical NT representation
    let canonical_nt = temp_envelope.to_ntriples();

    // Sign the canonical NT representation with the server key
    let signature = scatterbase_kernel::crypto::sign_with_key(&server_key, &canonical_nt);

    // Create the final ClaimEnvelope with the signature
    let envelope = ClaimEnvelope {
        triples,
        triple_hashes,
        claim_hash: claim_hash.clone(),
        attribution: server_uri.clone(),
        prev_claim,
        signature,
        domain,
        server_uri,
        subject_prev,
        object_prev,
        uri_to_claim_hash: HashMap::new(),
    };

    // Submit the claim
    scatterbase_kernel::claim::handle_claim_submit_envelope(envelope, &storage_file)?;

    println!("Key registered for attribution URI: {}", attribution);
    println!("Public key: {}", pubkey_uri);

    Ok(())
}

/// Prints help information for the key command
pub fn print_key_help() {
    crate::cli::print_help_info(
        "key",
        "scatterbase key <command> [options]",
        Some(vec![
            ("add", "Register a key for an attribution URI"),
        ]),
        Some(vec![
            "The key command provides operations for managing cryptographic keys.",
            "Keys are used to sign claims and verify their authenticity."
        ]),
        Some(vec![
            "scatterbase key add --attribution <uri> [--file <keyfile>]",
            "scatterbase key add --attribution https://example.org/alice",
            "scatterbase key add --attribution https://example.org/bob --file bob_key.private"
        ]),
        true
    );
}