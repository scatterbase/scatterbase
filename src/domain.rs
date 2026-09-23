use std::io::{self, Write, Cursor};
use std::fs::{self, File, Permissions};
use std::env;
use std::collections::HashMap;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::triple::Triple;
use crate::uri::Uri;
use crate::uri::rdf;
use crate::uri::prov;
use crate::uri::scatter;
use crate::uri::vocab;
use crate::term::Term;
use crate::utils::{
    ensure_directory_exists
};
use crate::cli::{
    prompt_for_domain, prompt_for_storage_file, prompt_for_env_file, 
    prompt_for_server_uri, prompt_for_key_file
};
use crate::constants::{DEFAULT_PUBLIC_KEY_FILE, DEFAULT_PREFIX_FILE, DEFAULT_PRIVATE_KEY_FILE};
use crate::crypto::{generate_ed25519_keypair, BASE32Z, get_server_signing_key, derive_key_from_server, sign_with_key, hash_string};
use crate::storage::{Storage, create_storage};
use crate::claim::{hash_triple, generate_claim_hash, ClaimEnvelope, parse_turtle};
use crate::prefix::read_prefixes;
use crate::literal::Literal;
use chrono::{Utc, SecondsFormat, DateTime};

/// Creates an .env file with the configuration
/// 
/// If `use_defaults` is true, all default values will be used without prompting.
pub fn create_env_file(env_file: &str, domain: &str, storage_file: &str, server_uri: &str, use_defaults: bool) -> io::Result<()> {
    // Generate ED25519 key pair
    let (signing_key, verifying_key) = generate_ed25519_keypair();

    // Convert keys to Base32z for storage (both private and public keys)
    let secret_key_b32z = BASE32Z.encode(&signing_key.to_bytes());
    let public_key_b32z = BASE32Z.encode(&verifying_key.to_bytes());

    // Get private key file (prompt or use default)
    let private_key_file = if use_defaults {
        let private_key_file = DEFAULT_PRIVATE_KEY_FILE.to_string();
        println!("Using private key file: {}", private_key_file);
        private_key_file
    } else {
        let private_key_file = prompt_for_key_file();
        println!("Using private key file: {}", private_key_file);
        private_key_file
    };

    // Ensure directory exists
    ensure_directory_exists(&private_key_file)?;

    // Write secret key to separate file
    let mut private_key_file_handle = File::create(&private_key_file)?;
    private_key_file_handle.write_all(secret_key_b32z.as_bytes())?;

    // Set file permissions to 600 (read/write for owner only) if on Unix
    #[cfg(unix)]
    {
        let permissions = Permissions::from_mode(0o600);
        fs::set_permissions(&private_key_file, permissions)?;
        println!("Set permissions of {} to 600 (read/write for owner only)", private_key_file);
    }

    // Write public key to separate file
    let public_key_file = DEFAULT_PUBLIC_KEY_FILE;
    ensure_directory_exists(&public_key_file)?;
    let mut public_key_file_handle = File::create(&public_key_file)?;
    public_key_file_handle.write_all(public_key_b32z.as_bytes())?;
    println!("Public key stored in {}", public_key_file);

    // Create content with domain, storage file, server URI, key file paths, and prefix file
    let content = format!(
        "DOMAIN={}\nSTORAGE_FILE={}\nSERVER_URI={}\nED25519_PUBLIC_KEY_FILE={}\nED25519_SECRET_KEY_FILE={}\nPREFIX_FILE={}\n",
        domain, storage_file, server_uri, public_key_file, private_key_file, DEFAULT_PREFIX_FILE
    );

    let mut file = File::create(env_file)?;
    file.write_all(content.as_bytes())?;

    println!("Generated ED25519 key pair for signing claims");
    println!("Secret key stored in {}", private_key_file);

    Ok(())
}

/// Creates the initial triple for the dataset
pub fn create_initial_triple(domain: &str) -> Triple {
    let subject = Uri::new(domain).unwrap();
    let property = rdf::TYPE_URI.clone();
    let object = Uri::new(&format!("{}Dataset", vocab::SCHEMA)).unwrap();

    Triple::new_uri_object(subject, property, object)
}

