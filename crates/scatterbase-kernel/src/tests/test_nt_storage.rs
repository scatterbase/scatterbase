use crate::storage::{Storage, MockStorage};
use oxrdf::NamedNode;
use oxrdf::Triple;
use crate::triple::TripleExt;

#[test]
fn test_storage_drops_duplicates() {
    let mut mock_storage = MockStorage::new();
    mock_storage.init("http://test.example.org").expect("Failed to initialize storage");

    let subject = NamedNode::new("http://example.org/subject").unwrap();
    let property = NamedNode::new("http://example.org/property").unwrap();
    let triple = Triple::new(subject.clone(), property.clone(), NamedNode::new("http://example.org/object").unwrap());
    let triple2 = Triple::new(subject, property, NamedNode::new("http://example.org/object2").unwrap());

    // The same triple appended twice is stored once
    mock_storage.append(&triple).expect("Failed to append triple");
    mock_storage.append(&triple).expect("Failed to append triple again");
    mock_storage.append(&triple2).expect("Failed to append second triple");

    assert_eq!(mock_storage.get_triples(), &vec![triple.to_ntriples(), triple2.to_ntriples()]);
    assert!(!mock_storage.claim_exists("yyyy").expect("Failed to check if claim exists"));
}
