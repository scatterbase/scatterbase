use crate::uri::Uri;
use crate::triple::Triple;
use crate::literal::Literal;

#[test]
fn test_rdf_star_formatting() {
    // Create a test triple
    let subject = Uri::new("https://example.com/subjects/1").unwrap();
    let property = Uri::new("https://example.com/properties/name").unwrap();
    let object = Uri::new("https://example.com/objects/John").unwrap();
    let inner_triple = Triple::new_uri_object(subject, property, object);

    // Create an outer triple that uses the inner triple as its subject
    let outer_property = Uri::new("https://example.com/properties/confidence").unwrap();
    let outer_object = Literal::new("0.95");
    let outer_triple = Triple::new_triple_subject_literal_object(inner_triple.clone(), outer_property, outer_object);

    // Check that the N-Triples representation doesn't have doubled angle brackets
    let ntriples = outer_triple.to_ntriples();
    println!("N-Triples: {}", ntriples);
    
    // The correct format should be: <<https://example.com/subjects/1> <https://example.com/properties/name> <https://example.com/objects/John> .>> <https://example.com/properties/confidence> "0.95" .
    // Not: <<<https://example.com/subjects/1>> <https://example.com/properties/name> <<https://example.com/objects/John>> .>> <https://example.com/properties/confidence> "0.95" .
    
    assert!(ntriples.contains("<<https://example.com/subjects/1>"));
    assert!(!ntriples.contains("<<<"));
    assert!(!ntriples.contains(">>>"));
}

#[test]
fn test_nested_rdf_star_formatting() {
    // Create an innermost triple
    let subject1 = Uri::new("https://example.com/subjects/1").unwrap();
    let property1 = Uri::new("https://example.com/properties/name").unwrap();
    let object1 = Uri::new("https://example.com/objects/John").unwrap();
    let triple1 = Triple::new_uri_object(subject1, property1, object1);
    
    // Create a middle triple that uses the innermost triple as its subject
    let property2 = Uri::new("https://example.com/properties/confidence").unwrap();
    let object2 = Literal::new("0.95");
    let triple2 = Triple::new_triple_subject_literal_object(triple1.clone(), property2, object2);
    
    // Create an outermost triple that uses the middle triple as its subject
    let property3 = Uri::new("https://example.com/properties/source").unwrap();
    let object3 = Uri::new("https://example.com/sources/database").unwrap();
    let triple3 = Triple::new_triple_subject_uri_object(triple2.clone(), property3, object3);
    
    // Check that the N-Triples representation doesn't have doubled angle brackets
    let ntriples = triple3.to_ntriples();
    println!("Nested N-Triples: {}", ntriples);
    
    // The correct format should have properly nested angle brackets without doubling
    assert!(!ntriples.contains("<<<"));
    assert!(!ntriples.contains(">>>"));
}