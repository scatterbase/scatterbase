//! URI module for Scatterbase
//! 
//! This module provides the fundamental URI data structure for Scatterbase,
//! a URI-driven data store.

use lazy_static::lazy_static;
use oxrdf::NamedNode;

/// Maximum length for a URI in bytes
/// RFC 7230 suggests a practical limit of 8000 bytes
pub const MAX_URI_LENGTH: usize = 8000;

/// String constants for URI values
mod uri_strings {
    /// Standard RDF and XML Schema URI string constants
    pub mod rdf {
        /// RDF type predicate
        pub const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        /// RDF subject predicate (for reification)
        pub const SUBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
        /// RDF predicate predicate (for reification)
        pub const PREDICATE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
        /// RDF object predicate (for reification)
        pub const OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
        /// RDF Statement class
        pub const STATEMENT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
        /// RDF 1.2 reifies predicate, linking a reifier to the triple term it reifies
        pub const REIFIES: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
        /// RDF Schema namespace
        pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
        /// XML Schema namespace
        pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
        /// XML Schema dateTime datatype
        pub const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
        /// XML Schema integer datatype
        pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
        /// XML Schema unsignedLong datatype
        pub const XSD_UNSIGNED_LONG: &str = "http://www.w3.org/2001/XMLSchema#unsignedLong";
        /// OWL namespace
        pub const OWL: &str = "http://www.w3.org/2002/07/owl#";
    }

    /// PROV ontology URI string constants
    pub mod prov {
        /// PROV namespace
        pub const NS: &str = "http://www.w3.org/ns/prov#";
        /// PROV wasAttributedTo predicate
        pub const WAS_ATTRIBUTED_TO: &str = "http://www.w3.org/ns/prov#wasAttributedTo";
        /// PROV generatedAtTime predicate
        pub const GENERATED_AT_TIME: &str = "http://www.w3.org/ns/prov#generatedAtTime";
    }

    /// Common vocabulary URI string constants
    pub mod vocab {
        /// FOAF vocabulary namespace
        pub const FOAF: &str = "http://xmlns.com/foaf/0.1/";
        /// Dublin Core elements namespace
        pub const DC: &str = "http://purl.org/dc/elements/1.1/";
        /// Schema.org namespace
        pub const SCHEMA: &str = "http://schema.org/";
    }

    /// Scatterbase project-specific URI string constants
    pub mod scatter {
        /// Base URI for Scatterbase terms
        pub const TERMS_BASE: &str = "https://scatter.red/terms/v0/";
        /// Triple type URI
        pub const TRIPLE: &str = "https://scatter.red/terms/v0/Triple";
        /// ClaimTriple type URI
        pub const CLAIM_TRIPLE: &str = "https://scatter.red/terms/v0/ClaimTriple";
        /// SignedClaim type URI
        pub const SIGNED_CLAIM: &str = "https://scatter.red/terms/v0/SignedClaim";
        /// claimedIn predicate URI
        pub const CLAIMED_IN: &str = "https://scatter.red/terms/v0/claimedIn";
        /// includes predicate URI
        pub const INCLUDES: &str = "https://scatter.red/terms/v0/includes";
        /// previousClaim predicate URI
        pub const PREVIOUS_CLAIM: &str = "https://scatter.red/terms/v0/previousClaim";
        /// userSignature predicate URI
        pub const USER_SIGNATURE: &str = "https://scatter.red/terms/v0/userSignature";
        /// serverSignature predicate URI
        pub const SERVER_SIGNATURE: &str = "https://scatter.red/terms/v0/serverSignature";
        /// seqID predicate URI
        pub const SEQ_ID: &str = "https://scatter.red/terms/v0/seqID";
        /// term predicate URI
        pub const TERM: &str = "https://scatter.red/terms/v0/term";
        /// priorClaim predicate URI
        pub const PRIOR_CLAIM: &str = "https://scatter.red/terms/v0/priorClaim";
        /// acknowledgesPriorUse predicate URI
        pub const ACKNOWLEDGES_PRIOR_USE: &str = "https://scatter.red/terms/v0/acknowledgesPriorUse";
        /// hasPublicKey predicate URI
        pub const HAS_PUBLIC_KEY: &str = "https://scatter.red/terms/v0/hasPublicKey";
        /// PublicKey type URI
        pub const PUBLIC_KEY: &str = "https://scatter.red/terms/v0/PublicKey";
        /// keyType predicate URI
        pub const KEY_TYPE: &str = "https://scatter.red/terms/v0/keyType";
        /// ed25519 key type URI
        pub const ED25519: &str = "https://scatter.red/terms/v0/ed25519";
        /// owner predicate URI
        pub const OWNER: &str = "https://scatter.red/terms/v0/owner";
    }
}

