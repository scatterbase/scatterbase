use std::io::{BufReader, Cursor};
use std::collections::HashMap;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use data_encoding::{Specification, Encoding};

use crate::storage::{MockStorage, Storage};
use crate::claim::{parse_ntriples, parse_turtle};
use crate::triple::Triple;
use crate::uri::Uri;
use crate::tests::test_data::{MOCK_PUBLIC_KEY, MOCK_PRIVATE_KEY, MOCK_PREFIXES_CONTENT};
use crate::crypto::hash_string;

// Define Base32z encoding specification
fn get_base32z_encoding() -> Encoding {
    let mut spec = Specification::new();
    spec.symbols = "ybndrfg8ejkmcpqxot1uwisza345h769".to_string();
    spec.padding = None;
    spec.ignore = "".to_string();
    spec.encoding().unwrap()
}

// Generates an ED25519 key pair
fn generate_ed25519_keypair() -> (SigningKey, VerifyingKey) {
    let mut csprng = OsRng;
    let mut secret_key_bytes = [0u8; 32];
    csprng.try_fill_bytes(&mut secret_key_bytes).expect("Failed to generate random bytes");
    let signing_key = SigningKey::from_bytes(&secret_key_bytes);
    let verifying_key = signing_key.verifying_key();
    (signing_key, verifying_key)
}

