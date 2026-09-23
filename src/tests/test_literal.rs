use crate::uri::Uri;
use crate::literal::Literal;
use crate::triple::Triple;

#[test]
fn test_literal_implementation() {
    // Test simple literal
    let subject = Uri::new("https://example.com/subjects/1").unwrap();
    let property = Uri::new("https://example.com/properties/name").unwrap();
    let literal = Literal::new("John Doe");
    let triple = Triple::new_literal_object(subject.clone(), property.clone(), literal);
    assert_eq!(triple.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/name> \"John Doe\" .");

    // Test literal with datatype
    let property2 = Uri::new("https://example.com/properties/age").unwrap();
    let datatype = Uri::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let literal2 = Literal::with_datatype("42", datatype);
    let triple2 = Triple::new_literal_object(subject.clone(), property2, literal2);
    assert_eq!(triple2.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/age> \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .");

    // Test literal with language tag
    let property3 = Uri::new("https://example.com/properties/description").unwrap();
    let literal3 = Literal::with_language("A sample description", "en");
    let triple3 = Triple::new_literal_object(subject.clone(), property3, literal3);
    assert_eq!(triple3.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/description> \"A sample description\"@en .");

    // Test URI object for comparison
    let object = Uri::new("https://example.com/objects/John").unwrap();
    let property4 = Uri::new("https://example.com/properties/link").unwrap();
    let triple4 = Triple::new_uri_object(subject.clone(), property4, object);
    assert_eq!(triple4.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/link> <https://example.com/objects/John> .");
}