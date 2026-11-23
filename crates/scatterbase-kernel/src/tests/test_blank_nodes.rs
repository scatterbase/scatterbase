use std::io::BufReader;
use std::collections::HashMap;

use crate::claim::parse_turtle;
use crate::tests::test_data::TEST_BLANK_NODES_CONTENT;
use crate::storage::{MockStorage, Storage};

#[test]
fn test_claim_prepare_with_blank_nodes() {
    // Use in-memory test data instead of reading from a file
    let test_content = TEST_BLANK_NODES_CONTENT;

    // Parse the test content directly
    let reader = BufReader::new(test_content.as_bytes());
    let mut prefixes = HashMap::new();
    prefixes.insert("schema".to_string(), "http://schema.org/".to_string());
    prefixes.insert("foaf".to_string(), "http://xmlns.com/foaf/0.1/".to_string());
    prefixes.insert("xsd".to_string(), "http://www.w3.org/2001/XMLSchema#".to_string());

    let triples = parse_turtle(reader, &prefixes).expect("Failed to parse Turtle content");

    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();
    let domain = "http://example.org/";
    mock_storage.init(domain).expect("Failed to initialize mock storage");

    // Store the triples in the mock storage
    for triple in &triples {
        mock_storage.append(triple).expect("Failed to append triple to mock storage");
    }

    // Verify that the triples were parsed correctly
    assert!(!triples.is_empty(), "No triples were parsed from the test content");

    // Check for specific triples that should be present
    let mut found_alice = false;
    let mut found_bob = false;
    let mut found_conference = false;

    for triple in &triples {
        let subject = triple.subject.to_string();
        let property = triple.predicate.as_str().to_string();
        let object = triple.object.to_string();

        if subject == "<http://example.org/person/alice>" && property == "http://xmlns.com/foaf/0.1/name" {
            if object.contains("Alice") {
                found_alice = true;
            }
        } else if property == "http://xmlns.com/foaf/0.1/name" && object.contains("Bob") {
            found_bob = true;
        } else if property == "http://schema.org/name" && object.contains("Conference") {
            found_conference = true;
        }
    }

    assert!(found_alice, "Missing triple about Alice");
    assert!(found_bob, "Missing triple about Bob");
    assert!(found_conference, "Missing triple about Conference");
}

#[test]
fn test_anonymous_blank_nodes_get_stable_labels() {
    // `_:anon1` is written in the input, so the first unlabelled node must not take it
    let turtle = r#"@prefix ex: <http://example.org/> .
ex:a ex:knows [ ex:name "first" ] , [ ex:name "second" ] .
_:anon1 ex:name "written" .
"#;
    let parse = || parse_turtle(BufReader::new(turtle.as_bytes()), &HashMap::new()).unwrap();

    let triples = parse();
    let subjects: Vec<String> = triples.iter()
        .filter(|t| t.predicate.as_str() == "http://example.org/name")
        .map(|t| t.subject.to_string())
        .collect();
    assert_eq!(subjects.len(), 3);
    assert!(subjects[0].ends_with("/anon2>"), "{}", subjects[0]);
    assert!(subjects[1].ends_with("/anon3>"), "{}", subjects[1]);
    assert!(subjects[2].ends_with("/anon1>"), "{}", subjects[2]);

    // Reading the same input again gives the same triples
    assert_eq!(triples, parse());
}