#[test]
fn test_claim_write_process() {
    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();

    // Generate a unique domain for this test run
    let unique_id = format!("{:x}", OsRng.next_u64());
    let domain = format!("http://example.org/test_{}/", unique_id);

    // Initialize the mock storage with the domain
    mock_storage.init(&domain).expect("Failed to initialize mock storage");

    // Create a random triple for testing
    let random_id = format!("{:x}", OsRng.next_u64());
    let triple_content = format!(
        "<http://example.org/subject/{}> <http://example.org/predicate> <http://example.org/object> .",
        random_id
    );

    // Parse the triple
    let reader = BufReader::new(triple_content.as_bytes());
    let prefixes = HashMap::new();
    let triples = parse_ntriples(reader, &prefixes).expect("Failed to parse N-Triples");

    // Verify that we have one triple
    assert_eq!(triples.len(), 1, "Expected 1 triple, got {}", triples.len());

    // Generate a triple hash
    let triple_str = triples[0].to_ntriples();
    let triple_hash = hash_string(&triple_str);

    // Create a triple hash URI
    let triple_hash_uri = Uri::new(&format!("https://scatter.red/triple/{}", triple_hash)).unwrap();

    // Create a claim hash
    let attribution = format!("{}/user/alice", domain);
    let prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
    let timestamp = "2023-01-01T00:00:00Z";
    let sequence_id = "20230101000000000000";

    let mut combined = String::new();
    combined.push_str(&triple_hash);
    combined.push_str(&attribution);
    combined.push_str(prev_claim);
    combined.push_str(timestamp);
    combined.push_str(sequence_id);

    let claim_hash = hash_string(&combined);

    // Create a claim hash URI
    let claim_hash_uri = Uri::new(&format!("https://scatter.red/claim/{}", claim_hash)).unwrap();

    // Create triples for the claim
    let mut claim_triples = Vec::new();

    // Triple type
    let triple_type_triple = Triple::new_uri_object(
        triple_hash_uri.clone(),
        Uri::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        Uri::new("https://scatter.red/terms/v0/Triple").unwrap()
    );
    claim_triples.push(triple_type_triple);

    // Triple claimed in claim
    let triple_claimed_in_triple = Triple::new_uri_object(
        triple_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/claimedIn").unwrap(),
        claim_hash_uri.clone()
    );
    claim_triples.push(triple_claimed_in_triple);

    // Claim type
    let claim_type_triple = Triple::new_uri_object(
        claim_hash_uri.clone(),
        Uri::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        Uri::new("https://scatter.red/terms/v0/SignedClaim").unwrap()
    );
    claim_triples.push(claim_type_triple);

    // Claim includes triple
    let claim_includes_triple = Triple::new_uri_object(
        claim_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/includes").unwrap(),
        triple_hash_uri.clone()
    );
    claim_triples.push(claim_includes_triple);

    // Claim attribution
    let claim_attribution_triple = Triple::new_uri_object(
        claim_hash_uri.clone(),
        Uri::new("http://www.w3.org/ns/prov#wasAttributedTo").unwrap(),
        Uri::new(&attribution).unwrap()
    );
    claim_triples.push(claim_attribution_triple);

    // Claim timestamp
    let claim_timestamp_triple = Triple::new_literal_object(
        claim_hash_uri.clone(),
        Uri::new("http://www.w3.org/ns/prov#generatedAtTime").unwrap(),
        crate::literal::Literal::with_datatype(
            timestamp,
            Uri::new("http://www.w3.org/2001/XMLSchema#dateTime").unwrap()
        )
    );
    claim_triples.push(claim_timestamp_triple);

    // Claim sequence ID
    let claim_sequence_id_triple = Triple::new_literal_object(
        claim_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/seqID").unwrap(),
        crate::literal::Literal::with_datatype(
            sequence_id,
            Uri::new("http://www.w3.org/2001/XMLSchema#unsignedLong").unwrap()
        )
    );
    claim_triples.push(claim_sequence_id_triple);

    // Claim signature
    let signature = "ed25519:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let claim_signature_triple = Triple::new_literal_object(
        claim_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/userSignature").unwrap(),
        crate::literal::Literal::with_datatype(
            signature,
            Uri::new("https://scatter.red/terms/v0/Base64").unwrap()
        )
    );
    claim_triples.push(claim_signature_triple);

    // Store the original triple and claim triples in the mock storage
    mock_storage.append(&triples[0]).expect("Failed to append original triple to mock storage");
    for triple in &claim_triples {
        mock_storage.append(triple).expect("Failed to append claim triple to mock storage");
    }

    // Check if the claim exists in the storage
    let claim_exists = mock_storage.claim_exists(&claim_hash).expect("Failed to check if claim exists");
    assert!(claim_exists, "Claim should exist in the storage");

    // Verify that all required triples are in the storage
    let mut found_triple_type = false;
    let mut found_triple_claimed_in = false;
    let mut found_claim_type = false;
    let mut found_claim_includes = false;
    let mut found_claim_attribution = false;
    let mut found_claim_timestamp = false;
    let mut found_claim_sequence_id = false;
    let mut found_claim_signature = false;

    println!("Triple hash URI: {}", triple_hash_uri.to_string());
    println!("Claim hash URI: {}", claim_hash_uri.to_string());

    for triple in &claim_triples {
        let subject = triple.subject().to_string();
        let property = triple.property().to_string();

        println!("Checking triple - Subject: {}, Property: {}", subject, property);

        // Format the URIs with angle brackets to match the format in the triple
        let formatted_triple_hash_uri = format!("<{}>", triple_hash_uri.to_string());
        let formatted_claim_hash_uri = format!("<{}>", claim_hash_uri.to_string());

        if subject == formatted_triple_hash_uri {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                found_triple_type = true;
                println!("Found triple type triple!");
            } else if property == "https://scatter.red/terms/v0/claimedIn" {
                found_triple_claimed_in = true;
                println!("Found triple claimed in triple!");
            }
        } else if subject == formatted_claim_hash_uri {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                found_claim_type = true;
            } else if property == "https://scatter.red/terms/v0/includes" {
                found_claim_includes = true;
            } else if property == "http://www.w3.org/ns/prov#wasAttributedTo" {
                found_claim_attribution = true;
            } else if property == "http://www.w3.org/ns/prov#generatedAtTime" {
                found_claim_timestamp = true;
            } else if property == "https://scatter.red/terms/v0/seqID" {
                found_claim_sequence_id = true;
            } else if property == "https://scatter.red/terms/v0/userSignature" {
                found_claim_signature = true;
            }
        }
    }

    assert!(found_triple_type, "Triple type triple not found");
    assert!(found_triple_claimed_in, "Triple claimed in triple not found");
    assert!(found_claim_type, "Claim type triple not found");
    assert!(found_claim_includes, "Claim includes triple not found");
    assert!(found_claim_attribution, "Claim attribution triple not found");
    assert!(found_claim_timestamp, "Claim timestamp triple not found");
    assert!(found_claim_sequence_id, "Claim sequence ID triple not found");
    assert!(found_claim_signature, "Claim signature triple not found");
}
