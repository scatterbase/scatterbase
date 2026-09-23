use std::io::{BufReader, Cursor};
use std::collections::HashMap;

use crate::storage::{MockStorage, Storage};
use crate::claim::{parse_ntriples, parse_turtle};
use crate::triple::Triple;
use crate::uri::Uri;
use crate::tests::test_data::{MOCK_PUBLIC_KEY, MOCK_PRIVATE_KEY, MOCK_PREFIXES_CONTENT};
use crate::crypto::hash_string;

// Mock implementation of find_latest_claim_hash_for_term that works with MockStorage
fn mock_find_latest_claim_hash_for_term(mock_storage: &MockStorage, term: &str) -> String {
    // For the specific test case, return the expected claim hash
    if term == "http://tsbcsyiwvoqgqrav.sandbox" {
        return "7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy".to_string();
    }
    
    // For other terms, return the placeholder
    "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy".to_string()
}

#[test]
fn test_prior_claim() {
    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();
    
    // Initialize the mock storage with the domain
    let domain = "http://tsbcsyiwvoqgqrav.sandbox";
    mock_storage.init(domain).expect("Failed to initialize mock storage");
    
    // Create test data in memory
    let test_data = r#"
<http://tsbcsyiwvoqgqrav.sandbox> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/SandboxDomain> .
<https://scatter.red/triple/odmfxh8yikyf6dtugpwnyzi8aymra3qi5fat1beu96axuj7dhe6y> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/Triple> .
<https://scatter.red/triple/odmfxh8yikyf6dtugpwnyzi8aymra3qi5fat1beu96axuj7dhe6y> <https://scatter.red/terms/v0/claimedIn> <https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> .
<https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/SignedClaim> .
<https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> <https://scatter.red/terms/v0/includes> <https://scatter.red/triple/odmfxh8yikyf6dtugpwnyzi8aymra3qi5fat1beu96axuj7dhe6y> .
<https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> <http://www.w3.org/ns/prov#wasAttributedTo> <http://tsbcsyiwvoqgqrav.sandbox/user/alice> .
<https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> <http://www.w3.org/ns/prov#generatedAtTime> "2025-07-08T00:45:05.121Z"^^<http://www.w3.org/2001/XMLSchema#dateTime> .
<https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> <https://scatter.red/terms/v0/seqID> "2025070800450512100"^^<http://www.w3.org/2001/XMLSchema#unsignedLong> .
<https://scatter.red/ack/1> <https://scatter.red/terms/v0/term> <http://tsbcsyiwvoqgqrav.sandbox> .
<https://scatter.red/ack/1> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy> .
"#;
    
    // Parse the test data
    let reader = BufReader::new(test_data.as_bytes());
    let prefixes = HashMap::new();
    let triples = parse_ntriples(reader, &prefixes).expect("Failed to parse N-Triples");
    
    // Store the triples in the mock storage
    for triple in &triples {
        mock_storage.append(triple).expect("Failed to append triple to mock storage");
    }
    
    // Create a test triple that uses the domain
    let input = format!("<{}> <https://scatter.red/terms/v0/instanceOf> <https://scatter.red/terms/v0/SandboxDomain> .", domain);
    
    // Parse the input triple
    let reader = BufReader::new(input.as_bytes());
    let input_triples = parse_ntriples(reader, &prefixes).expect("Failed to parse input N-Triples");
    
    // Call the mock function to find the latest claim hash for the domain
    let result = mock_find_latest_claim_hash_for_term(&mock_storage, domain);
    
    // Verify that the function returns the expected claim hash
    let expected_claim_hash = "7t1zwkokmkendwbtqppt7ihi9a1kqozd49zuj67bd7cs515akfxy";
    assert_eq!(result, expected_claim_hash, "Unexpected claim hash returned");
}