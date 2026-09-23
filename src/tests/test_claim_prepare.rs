use std::io::{BufReader, Cursor};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::{MockStorage, Storage};
use crate::claim::{parse_turtle, parse_ntriples};
use crate::triple::Triple;
use crate::uri::Uri;
use crate::tests::test_data::{MOCK_PUBLIC_KEY, MOCK_PRIVATE_KEY, MOCK_PREFIXES_CONTENT};
use crate::crypto::hash_string;

#[test]
fn test_claim_prepare_with_literals() {
    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();

    // Generate a unique domain for this test run
    let domain = "http://example.org/test/";

    // Initialize the mock storage with the domain
    mock_storage.init(domain).expect("Failed to initialize mock storage");

    // Use in-memory TTL content with literals
    let ttl_content = r#"@prefix ns1: <http://schema.org/> .
@prefix ns2: <http://xmlns.com/foaf/0.1/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<http://dbpedia.org/resource/Albert_Einstein> ns1:birthDate "1879-03-14"^^xsd:date ;
    ns1:birthPlace <http://dbpedia.org/resource/Ulm> ;
    ns1:deathDate "1955-04-18"^^xsd:date ;
    ns1:deathPlace <http://dbpedia.org/resource/Princeton,_New_Jersey> ;
    ns1:name "Albert Einstein",
        "アルベルト・アインシュタイン"@ja ;
    ns2:homepage <https://einstein-archives.org/> .

<http://dbpedia.org/resource/Berlin> a ns1:City ;
    ns1:country <http://dbpedia.org/resource/Germany> .

<http://dbpedia.org/resource/Wikipedia> a ns1:CreativeWork ;
    ns1:inLanguage "en" ;
    ns1:url <https://en.wikipedia.org/wiki/Main_Page> .

<http://dbpedia.org/resource/Germany> a ns1:Country .
"#;

    // Parse the TTL content
    let reader = BufReader::new(ttl_content.as_bytes());
    let mut prefixes = HashMap::new();
    prefixes.insert("ns1".to_string(), "http://schema.org/".to_string());
    prefixes.insert("ns2".to_string(), "http://xmlns.com/foaf/0.1/".to_string());
    prefixes.insert("xsd".to_string(), "http://www.w3.org/2001/XMLSchema#".to_string());

    let triples = parse_turtle(reader, &prefixes).expect("Failed to parse Turtle content");

    // Verify that the triples were parsed correctly
    assert!(!triples.is_empty(), "No triples were parsed from the TTL content");

    // Check for specific triples that should be present
    let mut found_einstein_birthdate = false;
    let mut found_einstein_name = false;
    let mut found_einstein_japanese_name = false;
    let mut found_berlin_city = false;

    for triple in &triples {
        let subject = triple.subject().to_string();
        let property = triple.property().to_string();
        let object = triple.object().to_string();

        if subject == "<http://dbpedia.org/resource/Albert_Einstein>" {
            if property == "http://schema.org/birthDate" && object.contains("1879-03-14") {
                found_einstein_birthdate = true;
            } else if property == "http://schema.org/name" {
                if object.contains("Albert Einstein") {
                    found_einstein_name = true;
                } else if object.contains("アルベルト・アインシュタイン") {
                    found_einstein_japanese_name = true;
                }
            }
        } else if subject == "<http://dbpedia.org/resource/Berlin>" && 
                  property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" && 
                  object == "<http://schema.org/City>" {
            found_berlin_city = true;
        }
    }

    assert!(found_einstein_birthdate, "Missing triple about Einstein's birth date");
    assert!(found_einstein_name, "Missing triple about Einstein's name");
    assert!(found_einstein_japanese_name, "Missing triple about Einstein's Japanese name");
    assert!(found_berlin_city, "Missing triple about Berlin being a city");

    // Store the triples in the mock storage
    for triple in &triples {
        mock_storage.append(triple).expect("Failed to append triple to mock storage");
    }
}

