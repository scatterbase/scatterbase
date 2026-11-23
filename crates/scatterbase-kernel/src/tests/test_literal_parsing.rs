use std::io::Cursor;
use std::collections::HashMap;
use crate::claim::parse_ntriples;

#[test]
fn test_parse_literal_object() {
    // Create a simple N-Triples string with a literal object
    let ntriples = "<http://example.org/subject> <http://example.org/predicate> \"Literal value\" .\n";

    // Parse the N-Triples
    let reader = Cursor::new(ntriples);
    let prefixes = HashMap::new();

    // Use the parse_ntriples function directly
    let triples = parse_ntriples(reader, &prefixes).unwrap();

    // Verify that we got one triple
    assert_eq!(triples.len(), 1);

    // Verify that the object is a literal with the correct value
    let triple = &triples[0];
    assert!(triple.object().is_literal());
    if let Some(literal) = triple.object().as_literal() {
        assert_eq!(literal.lexical_form(), "Literal value");
    } else {
        panic!("Expected a literal object");
    }
}

#[test]
fn test_parse_typed_literal_object() {
    // Create an N-Triples string with a typed literal object
    let ntriples = "<http://example.org/subject> <http://example.org/predicate> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n";

    // Parse the N-Triples
    let reader = Cursor::new(ntriples);
    let prefixes = HashMap::new();

    // Use the parse_ntriples function directly
    let triples = parse_ntriples(reader, &prefixes).unwrap();

    // Verify that we got one triple
    assert_eq!(triples.len(), 1);

    // Verify that the object is a literal with the correct value and datatype
    let triple = &triples[0];
    assert!(triple.object().is_literal());
    if let Some(literal) = triple.object().as_literal() {
        assert_eq!(literal.lexical_form(), "42");
        assert!(literal.datatype().is_some());
        if let Some(datatype) = literal.datatype() {
            assert_eq!(datatype.to_string(), "http://www.w3.org/2001/XMLSchema#integer");
        }
    } else {
        panic!("Expected a literal object");
    }
}

#[test]
fn test_parse_language_literal_object() {
    // Create an N-Triples string with a language-tagged literal object
    let ntriples = "<http://example.org/subject> <http://example.org/predicate> \"Hello\"@en .\n";

    // Parse the N-Triples
    let reader = Cursor::new(ntriples);
    let prefixes = HashMap::new();

    // Use the parse_ntriples function directly
    let triples = parse_ntriples(reader, &prefixes).unwrap();

    // Verify that we got one triple
    assert_eq!(triples.len(), 1);

    // Verify that the object is a literal with the correct value and language tag
    let triple = &triples[0];
    assert!(triple.object().is_literal());
    if let Some(literal) = triple.object().as_literal() {
        assert_eq!(literal.lexical_form(), "Hello");
        assert_eq!(literal.language_tag(), Some("en"));
    } else {
        panic!("Expected a literal object");
    }
}

#[test]
fn test_parse_escaped_literal_object() {
    // Create an N-Triples string with escaped characters in the literal
    let ntriples = "<http://example.org/subject> <http://example.org/predicate> \"Line 1\\nLine 2\\tTabbed\\\"Quoted\\\"\\\\Backslash\" .\n";

    // Parse the N-Triples
    let reader = Cursor::new(ntriples);
    let prefixes = HashMap::new();

    // Use the parse_ntriples function directly
    let triples = parse_ntriples(reader, &prefixes).unwrap();

    // Verify that we got one triple
    assert_eq!(triples.len(), 1);

    // Verify that the object is a literal with the correct value including escaped characters
    let triple = &triples[0];
    assert!(triple.object().is_literal());
    if let Some(literal) = triple.object().as_literal() {
        assert_eq!(literal.lexical_form(), "Line 1\nLine 2\tTabbed\"Quoted\"\\Backslash");
    } else {
        panic!("Expected a literal object");
    }
}

#[test]
fn test_parse_numeric_datatypes() {
    // Test various numeric datatypes
    let test_cases = vec![
        ("<http://example.org/subject> <http://example.org/predicate> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n", 
         "42", "http://www.w3.org/2001/XMLSchema#integer"),
        ("<http://example.org/subject> <http://example.org/predicate> \"3.14\"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n", 
         "3.14", "http://www.w3.org/2001/XMLSchema#decimal"),
        ("<http://example.org/subject> <http://example.org/predicate> \"2.99792458E8\"^^<http://www.w3.org/2001/XMLSchema#double> .\n", 
         "2.99792458E8", "http://www.w3.org/2001/XMLSchema#double"),
        ("<http://example.org/subject> <http://example.org/predicate> \"true\"^^<http://www.w3.org/2001/XMLSchema#boolean> .\n", 
         "true", "http://www.w3.org/2001/XMLSchema#boolean"),
    ];

    for (ntriples, expected_value, expected_datatype) in test_cases {
        // Parse the N-Triples
        let reader = Cursor::new(ntriples);
        let prefixes = HashMap::new();

        // Use the parse_ntriples function directly
        let triples = parse_ntriples(reader, &prefixes).unwrap();

        // Verify that we got one triple
        assert_eq!(triples.len(), 1);

        // Verify that the object is a literal with the correct value and datatype
        let triple = &triples[0];
        assert!(triple.object().is_literal());
        if let Some(literal) = triple.object().as_literal() {
            assert_eq!(literal.lexical_form(), expected_value);
            assert!(literal.datatype().is_some());
            if let Some(datatype) = literal.datatype() {
                assert_eq!(datatype.to_string(), expected_datatype);
            }
        } else {
            panic!("Expected a literal object");
        }
    }
}

#[test]
fn test_parse_language_subtag_literal_object() {
    // Create an N-Triples string with a language-tagged literal object with a region subtag
    let ntriples = "<http://example.org/subject> <http://example.org/predicate> \"Hello\"@en-GB .\n";

    // Parse the N-Triples
    let reader = Cursor::new(ntriples);
    let prefixes = HashMap::new();

    // Use the parse_ntriples function directly
    let triples = parse_ntriples(reader, &prefixes).unwrap();

    // Verify that we got one triple
    assert_eq!(triples.len(), 1);

    // Verify that the object is a literal with the correct value and language tag
    let triple = &triples[0];
    assert!(triple.object().is_literal());
    if let Some(literal) = triple.object().as_literal() {
        assert_eq!(literal.lexical_form(), "Hello");
        assert_eq!(literal.language_tag(), Some("en-gb"));
    } else {
        panic!("Expected a literal object");
    }
}
