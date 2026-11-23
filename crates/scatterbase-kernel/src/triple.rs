//! The canonical N-Triples line of a triple.
//!
//! Triples are [`oxrdf::Triple`]; this module adds the one serialization the kernel hashes
//! and stores.

use oxrdf::Triple;

/// The N-Triples form of a triple.
pub trait TripleExt {
    /// The triple as one N-Triples line, without the newline: `<s> <p> <o> .`
    ///
    /// This is the preimage of a triple hash, so it must not change.
    fn to_ntriples(&self) -> String;
}

impl TripleExt for Triple {
    fn to_ntriples(&self) -> String {
        format!("{} .", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::{Literal, NamedNode};

    #[test]
    fn test_to_ntriples_with_iri_object() {
        let triple = Triple::new(
            NamedNode::new("https://example.com/subjects/1").unwrap(),
            NamedNode::new("https://example.com/properties/knows").unwrap(),
            NamedNode::new("https://example.com/subjects/2").unwrap(),
        );
        assert_eq!(
            triple.to_ntriples(),
            "<https://example.com/subjects/1> <https://example.com/properties/knows> <https://example.com/subjects/2> ."
        );
    }

    #[test]
    fn test_to_ntriples_with_literal_objects() {
        let s = NamedNode::new("https://example.com/s").unwrap();
        let p = NamedNode::new("https://example.com/p").unwrap();
        let xsd_integer = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
        assert_eq!(
            Triple::new(s.clone(), p.clone(), Literal::new_simple_literal("a \"b\"")).to_ntriples(),
            "<https://example.com/s> <https://example.com/p> \"a \\\"b\\\"\" ."
        );
        assert_eq!(
            Triple::new(s.clone(), p.clone(), Literal::new_typed_literal("42", xsd_integer)).to_ntriples(),
            "<https://example.com/s> <https://example.com/p> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> ."
        );
        assert_eq!(
            Triple::new(s, p, Literal::new_language_tagged_literal_unchecked("Bonjour", "fr")).to_ntriples(),
            "<https://example.com/s> <https://example.com/p> \"Bonjour\"@fr ."
        );
    }
}
