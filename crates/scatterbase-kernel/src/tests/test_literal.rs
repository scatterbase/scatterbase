use oxrdf::NamedNode;
use oxrdf::Literal;
use oxrdf::Triple;
use crate::triple::TripleExt;

#[test]
fn test_literal_implementation() {
    // Test simple literal
    let subject = NamedNode::new("https://example.com/subjects/1").unwrap();
    let property = NamedNode::new("https://example.com/properties/name").unwrap();
    let literal = Literal::new_simple_literal("John Doe");
    let triple = Triple::new(subject.clone(), property.clone(), literal);
    assert_eq!(triple.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/name> \"John Doe\" .");

    // Test literal with datatype
    let property2 = NamedNode::new("https://example.com/properties/age").unwrap();
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let literal2 = Literal::new_typed_literal("42", datatype);
    let triple2 = Triple::new(subject.clone(), property2, literal2);
    assert_eq!(triple2.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/age> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .");

    // Test literal with language tag
    let property3 = NamedNode::new("https://example.com/properties/description").unwrap();
    let literal3 = Literal::new_language_tagged_literal_unchecked("A sample description", "en");
    let triple3 = Triple::new(subject.clone(), property3, literal3);
    assert_eq!(triple3.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/description> \"A sample description\"@en .");

    // Test URI object for comparison
    let object = NamedNode::new("https://example.com/objects/John").unwrap();
    let property4 = NamedNode::new("https://example.com/properties/link").unwrap();
    let triple4 = Triple::new(subject.clone(), property4, object);
    assert_eq!(triple4.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/link> <https://example.com/objects/John> .");
}