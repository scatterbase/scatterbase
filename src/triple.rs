//! Triple module for Scatterbase
//! 
//! This module provides the Triple data structure for Scatterbase,
//! which is composed of a subject URI, a property URI, and an object Term (URI or Literal).

use std::env;
use dotenv;

use crate::uri::Uri;
use crate::term::Term;
use crate::literal::Literal;
use crate::storage::{Storage, NtFileStorage};

/// Represents a Triple in Scatterbase
/// 
/// A Triple is composed of:
/// - subject: The entity that the triple is about (URI or Triple for RDF-star)
/// - property: The property or relationship being described (URI)
/// - object: The value or entity that is the object of the relationship (URI, Literal, or Triple for RDF-star)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triple {
    /// The subject Term (URI or Triple for RDF-star)
    subject: Term,
    /// The property URI
    property: Uri,
    /// The object Term (URI, Literal, or Triple for RDF-star)
    object: Term,
}

impl Triple {
    /// Creates a new Triple from a subject Term, a property URI, and an object Term
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject Term (URI or Triple for RDF-star)
    /// * `property` - The property URI
    /// * `object` - The object Term (URI, Literal, or Triple for RDF-star)
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new(subject: Term, property: Uri, object: Term) -> Self {
        Triple {
            subject,
            property,
            object,
        }
    }

    /// Creates a new Triple from two URIs and an object Term
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject URI
    /// * `property` - The property URI
    /// * `object` - The object Term (URI, Literal, or Triple for RDF-star)
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new_with_uri_subject(subject: Uri, property: Uri, object: Term) -> Self {
        Triple {
            subject: Term::Uri(subject),
            property,
            object,
        }
    }

    /// Creates a new Triple from a Triple subject, a property URI, and an object Term
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject Triple (for RDF-star)
    /// * `property` - The property URI
    /// * `object` - The object Term (URI, Literal, or Triple for RDF-star)
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new_with_triple_subject(subject: Triple, property: Uri, object: Term) -> Self {
        Triple {
            subject: Term::Triple(Box::new(subject)),
            property,
            object,
        }
    }

    /// Creates a new Triple from three URIs
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject URI
    /// * `property` - The property URI
    /// * `object` - The object URI
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new_uri_object(subject: Uri, property: Uri, object: Uri) -> Self {
        Triple {
            subject: Term::Uri(subject),
            property,
            object: Term::Uri(object),
        }
    }

    /// Creates a new Triple from two URIs and a Literal
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject URI
    /// * `property` - The property URI
    /// * `object` - The object Literal
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new_literal_object(subject: Uri, property: Uri, object: Literal) -> Self {
        Triple {
            subject: Term::Uri(subject),
            property,
            object: Term::Literal(object),
        }
    }

    /// Creates a new Triple from a Triple subject, a property URI, and a URI object
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject Triple (for RDF-star)
    /// * `property` - The property URI
    /// * `object` - The object URI
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new_triple_subject_uri_object(subject: Triple, property: Uri, object: Uri) -> Self {
        Triple {
            subject: Term::Triple(Box::new(subject)),
            property,
            object: Term::Uri(object),
        }
    }

    /// Creates a new Triple from a Triple subject, a property URI, and a Literal object
    ///
    /// # Arguments
    ///
    /// * `subject` - The subject Triple (for RDF-star)
    /// * `property` - The property URI
    /// * `object` - The object Literal
    ///
    /// # Returns
    ///
    /// * `Triple` - A new Triple instance
    pub fn new_triple_subject_literal_object(subject: Triple, property: Uri, object: Literal) -> Self {
        Triple {
            subject: Term::Triple(Box::new(subject)),
            property,
            object: Term::Literal(object),
        }
    }

    /// Returns the subject Term
    pub fn subject(&self) -> &Term {
        &self.subject
    }

    /// Returns the property URI
    pub fn property(&self) -> &Uri {
        &self.property
    }

    /// Returns the object Term
    pub fn object(&self) -> &Term {
        &self.object
    }

    /// Returns the triple in N-Triples format
    pub fn to_ntriples(&self) -> String {
        let subject_str = match &self.subject {
            Term::Uri(uri) => format!("<{}>", uri),
            Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples().trim_start_matches('<').trim_end_matches('>')),
            _ => panic!("Subject must be a URI or a Triple"),
        };

        format!("{} <{}> {} .", subject_str, self.property, self.object.to_ntriples())
    }
}

