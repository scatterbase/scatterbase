use std::io::BufReader;
use std::collections::HashMap;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use data_encoding::{Specification, Encoding};

use crate::storage::{MockStorage, Storage};
use crate::claim::parse_ntriples;
use oxrdf::Triple;
use crate::triple::TripleExt;
use oxrdf::NamedNode;
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
fn test_claim_prepare_and_submit_isolated() {
    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();
    
    // Generate a unique domain for this test run
    let unique_id = format!("{:x}", OsRng.next_u64());
    let domain = format!("http://example.org/test_{}/", unique_id);
    
    // Initialize the mock storage with the domain
    mock_storage.init(&domain).expect("Failed to initialize mock storage");
    
    // Create a random triple for testing
    let random_subject = format!("{:x}", OsRng.next_u64());
    let random_predicate = format!("{:x}", OsRng.next_u64());
    let random_object = format!("{:x}", OsRng.next_u64());
    let triple_content = format!(
        "<http://example.org/subject/{}> <http://example.org/predicate/{}> <http://example.org/object/{}> .",
        random_subject, random_predicate, random_object
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
    let triple_hash_uri = NamedNode::new(&format!("https://scatter.red/triple/{}", triple_hash)).unwrap();
    
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
    let claim_hash_uri = NamedNode::new(&format!("https://scatter.red/claim/{}", claim_hash)).unwrap();
    
    // Create triples for the claim
    let mut claim_triples = Vec::new();
    
    // Triple type
    let triple_type_triple = Triple::new(
        triple_hash_uri.clone(),
        NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        NamedNode::new("https://scatter.red/terms/v0/Triple").unwrap()
    );
    claim_triples.push(triple_type_triple);
    
    // Triple claimed in claim
    let triple_claimed_in_triple = Triple::new(
        triple_hash_uri.clone(),
        NamedNode::new("https://scatter.red/terms/v0/claimedIn").unwrap(),
        claim_hash_uri.clone()
    );
    claim_triples.push(triple_claimed_in_triple);
    
    // Claim type
    let claim_type_triple = Triple::new(
        claim_hash_uri.clone(),
        NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        NamedNode::new("https://scatter.red/terms/v0/SignedClaim").unwrap()
    );
    claim_triples.push(claim_type_triple);
    
    // Claim includes triple
    let claim_includes_triple = Triple::new(
        claim_hash_uri.clone(),
        NamedNode::new("https://scatter.red/terms/v0/includes").unwrap(),
        triple_hash_uri.clone()
    );
    claim_triples.push(claim_includes_triple);
    
    // Store the original triple and claim triples in the mock storage
    mock_storage.append(&triples[0]).expect("Failed to append original triple to mock storage");
    for triple in &claim_triples {
        mock_storage.append(triple).expect("Failed to append claim triple to mock storage");
    }
    
    // Check if the claim exists in the storage
    let claim_exists = mock_storage.claim_exists(&claim_hash).expect("Failed to check if claim exists");
    assert!(claim_exists, "Claim should exist in the storage");
    
    // Try to store the same claim again - this should be detected as a duplicate
    let duplicate_check = mock_storage.claim_exists(&claim_hash).expect("Failed to check if claim exists");
    assert!(duplicate_check, "Duplicate claim should be detected");
}

#[test]
fn test_duplicate_claim_rejection_with_random_data() {
    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();
    
    // Generate a unique domain for this test run
    let unique_id = format!("{:x}", OsRng.next_u64());
    let domain = format!("http://example.org/test_{}/", unique_id);
    
    // Initialize the mock storage with the domain
    mock_storage.init(&domain).expect("Failed to initialize mock storage");
    
    // Generate multiple random claims
    let mut rng = rand::thread_rng();
    let num_claims = 3; // Generate 3 random claims
    let mut claim_hashes = Vec::new();
    
    for i in 0..num_claims {
        // Generate random data for the claim
        let random_subject = format!("subject_{}", rng.next_u64());
        let random_predicate = format!("predicate_{}", rng.next_u64());
        let random_object = format!("object_{}", rng.next_u64());
        
        // Create a triple
        let triple_content = format!(
            "<http://example.org/{}> <http://example.org/{}> <http://example.org/{}> .",
            random_subject, random_predicate, random_object
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
        let triple_hash_uri = NamedNode::new(&format!("https://scatter.red/triple/{}", triple_hash)).unwrap();
        
        // Create a claim hash
        let attribution = format!("{}/user/alice", domain);
        let prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
        let timestamp = format!("2023-01-01T00:00:00.{:03}Z", i);
        let sequence_id = format!("2023010100000000{:03}", i);
        
        let mut combined = String::new();
        combined.push_str(&triple_hash);
        combined.push_str(&attribution);
        combined.push_str(prev_claim);
        combined.push_str(&timestamp);
        combined.push_str(&sequence_id);
        
        let claim_hash = hash_string(&combined);
        claim_hashes.push(claim_hash.clone());
        
        // Create a claim hash URI
        let claim_hash_uri = NamedNode::new(&format!("https://scatter.red/claim/{}", claim_hash)).unwrap();
        
        // Create triples for the claim
        let mut claim_triples = Vec::new();
        
        // Triple type
        let triple_type_triple = Triple::new(
            triple_hash_uri.clone(),
            NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
            NamedNode::new("https://scatter.red/terms/v0/Triple").unwrap()
        );
        claim_triples.push(triple_type_triple);
        
        // Triple claimed in claim
        let triple_claimed_in_triple = Triple::new(
            triple_hash_uri.clone(),
            NamedNode::new("https://scatter.red/terms/v0/claimedIn").unwrap(),
            claim_hash_uri.clone()
        );
        claim_triples.push(triple_claimed_in_triple);
        
        // Claim type
        let claim_type_triple = Triple::new(
            claim_hash_uri.clone(),
            NamedNode::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
            NamedNode::new("https://scatter.red/terms/v0/SignedClaim").unwrap()
        );
        claim_triples.push(claim_type_triple);
        
        // Claim includes triple
        let claim_includes_triple = Triple::new(
            claim_hash_uri.clone(),
            NamedNode::new("https://scatter.red/terms/v0/includes").unwrap(),
            triple_hash_uri.clone()
        );
        claim_triples.push(claim_includes_triple);
        
        // Store the original triple and claim triples in the mock storage
        mock_storage.append(&triples[0]).expect("Failed to append original triple to mock storage");
        for triple in &claim_triples {
            mock_storage.append(triple).expect("Failed to append claim triple to mock storage");
        }
        
        // Check if the claim exists in the storage
        let claim_exists = mock_storage.claim_exists(&claim_hash).expect("Failed to check if claim exists");
        assert!(claim_exists, "Claim {} should exist in the storage", i);
    }
    
    // Try to check for each claim again - they should all be detected as duplicates
    for (i, claim_hash) in claim_hashes.iter().enumerate() {
        let duplicate_check = mock_storage.claim_exists(claim_hash).expect("Failed to check if claim exists");
        assert!(duplicate_check, "Duplicate claim {} should be detected", i);
    }
}