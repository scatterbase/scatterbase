use std::io::BufReader;
use std::collections::HashMap;

use oxrdf::{Literal, NamedNode, Term, Triple};

use crate::claim::{hash_triple, parse_turtle};
use crate::tests::test_data::RDF_STAR_TEST_CONTENT;
use crate::uri::{rdf, scatter};

fn iri(s: &str) -> NamedNode {
    NamedNode::new(s).unwrap()
}

#[test]
fn test_parse_rdf_star() {
    let reader = BufReader::new(RDF_STAR_TEST_CONTENT.as_bytes());
    let triples = parse_turtle(reader, &HashMap::new()).expect("Failed to parse Turtle file");

    let alice = iri("http://example.org/alice");
    let bob = iri("http://example.org/bob");
    let person = iri("http://xmlns.com/foaf/0.1/Person");
    let name = iri("http://xmlns.com/foaf/0.1/name");
    let knows = iri("http://xmlns.com/foaf/0.1/knows");
    let rdf_type = iri(rdf::TYPE);

    // The plain triples are parsed as they are written
    let alice_knows_bob = Triple::new(alice.clone(), knows, bob.clone());
    for expected in [
        Triple::new(alice.clone(), rdf_type.clone(), person.clone()),
        Triple::new(alice, name.clone(), Literal::new_simple_literal("Alice")),
        alice_knows_bob.clone(),
        Triple::new(bob.clone(), rdf_type, person),
        Triple::new(bob, name, Literal::new_simple_literal("Bob")),
    ] {
        assert!(triples.contains(&expected), "Missing triple: {}", expected);
    }

    // The quoted triple is replaced by its triple hash IRI, the same IRI the claim
    // envelope gives the triple, and nothing of the reifier is left
    let annotation = Triple::new(
        scatter::triple_hash_uri(&hash_triple(&alice_knows_bob)),
        iri("http://schema.org/startDate"),
        Literal::new_typed_literal("2020-01-01", iri("http://www.w3.org/2001/XMLSchema#date")),
    );
    assert!(triples.contains(&annotation), "Missing triple: {}", annotation);

    // Two annotations on one quoted triple share its triple hash IRI
    let alice_name = scatter::triple_hash_uri(&hash_triple(&Triple::new(
        iri("http://example.org/alice"),
        iri("http://xmlns.com/foaf/0.1/name"),
        Literal::new_simple_literal("Alice"),
    )));
    assert!(triples.contains(&Triple::new(
        alice_name.clone(),
        iri("http://example.org/claimedBy"),
        iri("http://example.org/alice"),
    )));
    assert!(triples.contains(&Triple::new(
        alice_name,
        iri("http://example.org/confidence"),
        Literal::new_typed_literal("0.95", iri("http://www.w3.org/2001/XMLSchema#decimal")),
    )));

    assert_eq!(triples.len(), 8);
    for triple in &triples {
        assert!(triple.subject.is_named_node());
        assert!(triple.predicate.as_str() != rdf::REIFIES);
        assert!(matches!(triple.object, Term::NamedNode(_) | Term::Literal(_)));
    }
}