/// Handles the "triple write" command
pub fn handle_triple_write_command(subject: &str, property: &str, object: &str, env_path: &str) {
    // Load environment variables from .env file
    if let Err(e) = dotenv::from_path(env_path) {
        eprintln!("Error loading .env file at {}: {}", env_path, e);
        eprintln!("Make sure you've initialized Scatterbase with 'scatterbase init' first.");
        return;
    }

    // Get storage file from environment
    let storage_file = match env::var("STORAGE_FILE") {
        Ok(file) => file,
        Err(_) => {
            eprintln!("STORAGE_FILE not set in environment. Run 'scatterbase init' first.");
            return;
        }
    };

    // Create URI objects
    let subject_uri = match Uri::new(subject) {
        Ok(uri) => uri,
        Err(e) => {
            eprintln!("Invalid subject URI: {}", e);
            return;
        }
    };

    let property_uri = match Uri::new(property) {
        Ok(uri) => uri,
        Err(e) => {
            eprintln!("Invalid property URI: {}", e);
            return;
        }
    };

    // Check if object is a literal (enclosed in quotes) or a URI
    let triple = if object.starts_with('"') && object.ends_with('"') {
        // It's a literal - remove the quotes and create a literal object
        let literal_value = &object[1..object.len()-1];
        let literal = Literal::new(literal_value);
        Triple::new_literal_object(subject_uri, property_uri, literal)
    } else {
        // Try to parse as URI
        let object_uri = match Uri::new(object) {
            Ok(uri) => uri,
            Err(e) => {
                eprintln!("Invalid object URI: {}", e);
                return;
            }
        };
        Triple::new_uri_object(subject_uri, property_uri, object_uri)
    };

    // Create a storage instance
    let mut storage = NtFileStorage::new(&storage_file);

    // Store the triple
    if let Err(e) = storage.append(&triple) {
        eprintln!("Error storing triple: {}", e);
        return;
    }

    println!("Triple stored successfully:");
    println!("{}", triple.to_ntriples());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_triple_with_uri_object() {
        let subject = Uri::new("https://example.com/subjects/1").unwrap();
        let property = Uri::new("https://example.com/properties/name").unwrap();
        let object = Uri::new("https://example.com/objects/John").unwrap();

        let triple = Triple::new_uri_object(subject.clone(), property.clone(), object.clone());

        assert_eq!(triple.subject().as_uri(), Some(&subject));
        assert_eq!(triple.property(), &property);
        assert_eq!(triple.object().as_uri(), Some(&object));
        assert_eq!(triple.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/name> <https://example.com/objects/John> .");
    }

    #[test]
    fn test_create_triple_with_literal_object() {
        let subject = Uri::new("https://example.com/subjects/1").unwrap();
        let property = Uri::new("https://example.com/properties/name").unwrap();
        let object = Literal::new("John Doe");

        let triple = Triple::new_literal_object(subject.clone(), property.clone(), object.clone());

        assert_eq!(triple.subject().as_uri(), Some(&subject));
        assert_eq!(triple.property(), &property);
        assert_eq!(triple.object().as_literal(), Some(&object));
        assert_eq!(triple.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/name> \"John Doe\" .");
    }

    #[test]
    fn test_create_triple_with_typed_literal_object() {
        let subject = Uri::new("https://example.com/subjects/1").unwrap();
        let property = Uri::new("https://example.com/properties/age").unwrap();
        let datatype = Uri::new("https://example.com/datatypes/integer").unwrap();
        let object = Literal::with_datatype("42", datatype);

        let triple = Triple::new_literal_object(subject.clone(), property.clone(), object.clone());

        assert_eq!(triple.subject().as_uri(), Some(&subject));
        assert_eq!(triple.property(), &property);
        assert_eq!(triple.object().as_literal(), Some(&object));
        assert_eq!(triple.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/age> \"42\"^^<https://example.com/datatypes/integer> .");
    }

    #[test]
    fn test_create_triple_with_language_literal_object() {
        let subject = Uri::new("https://example.com/subjects/1").unwrap();
        let property = Uri::new("https://example.com/properties/description").unwrap();
        let object = Literal::with_language("A sample description", "en");

        let triple = Triple::new_literal_object(subject.clone(), property.clone(), object.clone());

        assert_eq!(triple.subject().as_uri(), Some(&subject));
        assert_eq!(triple.property(), &property);
        assert_eq!(triple.object().as_literal(), Some(&object));
        assert_eq!(triple.to_ntriples(), "<https://example.com/subjects/1> <https://example.com/properties/description> \"A sample description\"@en .");
    }

    #[test]
    fn test_create_triple_with_triple_subject() {
        let inner_subject = Uri::new("https://example.com/subjects/1").unwrap();
        let inner_property = Uri::new("https://example.com/properties/name").unwrap();
        let inner_object = Literal::new("John Doe");

        let inner_triple = Triple::new_literal_object(inner_subject, inner_property, inner_object);

        let property = Uri::new("https://example.com/properties/confidence").unwrap();
        let object = Literal::new("0.95");

        let triple = Triple::new_triple_subject_literal_object(inner_triple.clone(), property.clone(), object.clone());

        assert!(triple.subject().is_triple());
        assert_eq!(triple.property(), &property);
        assert_eq!(triple.object().as_literal(), Some(&object));
        assert_eq!(triple.to_ntriples(), "<<https://example.com/subjects/1> <https://example.com/properties/name> \"John Doe\" .>> <https://example.com/properties/confidence> \"0.95\" .");
    }
}