/// Creates a signed claim with just the server's public key
/// 
/// # Arguments
/// 
/// * `domain` - The domain for the dataset
/// * `storage` - The storage instance to append the claim to
/// 
/// # Returns
/// 
/// * `io::Result<String>` - Result of the operation with the claim hash
fn create_server_key_claim(domain: &str, storage: &mut impl Storage) -> io::Result<String> {
    // Create the attribution URI <domain/Server>
    let attribution = format!("{}/Server", domain);

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Create the server URI
    let server_uri = format!("{}/Server", domain);

    // Create the public key URI
    let pubkey_uri = format!("urn:ed25519:{}", BASE32Z.encode(&server_key.verifying_key().to_bytes()));

    // Create triples for the server's public key
    let server_uri_obj = Uri::new(&server_uri).unwrap();
    let pubkey_uri_obj = Uri::new(&pubkey_uri).unwrap();

    // <domain/Server> scatter:hasPublicKey <urn:ed25519> .
    let has_pubkey_triple = Triple::new_uri_object(
        server_uri_obj.clone(),
        scatter::HAS_PUBLIC_KEY_URI.clone(),
        pubkey_uri_obj.clone()
    );

    // <urn:ed25519> a scatter:publicKey .
    let pubkey_type_triple = Triple::new_uri_object(
        pubkey_uri_obj.clone(),
        rdf::TYPE_URI.clone(),
        scatter::PUBLIC_KEY_URI.clone()
    );

    // <urn:ed25519> scatter:keyType scatter:ed25519 .
    let key_type_triple = Triple::new_uri_object(
        pubkey_uri_obj.clone(),
        scatter::KEY_TYPE_URI.clone(),
        scatter::ED25519_URI.clone()
    );

    // Collect all triples for the first claim (only public key related triples)
    let triples = vec![
        has_pubkey_triple.clone(),
        pubkey_type_triple.clone(),
        key_type_triple.clone(),
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
    let claim_hash = generate_claim_hash(&triple_hashes, &attribution, &prev_claim, "", "");

    // Derive a key for this attribution
    let attribution_key = derive_key_from_server(&server_key, &attribution);

    // Create the ClaimEnvelope without a signature first
    let temp_envelope = ClaimEnvelope {
        triples: triples.clone(),
        triple_hashes: triple_hashes.clone(),
        claim_hash: claim_hash.clone(),
        attribution: attribution.clone(),
        prev_claim: prev_claim.clone(),
        signature: String::new(), // Empty signature for now
        domain: domain.to_string(),
        server_uri: server_uri.clone(),
        subject_prev: subject_prev.clone(),
        object_prev: object_prev.clone(),
        uri_to_claim_hash: HashMap::new(),
    };

    // Get the canonical NT representation
    let canonical_nt = temp_envelope.to_ntriples();

    // Sign the canonical NT representation
    let signature = sign_with_key(&attribution_key, &canonical_nt);

    // Create the final ClaimEnvelope with the signature
    let envelope = ClaimEnvelope {
        triples,
        triple_hashes,
        claim_hash: claim_hash.clone(),
        attribution,
        prev_claim,
        signature: signature.clone(),
        domain: domain.to_string(),
        server_uri: server_uri.clone(),
        subject_prev,
        object_prev,
        uri_to_claim_hash: HashMap::new(),
    };

    // Process and store the claim
    process_and_store_claim(envelope, &server_key, storage)?;

    // Return the claim hash for use in the second claim
    Ok(claim_hash)
}

/// Creates a signed claim with the initial triple as the payload
/// 
/// # Arguments
/// 
/// * `domain` - The domain for the dataset
/// * `storage` - The storage instance to append the claim to
/// 
/// # Returns
/// 
/// * `io::Result<()>` - Result of the operation
pub fn create_initial_claim(domain: &str, storage: &mut impl Storage) -> io::Result<()> {
    // First, create and submit the server key claim
    let first_claim_hash = create_server_key_claim(domain, storage)?;

    // Create the initial triple
    let initial_triple = create_initial_triple(domain);

    // Create the attribution URI <domain/Server>
    let attribution = format!("{}/Server", domain);

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Create the server URI
    let server_uri = format!("{}/Server", domain);

    // Create a triple declaring the domain as a scatter:Domain
    let domain_uri = Uri::new(domain).unwrap();
    let domain_type_triple = Triple::new_uri_object(
        domain_uri.clone(),
        rdf::TYPE_URI.clone(),
        Uri::new(&format!("{}Domain", scatter::TERMS_BASE)).unwrap()
    );

    // Collect all triples for the second claim (everything except public key triples)
    let triples = vec![
        initial_triple.clone(),
        domain_type_triple.clone()
    ];

    // Hash each triple
    let mut triple_hashes = Vec::new();
    for triple in &triples {
        triple_hashes.push(hash_triple(triple));
    }

    // Use the first claim hash as the previous claim
    let prev_claim = first_claim_hash;
    let subject_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();
    let object_prev = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string();

    // Generate the claim hash
    let claim_hash = generate_claim_hash(&triple_hashes, &attribution, &prev_claim, "", "");

    // Derive a key for this attribution
    let attribution_key = derive_key_from_server(&server_key, &attribution);

    // Create the ClaimEnvelope without a signature first
    let temp_envelope = ClaimEnvelope {
        triples: triples.clone(),
        triple_hashes: triple_hashes.clone(),
        claim_hash: claim_hash.clone(),
        attribution: attribution.clone(),
        prev_claim: prev_claim.clone(),
        signature: String::new(), // Empty signature for now
        domain: domain.to_string(),
        server_uri: server_uri.clone(),
        subject_prev: subject_prev.clone(),
        object_prev: object_prev.clone(),
        uri_to_claim_hash: HashMap::new(),
    };

    // Get the canonical NT representation
    let canonical_nt = temp_envelope.to_ntriples();

    // Sign the canonical NT representation
    let signature = sign_with_key(&attribution_key, &canonical_nt);

    // Create the final ClaimEnvelope with the signature
    let envelope = ClaimEnvelope {
        triples,
        triple_hashes,
        claim_hash,
        attribution,
        prev_claim,
        signature: signature.clone(),
        domain: domain.to_string(),
        server_uri,
        subject_prev,
        object_prev,
        uri_to_claim_hash: HashMap::new(),
    };

    // Process and store the claim
    process_and_store_claim(envelope, &server_key, storage)?;

    Ok(())
}

/// Helper function to process and store a claim
/// 
/// # Arguments
/// 
/// * `envelope` - The claim envelope to process
/// * `server_key` - The server's signing key
/// * `storage` - The storage instance to append the claim to
/// 
/// # Returns
/// 
/// * `io::Result<()>` - Result of the operation
fn process_and_store_claim(envelope: ClaimEnvelope, server_key: &ed25519_dalek::SigningKey, storage: &mut impl Storage) -> io::Result<()> {
    // Convert the envelope to TTL format
    let envelope_ttl = envelope.to_ttl();

    // Create a cursor to read the TTL data
    let mut reader = Cursor::new(envelope_ttl);

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            // If STORAGE_FILE is not set, use a default value or return an error
            return Err(io::Error::new(io::ErrorKind::NotFound, "STORAGE_FILE not set in environment"));
        }
    };

    // Create a storage instance
    let mut new_storage = create_storage(&storage_file);

    // Initialize the storage with the domain
    if let Err(e) = new_storage.init(&envelope.domain) {
        return Err(io::Error::new(io::ErrorKind::Other, format!("Error initializing storage: {}", e)));
    }

    // Parse the TTL data to get the triples
    let prefixes = read_prefixes();
    let triples = match parse_turtle(reader, &prefixes) {
        Ok(triples) => triples,
        Err(e) => {
            return Err(io::Error::new(io::ErrorKind::Other, format!("Error parsing TTL: {}", e)));
        }
    };

    // Generate a timestamp
    let now = Utc::now();
    let timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);

    // Generate a sequence ID
    let sequence_id = format!("{}", now.format("%Y%m%d%H%M%S%3f00"));

    // Store all the triples
    for triple in &triples {
        if let Err(e) = storage.append(triple) {
            return Err(io::Error::new(io::ErrorKind::Other, format!("Error storing triple: {}", e)));
        }
    }

    // Extract the claim hash URI from the triples
    let mut claim_hash_uri = None;
    for triple in &triples {
        // Look for triples with wasAttributedTo predicate, which should have the claim hash as subject
        if triple.property().to_string() == prov::WAS_ATTRIBUTED_TO {
            claim_hash_uri = Some(triple.subject().clone());
            break;
        }
    }

    if let Some(claim_hash_uri) = claim_hash_uri {
        // Create server-side triples (sequence ID, timestamp, server signature)
        let seq_id_uri = scatter::SEQ_ID_URI.clone();
        let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
        let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();

        // Create a string representation of the data to sign
        // This should include the claim envelope (with the user signature), the seqID, and the timestamp
        let data_to_sign = format!("{}\n{}\n{}", 
            triples.iter().map(|t| t.to_ntriples()).collect::<Vec<_>>().join("\n"),
            sequence_id,
            timestamp);

        // Sign the data with the server key
        let server_signature = sign_with_key(server_key, &data_to_sign);

        // Create literals for the values
        let seq_id_literal = crate::literal::Literal::new(&sequence_id);
        let timestamp_literal = crate::literal::Literal::new(&timestamp);
        let server_signature_literal = crate::literal::Literal::new(&format!("ed25519:{}", server_signature));

        // Create the triples
        let seq_id_triple = if let Term::Uri(uri) = &claim_hash_uri {
            Triple::new_literal_object(uri.clone(), seq_id_uri, seq_id_literal)
        } else {
            Triple::new(claim_hash_uri.clone(), seq_id_uri, Term::Literal(seq_id_literal))
        };
        let timestamp_triple = if let Term::Uri(uri) = &claim_hash_uri {
            Triple::new_literal_object(uri.clone(), generated_at_time_uri, timestamp_literal)
        } else {
            Triple::new(claim_hash_uri.clone(), generated_at_time_uri, Term::Literal(timestamp_literal))
        };
        let server_signature_triple = if let Term::Uri(uri) = &claim_hash_uri {
            Triple::new_literal_object(uri.clone(), server_signature_uri, server_signature_literal)
        } else {
            Triple::new(claim_hash_uri.clone(), server_signature_uri, Term::Literal(server_signature_literal))
        };

        // Store the triples
        if let Err(e) = storage.append(&seq_id_triple) {
            return Err(io::Error::new(io::ErrorKind::Other, format!("Error storing sequence ID triple: {}", e)));
        }

        if let Err(e) = storage.append(&timestamp_triple) {
            return Err(io::Error::new(io::ErrorKind::Other, format!("Error storing timestamp triple: {}", e)));
        }

        if let Err(e) = storage.append(&server_signature_triple) {
            return Err(io::Error::new(io::ErrorKind::Other, format!("Error storing server signature triple: {}", e)));
        }
    } else {
        return Err(io::Error::new(io::ErrorKind::Other, "Could not find claim hash in the envelope"));
    }

    Ok(())
}