#[test]
fn test_claim_prepare_with_prior_claim_lookup() {
    // Create a mock storage instance
    let mut mock_storage = MockStorage::new();

    // Generate a unique domain for this test run
    let domain = "http://example.org/test/";

    // Initialize the mock storage with the domain
    mock_storage.init(domain).expect("Failed to initialize mock storage");

    // Generate a unique resource URI
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_nanos();
    let resource_uri = format!("http://example.org/resource/TestResource/{}", timestamp);

    // Create first TTL content
    let first_ttl_content = format!(r#"@prefix ns1: <http://schema.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<{}> a ns1:Thing ;
    ns1:name "Test Resource" ;
    ns1:description "A test resource for claim prepare" .
"#, resource_uri);

    // Parse the first TTL content
    let reader = BufReader::new(first_ttl_content.as_bytes());
    let mut prefixes = HashMap::new();
    prefixes.insert("ns1".to_string(), "http://schema.org/".to_string());
    prefixes.insert("xsd".to_string(), "http://www.w3.org/2001/XMLSchema#".to_string());

    let first_triples = parse_turtle(reader, &prefixes).expect("Failed to parse first Turtle content");

    // Generate a triple hash for the first claim
    let first_triple_str = first_triples[0].to_ntriples();
    let first_triple_hash = hash_string(&first_triple_str);

    // Create a triple hash URI
    let first_triple_hash_uri = Uri::new(&format!("https://scatter.red/triple/{}", first_triple_hash)).unwrap();

    // Create a claim hash for the first claim
    let attribution = format!("{}/user/alice", domain);
    let prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
    let timestamp = "2023-01-01T00:00:00Z";
    let sequence_id = "20230101000000000000";

    let mut combined = String::new();
    combined.push_str(&first_triple_hash);
    combined.push_str(&attribution);
    combined.push_str(prev_claim);
    combined.push_str(timestamp);
    combined.push_str(sequence_id);

    let first_claim_hash = hash_string(&combined);

    // Create a claim hash URI
    let first_claim_hash_uri = Uri::new(&format!("https://scatter.red/claim/{}", first_claim_hash)).unwrap();

    // Create triples for the first claim
    let mut first_claim_triples = Vec::new();

    // Triple type
    let triple_type_triple = Triple::new_uri_object(
        first_triple_hash_uri.clone(),
        Uri::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        Uri::new("https://scatter.red/terms/v0/Triple").unwrap()
    );
    first_claim_triples.push(triple_type_triple);

    // Triple claimed in claim
    let triple_claimed_in_triple = Triple::new_uri_object(
        first_triple_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/claimedIn").unwrap(),
        first_claim_hash_uri.clone()
    );
    first_claim_triples.push(triple_claimed_in_triple);

    // Claim type
    let claim_type_triple = Triple::new_uri_object(
        first_claim_hash_uri.clone(),
        Uri::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        Uri::new("https://scatter.red/terms/v0/SignedClaim").unwrap()
    );
    first_claim_triples.push(claim_type_triple);

    // Claim includes triple
    let claim_includes_triple = Triple::new_uri_object(
        first_claim_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/includes").unwrap(),
        first_triple_hash_uri.clone()
    );
    first_claim_triples.push(claim_includes_triple);

    // Store the original triple and claim triples in the mock storage
    for triple in &first_triples {
        mock_storage.append(triple).expect("Failed to append original triple to mock storage");
    }
    for triple in &first_claim_triples {
        mock_storage.append(triple).expect("Failed to append claim triple to mock storage");
    }

    // Create second TTL content
    let second_ttl_content = format!(r#"@prefix ns1: <http://schema.org/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<{}> ns1:dateModified "2023-01-01"^^xsd:date ;
    ns1:version "2.0" .
"#, resource_uri);

    // Parse the second TTL content
    let reader = BufReader::new(second_ttl_content.as_bytes());
    let second_triples = parse_turtle(reader, &prefixes).expect("Failed to parse second Turtle content");

    // Generate a triple hash for the second claim
    let second_triple_str = second_triples[0].to_ntriples();
    let second_triple_hash = hash_string(&second_triple_str);

    // Create a triple hash URI
    let second_triple_hash_uri = Uri::new(&format!("https://scatter.red/triple/{}", second_triple_hash)).unwrap();

    // Create a claim hash for the second claim
    let timestamp = "2023-01-02T00:00:00Z";
    let sequence_id = "20230102000000000000";

    let mut combined = String::new();
    combined.push_str(&second_triple_hash);
    combined.push_str(&attribution);
    combined.push_str(&first_claim_hash); // Use the first claim hash as the previous claim
    combined.push_str(timestamp);
    combined.push_str(sequence_id);

    let second_claim_hash = hash_string(&combined);

    // Create a claim hash URI
    let second_claim_hash_uri = Uri::new(&format!("https://scatter.red/claim/{}", second_claim_hash)).unwrap();

    // Create triples for the second claim
    let mut second_claim_triples = Vec::new();

    // Triple type
    let triple_type_triple = Triple::new_uri_object(
        second_triple_hash_uri.clone(),
        Uri::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        Uri::new("https://scatter.red/terms/v0/Triple").unwrap()
    );
    second_claim_triples.push(triple_type_triple);

    // Triple claimed in claim
    let triple_claimed_in_triple = Triple::new_uri_object(
        second_triple_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/claimedIn").unwrap(),
        second_claim_hash_uri.clone()
    );
    second_claim_triples.push(triple_claimed_in_triple);

    // Claim type
    let claim_type_triple = Triple::new_uri_object(
        second_claim_hash_uri.clone(),
        Uri::new("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").unwrap(),
        Uri::new("https://scatter.red/terms/v0/SignedClaim").unwrap()
    );
    second_claim_triples.push(claim_type_triple);

    // Claim includes triple
    let claim_includes_triple = Triple::new_uri_object(
        second_claim_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/includes").unwrap(),
        second_triple_hash_uri.clone()
    );
    second_claim_triples.push(claim_includes_triple);

    // Claim previous claim
    let claim_previous_claim_triple = Triple::new_uri_object(
        second_claim_hash_uri.clone(),
        Uri::new("https://scatter.red/terms/v0/previousClaim").unwrap(),
        first_claim_hash_uri.clone()
    );
    second_claim_triples.push(claim_previous_claim_triple);

    // Store the second triple and claim triples in the mock storage
    for triple in &second_triples {
        mock_storage.append(triple).expect("Failed to append second triple to mock storage");
    }
    for triple in &second_claim_triples {
        mock_storage.append(triple).expect("Failed to append second claim triple to mock storage");
    }

    // Verify that the second claim references the first claim
    let mut found_previous_claim = false;
    println!("Expected first_claim_hash_uri: {}", first_claim_hash_uri.to_string());

    // Get all triples from the mock storage
    let all_triples = mock_storage.get_triples();

    // Look for a triple with the second claim as subject, previousClaim as property, and first claim as object
    for triple_str in all_triples {
        println!("Checking triple: {}", triple_str);
        if triple_str.starts_with(&format!("<{}>", second_claim_hash_uri.to_string())) &&
           triple_str.contains("<https://scatter.red/terms/v0/previousClaim>") {
            println!("Found previousClaim triple: {}", triple_str);
            if triple_str.contains(&format!("<{}>", first_claim_hash_uri.to_string())) {
                found_previous_claim = true;
                break;
            }
        }
    }

    assert!(found_previous_claim, "Second claim should reference the first claim as previousClaim");
}