/// Standard RDF and XML Schema URI constants
pub mod rdf {
    use super::*;
    use super::uri_strings::rdf as rdf_str;

    // String constants for backward compatibility
    /// RDF type predicate
    pub const TYPE: &str = rdf_str::TYPE;
    /// RDF subject predicate (for reification)
    pub const SUBJECT: &str = rdf_str::SUBJECT;
    /// RDF predicate predicate (for reification)
    pub const PREDICATE: &str = rdf_str::PREDICATE;
    /// RDF object predicate (for reification)
    pub const OBJECT: &str = rdf_str::OBJECT;
    /// RDF Statement class
    pub const STATEMENT: &str = rdf_str::STATEMENT;
    /// RDF 1.2 reifies predicate
    pub const REIFIES: &str = rdf_str::REIFIES;
    /// RDF Schema namespace
    pub const RDFS: &str = rdf_str::RDFS;
    /// XML Schema namespace
    pub const XSD: &str = rdf_str::XSD;
    /// XML Schema dateTime datatype
    pub const XSD_DATETIME: &str = rdf_str::XSD_DATETIME;
    /// XML Schema integer datatype
    pub const XSD_INTEGER: &str = rdf_str::XSD_INTEGER;
    /// XML Schema unsignedLong datatype
    pub const XSD_UNSIGNED_LONG: &str = rdf_str::XSD_UNSIGNED_LONG;
    /// OWL namespace
    pub const OWL: &str = rdf_str::OWL;

    // NamedNode objects
    lazy_static! {
        /// RDF type predicate as NamedNode
        pub static ref TYPE_URI: NamedNode = NamedNode::new(TYPE).unwrap();
        /// RDF subject predicate (for reification) as NamedNode
        pub static ref SUBJECT_URI: NamedNode = NamedNode::new(SUBJECT).unwrap();
        /// RDF predicate predicate (for reification) as NamedNode
        pub static ref PREDICATE_URI: NamedNode = NamedNode::new(PREDICATE).unwrap();
        /// RDF object predicate (for reification) as NamedNode
        pub static ref OBJECT_URI: NamedNode = NamedNode::new(OBJECT).unwrap();
        /// RDF Statement class as NamedNode
        pub static ref STATEMENT_URI: NamedNode = NamedNode::new(STATEMENT).unwrap();
        /// RDF Schema namespace as NamedNode
        pub static ref RDFS_URI: NamedNode = NamedNode::new(RDFS).unwrap();
        /// XML Schema namespace as NamedNode
        pub static ref XSD_URI: NamedNode = NamedNode::new(XSD).unwrap();
        /// XML Schema dateTime datatype as NamedNode
        pub static ref XSD_DATETIME_URI: NamedNode = NamedNode::new(XSD_DATETIME).unwrap();
        /// XML Schema integer datatype as NamedNode
        pub static ref XSD_INTEGER_URI: NamedNode = NamedNode::new(XSD_INTEGER).unwrap();
        /// XML Schema unsignedLong datatype as NamedNode
        pub static ref XSD_UNSIGNED_LONG_URI: NamedNode = NamedNode::new(XSD_UNSIGNED_LONG).unwrap();
        /// OWL namespace as NamedNode
        pub static ref OWL_URI: NamedNode = NamedNode::new(OWL).unwrap();
    }
}

/// PROV ontology URI constants
pub mod prov {
    use super::*;
    use super::uri_strings::prov as prov_str;