/// Handles the "domain init" command
pub fn handle_domain_init_command(env_path: &str, uri_param: Option<&str>) {
    println!("Initializing a new domain...");

    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        println!("Error loading .env file at {}: {}", env_path, e);
        println!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get domain - either from parameter, prompt, or environment
    let domain = if let Some(uri) = uri_param {
        uri.to_string()
    } else {
        // Prompt user for domain if not provided as parameter
        prompt_for_domain()
    };

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => Some(file),
        Err(_) => {
            println!("STORAGE_FILE not set in environment. Output will be sent to stdout.");
            None
        }
    };

    // Create the initial triple
    let triple = create_initial_triple(&domain);

    // Handle storage based on whether we have a storage file
    match &storage_file {
        Some(file_path) => {
            // Ensure the storage directory exists
            if let Err(e) = ensure_directory_exists(file_path) {
                println!("Error creating storage directory: {}", e);
                return;
            }

            // Use the storage module to append the triple
            let mut storage = crate::storage::NtFileStorage::new(file_path);

            // Initialize storage if needed
            if let Err(e) = storage.init(&domain) {
                println!("Error initializing storage: {}", e);
                return;
            }

            // Append the triple to the storage file
            if let Err(e) = storage.append(&triple) {
                println!("Error appending to storage file: {}", e);
                return;
            }

            println!("Domain initialized with initial triple:");
            println!("{}", triple.to_ntriples());
            println!("Triple appended to storage file: {}", file_path);
        },
        None => {
            // If no storage file is defined, write to stdout
            println!("Domain initialized with initial triple:");
            println!("{}", triple.to_ntriples());
        }
    }
}
