use std::io::Cursor;
use rio_api::model::Triple as RioTriple;
use rio_api::parser::TriplesParser;
use rio_turtle::NTriplesParser;

#[test]
fn test_rio_literal_parsing() {
    // Create a simple N-Triples string with different types of literals
    let ntriples = r#"
<http://example.org/subject> <http://example.org/predicate> "Simple literal" .
<http://example.org/subject> <http://example.org/predicate> "42"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/subject> <http://example.org/predicate> "Hello"@en .
"#;

    // Parse the N-Triples
    let reader = Cursor::new(ntriples);
    let mut parser = NTriplesParser::new(reader);

    let mut simple_literal_found = false;
    let mut typed_literal_found = false;
    let mut language_literal_found = false;

    parser.parse_all(&mut |rio_triple| {
        match &rio_triple.object {
            rio_api::model::Term::Literal(literal) => {
                match literal {
                    rio_api::model::Literal::Simple { value } => {
                        assert_eq!(value, &"Simple literal");
                        simple_literal_found = true;
                    },
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        assert_eq!(value, &"Hello");
                        assert_eq!(language, &"en");
                        language_literal_found = true;
                    },
                    rio_api::model::Literal::Typed { value, datatype } => {
                        assert_eq!(value, &"42");
                        assert_eq!(datatype.iri, "http://www.w3.org/2001/XMLSchema#integer");
                        typed_literal_found = true;
                    },
                }
            },
            _ => {
                panic!("Expected a literal object");
            }
        }

        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }).expect("Parsing should succeed");

    // Verify that all types of literals were found
    assert!(simple_literal_found, "Simple literal not found");
    assert!(typed_literal_found, "Typed literal not found");
    assert!(language_literal_found, "Language-tagged literal not found");
}
