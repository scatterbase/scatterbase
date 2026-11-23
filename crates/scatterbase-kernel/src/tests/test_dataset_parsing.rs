use std::io::BufReader;
use std::collections::HashMap;

use crate::claim::parse_ntriples;
use crate::term::Term;
use crate::tests::test_data::TEST_DATASET_CONTENT;

#[test]
fn test_parse_dataset() {
    // Use in-memory test data instead of reading from a file
    let file_content = TEST_DATASET_CONTENT;
    println!("File content:\n{}", file_content);
    let reader = BufReader::new(file_content.as_bytes());

    // Parse the N-Triples file
    let prefixes = HashMap::new();
    let triples = parse_ntriples(reader, &prefixes).expect("Failed to parse N-Triples file");

    // Print the triples for debugging
    println!("Parsed {} triples:", triples.len());
    for triple in &triples {
        println!("{}", triple.to_ntriples());

        // More detailed debugging for all triples
        let subject = triple.subject().to_string();
        let property = triple.property().to_string();
        let object = triple.object();

        println!("DEBUG: Subject: '{}', Property: '{}', Object: '{}'", subject, property, object);
    }

    // Verify that the basic triples were parsed correctly
    let mut found_dataset_type = false;
    let mut found_claim_type = false;
    let mut found_triple_type = false;
    let mut found_claim_includes_triple = false;
    let mut found_triple_claimed_in = false;
    let mut found_claim_attributed_to = false;
    let mut found_claim_generated_at_time = false;
    let mut found_claim_seq_id = false;
    let mut found_ack_term1 = false;
    let mut found_ack_prior_claim1 = false;
    let mut found_ack_term2 = false;
    let mut found_ack_prior_claim2 = false;
    let mut found_claim_acknowledges_prior_use1 = false;
    let mut found_claim_acknowledges_prior_use2 = false;
    let mut found_claim_user_signature = false;

    for triple in &triples {
        let subject = triple.subject().to_string();
        let property = triple.property().to_string();
        let object = triple.object();

        // Print more detailed information about each triple for debugging
        println!("Checking triple - Subject: {}, Property: {}, Object type: {:?}", 
                 subject, property, object);

        if subject == "<http://uwlcekqxsvwtoxzm.sandbox>" {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                if object.to_string() == "<http://schema.org/Dataset>" {
                    found_dataset_type = true;
                }
            }
        } else if subject.contains("https://scatter.red/triple/") {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                if object.to_string() == "<https://scatter.red/terms/v0/Triple>" {
                    found_triple_type = true;
                }
            } else if property == "https://scatter.red/terms/v0/claimedIn" {
                if object.to_string().contains("https://scatter.red/claim/") {
                    found_triple_claimed_in = true;
                }
            }
        } else if subject.contains("https://scatter.red/claim/") {
            if property == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type" {
                if object.to_string() == "<https://scatter.red/terms/v0/SignedClaim>" {
                    found_claim_type = true;
                }
            } else if property == "https://scatter.red/terms/v0/includes" {
                if object.to_string().contains("https://scatter.red/triple/") {
                    found_claim_includes_triple = true;
                }
            } else if property == "http://www.w3.org/ns/prov#wasAttributedTo" {
                if object.to_string().contains("http://uwlcekqxsvwtoxzm.sandbox/user/") {
                    found_claim_attributed_to = true;
                }
            } else if property == "http://www.w3.org/ns/prov#generatedAtTime" {
                if let Term::Literal(literal) = object {
                    if literal.lexical_form().contains("2025-07-08T00:45:05.121Z") {
                        found_claim_generated_at_time = true;
                    }
                }
            } else if property == "https://scatter.red/terms/v0/seqID" {
                if let Term::Literal(literal) = object {
                    if literal.lexical_form() == "2025070800450512100" {
                        found_claim_seq_id = true;
                    }
                }
            } else if property == "https://scatter.red/terms/v0/acknowledgesPriorUse" {
                if object.to_string().contains("https://scatter.red/ack/") {
                    // We need to track two different acknowledgments
                    if !found_claim_acknowledges_prior_use1 {
                        found_claim_acknowledges_prior_use1 = true;
                    } else {
                        found_claim_acknowledges_prior_use2 = true;
                    }
                }
            } else if property == "https://scatter.red/terms/v0/userSignature" {
                if let Term::Literal(literal) = object {
                    if literal.lexical_form().contains("ed25519:") {
                        found_claim_user_signature = true;
                    }
                }
            }
        } else if subject.contains("https://scatter.red/ack/") {
            if property == "https://scatter.red/terms/v0/term" {
                if object.to_string() == "<http://uwlcekqxsvwtoxzm.sandbox>" {
                    found_ack_term1 = true;
                } else if object.to_string() == "<http://schema.org/Dataset>" {
                    found_ack_term2 = true;
                }
            } else if property == "https://scatter.red/terms/v0/priorClaim" {
                if object.to_string().contains("https://scatter.red/claim/yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy") {
                    // We need to track two different prior claims
                    if !found_ack_prior_claim1 {
                        found_ack_prior_claim1 = true;
                    } else {
                        found_ack_prior_claim2 = true;
                    }
                }
            }
        }
    }

    // Verify that all triples were found
    assert!(found_dataset_type, "Missing triple: sandbox a schema:Dataset");
    assert!(found_claim_type, "Missing triple: claim a scatter:SignedClaim");
    assert!(found_triple_type, "Missing triple: triple a scatter:Triple");
    assert!(found_claim_includes_triple, "Missing triple: claim scatter:includes triple");
    assert!(found_triple_claimed_in, "Missing triple: triple scatter:claimedIn claim");
    assert!(found_claim_attributed_to, "Missing triple: claim prov:wasAttributedTo user");
    assert!(found_claim_generated_at_time, "Missing triple: claim prov:generatedAtTime dateTime");
    assert!(found_claim_seq_id, "Missing triple: claim scatter:seqID unsignedLong");
    assert!(found_ack_term1, "Missing triple: ack1 scatter:term sandbox");
    assert!(found_ack_prior_claim1, "Missing triple: ack1 scatter:priorClaim claim");
    assert!(found_ack_term2, "Missing triple: ack2 scatter:term Dataset");
    assert!(found_ack_prior_claim2, "Missing triple: ack2 scatter:priorClaim claim");
    assert!(found_claim_acknowledges_prior_use1, "Missing triple: claim scatter:acknowledgesPriorUse ack1");
    assert!(found_claim_acknowledges_prior_use2, "Missing triple: claim scatter:acknowledgesPriorUse ack2");
    assert!(found_claim_user_signature, "Missing triple: claim scatter:userSignature signature");
}