    // String constants for backward compatibility
    /// PROV namespace
    pub const NS: &str = prov_str::NS;
    /// PROV wasAttributedTo predicate
    pub const WAS_ATTRIBUTED_TO: &str = prov_str::WAS_ATTRIBUTED_TO;
    /// PROV generatedAtTime predicate
    pub const GENERATED_AT_TIME: &str = prov_str::GENERATED_AT_TIME;

    // NamedNode objects
    lazy_static! {
        /// PROV namespace as NamedNode
        pub static ref NS_URI: NamedNode = NamedNode::new(NS).unwrap();
        /// PROV wasAttributedTo predicate as NamedNode
        pub static ref WAS_ATTRIBUTED_TO_URI: NamedNode = NamedNode::new(WAS_ATTRIBUTED_TO).unwrap();
        /// PROV generatedAtTime predicate as NamedNode
        pub static ref GENERATED_AT_TIME_URI: NamedNode = NamedNode::new(GENERATED_AT_TIME).unwrap();
    }
}

/// Common vocabulary URI constants
pub mod vocab {
    use super::*;
    use super::uri_strings::vocab as vocab_str;

    // String constants for backward compatibility
    /// FOAF vocabulary namespace
    pub const FOAF: &str = vocab_str::FOAF;
    /// Dublin Core elements namespace
    pub const DC: &str = vocab_str::DC;
    /// Schema.org namespace
    pub const SCHEMA: &str = vocab_str::SCHEMA;

    // NamedNode objects
    lazy_static! {
        /// FOAF vocabulary namespace as NamedNode
        pub static ref FOAF_URI: NamedNode = NamedNode::new(FOAF).unwrap();
        /// Dublin Core elements namespace as NamedNode
        pub static ref DC_URI: NamedNode = NamedNode::new(DC).unwrap();
        /// Schema.org namespace as NamedNode
        pub static ref SCHEMA_URI: NamedNode = NamedNode::new(SCHEMA).unwrap();
    }
}

/// Scatterbase project-specific URI constants
pub mod scatter {
    use super::*;
    use super::uri_strings::scatter as scatter_str;

    // String constants for backward compatibility
    /// Base URI for Scatterbase terms
    pub const TERMS_BASE: &str = scatter_str::TERMS_BASE;
    /// Triple type URI
    pub const TRIPLE: &str = scatter_str::TRIPLE;
    /// ClaimTriple type URI
    pub const CLAIM_TRIPLE: &str = scatter_str::CLAIM_TRIPLE;
    /// SignedClaim type URI
    pub const SIGNED_CLAIM: &str = scatter_str::SIGNED_CLAIM;
    /// claimedIn predicate URI
    pub const CLAIMED_IN: &str = scatter_str::CLAIMED_IN;
    /// includes predicate URI
    pub const INCLUDES: &str = scatter_str::INCLUDES;
    /// previousClaim predicate URI
    pub const PREVIOUS_CLAIM: &str = scatter_str::PREVIOUS_CLAIM;
    /// userSignature predicate URI
    pub const USER_SIGNATURE: &str = scatter_str::USER_SIGNATURE;
    /// serverSignature predicate URI
    pub const SERVER_SIGNATURE: &str = scatter_str::SERVER_SIGNATURE;
    /// seqID predicate URI
    pub const SEQ_ID: &str = scatter_str::SEQ_ID;
    /// term predicate URI
    pub const TERM: &str = scatter_str::TERM;
    /// priorClaim predicate URI
    pub const PRIOR_CLAIM: &str = scatter_str::PRIOR_CLAIM;
    /// acknowledgesPriorUse predicate URI
    pub const ACKNOWLEDGES_PRIOR_USE: &str = scatter_str::ACKNOWLEDGES_PRIOR_USE;

