//! IRIs: parsing one that a person wrote, and the vocabulary the kernel uses beyond the
//! claim envelope's own ([`scatter_claims::rdf::vocab`]).

use std::sync::LazyLock;

use oxrdf::NamedNode;

/// Maximum length for a URI in bytes
/// RFC 7230 suggests a practical limit of 8000 bytes
pub const MAX_URI_LENGTH: usize = 8000;

macro_rules! iri {
    ($(#[$doc:meta])* $name:ident = $iri:expr) => {
        $(#[$doc])*
        pub static $name: LazyLock<NamedNode> = LazyLock::new(|| NamedNode::new_unchecked($iri));
    };
}

/// RDF, RDFS, XSD and OWL
pub mod rdf {
    use super::*;

    /// RDF type predicate
    pub const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    /// RDF Schema namespace
    pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
    /// XML Schema namespace
    pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
    /// OWL namespace
    pub const OWL: &str = "http://www.w3.org/2002/07/owl#";

    iri!(/** RDF type predicate */ TYPE_URI = TYPE);
}

/// Namespaces of common vocabularies
pub mod vocab {
    /// FOAF vocabulary namespace
    pub const FOAF: &str = "http://xmlns.com/foaf/0.1/";
    /// Dublin Core elements namespace
    pub const DC: &str = "http://purl.org/dc/elements/1.1/";
    /// Schema.org namespace
    pub const SCHEMA: &str = "http://schema.org/";
}

/// The `scatter:` terms of key registration
pub mod scatter {
    use super::*;

    /// Base URI for Scatterbase terms
    pub const TERMS_BASE: &str = "https://scatter.red/terms/v0/";
    /// hasPublicKey predicate, from an actor to its public key
    pub const HAS_PUBLIC_KEY: &str = "https://scatter.red/terms/v0/hasPublicKey";
    /// PublicKey class
    pub const PUBLIC_KEY: &str = "https://scatter.red/terms/v0/PublicKey";
    /// keyType predicate, from a public key to its algorithm
    pub const KEY_TYPE: &str = "https://scatter.red/terms/v0/keyType";
    /// The Ed25519 key type
    pub const ED25519: &str = "https://scatter.red/terms/v0/ed25519";
    /// owner predicate, from a public key to its actor
    pub const OWNER: &str = "https://scatter.red/terms/v0/owner";
    /// seqID predicate, from a claim to the sequence ID its server assigned
    pub const SEQ_ID: &str = "https://scatter.red/terms/v0/seqID";

    iri!(/** hasPublicKey predicate */ HAS_PUBLIC_KEY_URI = HAS_PUBLIC_KEY);
    iri!(/** PublicKey class */ PUBLIC_KEY_URI = PUBLIC_KEY);
    iri!(/** keyType predicate */ KEY_TYPE_URI = KEY_TYPE);
    iri!(/** The Ed25519 key type */ ED25519_URI = ED25519);
    iri!(/** owner predicate */ OWNER_URI = OWNER);
}

/// Parses an IRI given on the command line or read from a file.
///
/// Angle brackets around the IRI are delimiters and are dropped. The IRI must be absolute
/// and no longer than [`MAX_URI_LENGTH`] bytes.
pub fn parse_iri(iri: &str) -> Result<NamedNode, String> {
    let iri = iri.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')).unwrap_or(iri);
    if iri.len() > MAX_URI_LENGTH {
        return Err(format!("URI exceeds maximum length of {} bytes", MAX_URI_LENGTH));
    }
    NamedNode::new(iri).map_err(|e| format!("Invalid URI '{}': {}", iri, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iri_strips_angle_brackets() {
        assert_eq!(parse_iri("<https://example.com/a>").unwrap().as_str(), "https://example.com/a");
        assert_eq!(parse_iri("https://example.com/a").unwrap().as_str(), "https://example.com/a");
    }

    #[test]
    fn test_parse_iri_rejects_relative_and_malformed() {
        assert!(parse_iri("").is_err());
        assert!(parse_iri("relative/path").is_err());
        assert!(parse_iri("https://example.com/<a>").is_err());
    }

    #[test]
    fn test_parse_iri_max_length() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_URI_LENGTH));
        assert!(parse_iri(&long).is_err());
    }
}
