use oxrdf::NamedNode;
use oxrdf::Triple;
use crate::triple::TripleExt;
use crate::crypto::hash_string;

#[test]
fn test_triple_and_claim_hash_uri_formats() {
    // Create a test triple
    let subject = NamedNode::new("https://example.com/subjects/1").unwrap();
    let property = NamedNode::new("https://example.com/properties/name").unwrap();
    let object = NamedNode::new("https://example.com/objects/John").unwrap();
    let triple = Triple::new(subject, property, object);

    // Generate a triple hash
    let triple_str = triple.to_ntriples();
    let triple_hash = hash_string(&triple_str);
    
    // Create a triple hash URI
    let triple_hash_uri = NamedNode::new(&format!("https://scatter.red/triple/{}", triple_hash)).unwrap();
    assert!(triple_hash_uri.as_str().to_string().starts_with("https://scatter.red/triple/"));
    assert_eq!(triple_hash_uri.as_str().to_string().len(), "https://scatter.red/triple/".len() + triple_hash.len());

    // Generate a claim hash
    let attribution = "https://example.com/user";
    let prev_claim = "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy";
    let timestamp = "2023-01-01T00:00:00Z";
    let sequence_id = "20230101000000000000";
    
    let mut combined = String::new();
    combined.push_str(&triple_hash);
    combined.push_str(attribution);
    combined.push_str(prev_claim);
    combined.push_str(timestamp);
    combined.push_str(sequence_id);
    
    let claim_hash = hash_string(&combined);
    
    // Create a claim hash URI
    let claim_hash_uri = NamedNode::new(&format!("https://scatter.red/claim/{}", claim_hash)).unwrap();
    assert!(claim_hash_uri.as_str().to_string().starts_with("https://scatter.red/claim/"));
    assert_eq!(claim_hash_uri.as_str().to_string().len(), "https://scatter.red/claim/".len() + claim_hash.len());
}