    // NamedNode objects
    lazy_static! {
        /// Base URI for Scatterbase terms as NamedNode
        pub static ref TERMS_BASE_URI: NamedNode = NamedNode::new(TERMS_BASE).unwrap();
        /// Triple type URI as NamedNode
        pub static ref TRIPLE_URI: NamedNode = NamedNode::new(TRIPLE).unwrap();
        /// ClaimTriple type URI as NamedNode
        pub static ref CLAIM_TRIPLE_URI: NamedNode = NamedNode::new(CLAIM_TRIPLE).unwrap();
        /// SignedClaim type URI as NamedNode
        pub static ref SIGNED_CLAIM_URI: NamedNode = NamedNode::new(SIGNED_CLAIM).unwrap();
        /// claimedIn predicate URI as NamedNode
        pub static ref CLAIMED_IN_URI: NamedNode = NamedNode::new(CLAIMED_IN).unwrap();
        /// includes predicate URI as NamedNode
        pub static ref INCLUDES_URI: NamedNode = NamedNode::new(INCLUDES).unwrap();
        /// previousClaim predicate URI as NamedNode
        pub static ref PREVIOUS_CLAIM_URI: NamedNode = NamedNode::new(PREVIOUS_CLAIM).unwrap();
        /// userSignature predicate URI as NamedNode
        pub static ref USER_SIGNATURE_URI: NamedNode = NamedNode::new(USER_SIGNATURE).unwrap();
        /// serverSignature predicate URI as NamedNode
        pub static ref SERVER_SIGNATURE_URI: NamedNode = NamedNode::new(SERVER_SIGNATURE).unwrap();
        /// seqID predicate URI as NamedNode
        pub static ref SEQ_ID_URI: NamedNode = NamedNode::new(SEQ_ID).unwrap();
        /// term predicate URI as NamedNode
        pub static ref TERM_URI: NamedNode = NamedNode::new(TERM).unwrap();
        /// priorClaim predicate URI as NamedNode
        pub static ref PRIOR_CLAIM_URI: NamedNode = NamedNode::new(PRIOR_CLAIM).unwrap();
        /// acknowledgesPriorUse predicate URI as NamedNode
        pub static ref ACKNOWLEDGES_PRIOR_USE_URI: NamedNode = NamedNode::new(ACKNOWLEDGES_PRIOR_USE).unwrap();
        /// hasPublicKey predicate URI as NamedNode
        pub static ref HAS_PUBLIC_KEY_URI: NamedNode = NamedNode::new(uri_strings::scatter::HAS_PUBLIC_KEY).unwrap();
        /// publicKey type URI as NamedNode
        pub static ref PUBLIC_KEY_URI: NamedNode = NamedNode::new(uri_strings::scatter::PUBLIC_KEY).unwrap();
        /// keyType predicate URI as NamedNode
        pub static ref KEY_TYPE_URI: NamedNode = NamedNode::new(uri_strings::scatter::KEY_TYPE).unwrap();
        /// ed25519 key type URI as NamedNode
        pub static ref ED25519_URI: NamedNode = NamedNode::new(uri_strings::scatter::ED25519).unwrap();
        /// owner predicate URI as NamedNode
        pub static ref OWNER_URI: NamedNode = NamedNode::new(uri_strings::scatter::OWNER).unwrap();
    }

    /// Creates a triple hash URI string
    pub fn triple_hash(hash: &str) -> String {
        format!("https://scatter.red/triple/{}", hash)
    }

    /// Creates a claim hash URI string
    pub fn claim_hash(hash: &str) -> String {
        format!("https://scatter.red/claim/{}", hash)
    }

    /// Creates an acknowledgment URI string
    pub fn ack(claim_hash: &str, term_hash: &str) -> String {
        format!("https://scatter.red/ack/{}/term/{}", claim_hash, term_hash)
    }

    /// Creates a triple hash URI object
    pub fn triple_hash_uri(hash: &str) -> NamedNode {
        NamedNode::new(&triple_hash(hash)).unwrap()
    }

    /// Creates a claim hash URI object
    pub fn claim_hash_uri(hash: &str) -> NamedNode {
        NamedNode::new(&claim_hash(hash)).unwrap()
    }

    /// Creates an acknowledgment URI object
    pub fn ack_uri(claim_hash: &str, term_hash: &str) -> NamedNode {
        NamedNode::new(&ack(claim_hash, term_hash)).unwrap()
    }
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
