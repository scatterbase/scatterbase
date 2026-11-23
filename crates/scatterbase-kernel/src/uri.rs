//! URI module for Scatterbase
//! 
//! This module provides the fundamental URI data structure for Scatterbase,
//! a URI-driven data store.

use lazy_static::lazy_static;

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

    // Uri objects
    lazy_static! {
        /// RDF type predicate as Uri
        pub static ref TYPE_URI: Uri = Uri::new(TYPE).unwrap();
        /// RDF subject predicate (for reification) as Uri
        pub static ref SUBJECT_URI: Uri = Uri::new(SUBJECT).unwrap();
        /// RDF predicate predicate (for reification) as Uri
        pub static ref PREDICATE_URI: Uri = Uri::new(PREDICATE).unwrap();
        /// RDF object predicate (for reification) as Uri
        pub static ref OBJECT_URI: Uri = Uri::new(OBJECT).unwrap();
        /// RDF Statement class as Uri
        pub static ref STATEMENT_URI: Uri = Uri::new(STATEMENT).unwrap();
        /// RDF Schema namespace as Uri
        pub static ref RDFS_URI: Uri = Uri::new(RDFS).unwrap();
        /// XML Schema namespace as Uri
        pub static ref XSD_URI: Uri = Uri::new(XSD).unwrap();
        /// XML Schema dateTime datatype as Uri
        pub static ref XSD_DATETIME_URI: Uri = Uri::new(XSD_DATETIME).unwrap();
        /// XML Schema integer datatype as Uri
        pub static ref XSD_INTEGER_URI: Uri = Uri::new(XSD_INTEGER).unwrap();
        /// XML Schema unsignedLong datatype as Uri
        pub static ref XSD_UNSIGNED_LONG_URI: Uri = Uri::new(XSD_UNSIGNED_LONG).unwrap();
        /// OWL namespace as Uri
        pub static ref OWL_URI: Uri = Uri::new(OWL).unwrap();
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

    // Uri objects
    lazy_static! {
        /// PROV namespace as Uri
        pub static ref NS_URI: Uri = Uri::new(NS).unwrap();
        /// PROV wasAttributedTo predicate as Uri
        pub static ref WAS_ATTRIBUTED_TO_URI: Uri = Uri::new(WAS_ATTRIBUTED_TO).unwrap();
        /// PROV generatedAtTime predicate as Uri
        pub static ref GENERATED_AT_TIME_URI: Uri = Uri::new(GENERATED_AT_TIME).unwrap();
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

    // Uri objects
    lazy_static! {
        /// FOAF vocabulary namespace as Uri
        pub static ref FOAF_URI: Uri = Uri::new(FOAF).unwrap();
        /// Dublin Core elements namespace as Uri
        pub static ref DC_URI: Uri = Uri::new(DC).unwrap();
        /// Schema.org namespace as Uri
        pub static ref SCHEMA_URI: Uri = Uri::new(SCHEMA).unwrap();
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

    // Uri objects
    lazy_static! {
        /// Base URI for Scatterbase terms as Uri
        pub static ref TERMS_BASE_URI: Uri = Uri::new(TERMS_BASE).unwrap();
        /// Triple type URI as Uri
        pub static ref TRIPLE_URI: Uri = Uri::new(TRIPLE).unwrap();
        /// ClaimTriple type URI as Uri
        pub static ref CLAIM_TRIPLE_URI: Uri = Uri::new(CLAIM_TRIPLE).unwrap();
        /// SignedClaim type URI as Uri
        pub static ref SIGNED_CLAIM_URI: Uri = Uri::new(SIGNED_CLAIM).unwrap();
        /// claimedIn predicate URI as Uri
        pub static ref CLAIMED_IN_URI: Uri = Uri::new(CLAIMED_IN).unwrap();
        /// includes predicate URI as Uri
        pub static ref INCLUDES_URI: Uri = Uri::new(INCLUDES).unwrap();
        /// previousClaim predicate URI as Uri
        pub static ref PREVIOUS_CLAIM_URI: Uri = Uri::new(PREVIOUS_CLAIM).unwrap();
        /// userSignature predicate URI as Uri
        pub static ref USER_SIGNATURE_URI: Uri = Uri::new(USER_SIGNATURE).unwrap();
        /// serverSignature predicate URI as Uri
        pub static ref SERVER_SIGNATURE_URI: Uri = Uri::new(SERVER_SIGNATURE).unwrap();
        /// seqID predicate URI as Uri
        pub static ref SEQ_ID_URI: Uri = Uri::new(SEQ_ID).unwrap();
        /// term predicate URI as Uri
        pub static ref TERM_URI: Uri = Uri::new(TERM).unwrap();
        /// priorClaim predicate URI as Uri
        pub static ref PRIOR_CLAIM_URI: Uri = Uri::new(PRIOR_CLAIM).unwrap();
        /// acknowledgesPriorUse predicate URI as Uri
        pub static ref ACKNOWLEDGES_PRIOR_USE_URI: Uri = Uri::new(ACKNOWLEDGES_PRIOR_USE).unwrap();
        /// hasPublicKey predicate URI as Uri
        pub static ref HAS_PUBLIC_KEY_URI: Uri = Uri::new(uri_strings::scatter::HAS_PUBLIC_KEY).unwrap();
        /// publicKey type URI as Uri
        pub static ref PUBLIC_KEY_URI: Uri = Uri::new(uri_strings::scatter::PUBLIC_KEY).unwrap();
        /// keyType predicate URI as Uri
        pub static ref KEY_TYPE_URI: Uri = Uri::new(uri_strings::scatter::KEY_TYPE).unwrap();
        /// ed25519 key type URI as Uri
        pub static ref ED25519_URI: Uri = Uri::new(uri_strings::scatter::ED25519).unwrap();
        /// owner predicate URI as Uri
        pub static ref OWNER_URI: Uri = Uri::new(uri_strings::scatter::OWNER).unwrap();
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
    pub fn triple_hash_uri(hash: &str) -> Uri {
        Uri::new(&triple_hash(hash)).unwrap()
    }

    /// Creates a claim hash URI object
    pub fn claim_hash_uri(hash: &str) -> Uri {
        Uri::new(&claim_hash(hash)).unwrap()
    }

    /// Creates an acknowledgment URI object
    pub fn ack_uri(claim_hash: &str, term_hash: &str) -> Uri {
        Uri::new(&ack(claim_hash, term_hash)).unwrap()
    }
}

/// Represents a Uniform Resource Identifier (URI)
/// 
/// A URI is composed of several parts:
/// - scheme (protocol): e.g., "http", "https", "ftp"
/// - authority (domain): e.g., "example.com:8080"
/// - path: e.g., "/users/profile"
/// - query: e.g., "?id=123&name=test"
/// - fragment (anchor): e.g., "#section1"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    /// The full URI string
    raw: String,
    /// The scheme/protocol (e.g., "http", "https", "ftp")
    scheme: Option<String>,
    /// The authority/domain (e.g., "example.com:8080")
    authority: Option<String>,
    /// The path component (e.g., "/users/profile")
    path: Option<String>,
    /// The query component (e.g., "id=123&name=test")
    query: Option<String>,
    /// The fragment/anchor (e.g., "section1")
    fragment: Option<String>,
}

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl Uri {
    /// Creates a new URI from a string
    ///
    /// # Arguments
    ///
    /// * `uri_str` - A string slice that holds the URI
    ///
    /// # Returns
    ///
    /// * `Result<Uri, String>` - A Result containing either the parsed URI or an error message
    pub fn new(uri_str: &str) -> Result<Self, String> {
        if uri_str.len() > MAX_URI_LENGTH {
            return Err(format!("URI exceeds maximum length of {} bytes", MAX_URI_LENGTH));
        }

        if uri_str.is_empty() {
            return Err("URI cannot be empty".to_string());
        }

        // Strip angle brackets if present (they're delimiters, not part of the URI)
        let cleaned_uri_str = if uri_str.starts_with('<') && uri_str.ends_with('>') {
            &uri_str[1..uri_str.len()-1]
        } else {
            uri_str
        };

        // Check for any remaining angle brackets in the URI
        if cleaned_uri_str.contains('<') || cleaned_uri_str.contains('>') {
            return Err("URI cannot contain angle brackets (<, >) as they are reserved for RDF syntax".to_string());
        }

        // Parse the URI components
        let mut scheme = None;
        let mut authority = None;
        let mut path = None;
        let mut query = None;
        let mut fragment = None;

        // Extract scheme
        let parts: Vec<&str> = cleaned_uri_str.splitn(2, "://").collect();
        if parts.len() == 2 {
            scheme = Some(parts[0].to_string());

            // Process the rest
            let rest = parts[1];

            // Extract fragment
            let parts: Vec<&str> = rest.splitn(2, '#').collect();
            let rest = parts[0];
            if parts.len() == 2 {
                fragment = Some(parts[1].to_string());
            }

            // Extract query
            let parts: Vec<&str> = rest.splitn(2, '?').collect();
            let rest = parts[0];
            if parts.len() == 2 {
                query = Some(parts[1].to_string());
            }

            // Extract authority and path
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            if !parts[0].is_empty() {
                authority = Some(parts[0].to_string());
            }

            if parts.len() > 1 {
                path = Some(format!("/{}", parts[1]));
            }
        } else {
            // No scheme, treat as a relative URI
            let parts: Vec<&str> = cleaned_uri_str.splitn(2, '#').collect();
            let rest = parts[0];
            if parts.len() == 2 {
                fragment = Some(parts[1].to_string());
            }

            let parts: Vec<&str> = rest.splitn(2, '?').collect();
            let rest = parts[0];
            if parts.len() == 2 {
                query = Some(parts[1].to_string());
            }

            if !rest.is_empty() {
                path = Some(rest.to_string());
            }
        }

        Ok(Uri {
            raw: cleaned_uri_str.to_string(),
            scheme,
            authority,
            path,
            query,
            fragment,
        })
    }

    /// Returns the full URI as a string
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Returns the scheme/protocol part of the URI
    pub fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }

    /// Returns the authority/domain part of the URI
    pub fn authority(&self) -> Option<&str> {
        self.authority.as_deref()
    }

    /// Returns the path part of the URI
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    /// Returns the query part of the URI
    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// Returns the fragment/anchor part of the URI
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// Returns a string slice with the suffix removed if the URI ends with the suffix
    pub fn strip_suffix(&self, suffix: &str) -> Option<&str> {
        self.raw.strip_suffix(suffix)
    }

    /// Returns a string slice with the prefix removed if the URI starts with the prefix
    pub fn strip_prefix(&self, prefix: &str) -> Option<&str> {
        self.raw.strip_prefix(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_complete_uri() {
        let uri = Uri::new("https://example.com/path/to/resource?query=value#fragment").unwrap();
        assert_eq!(uri.scheme(), Some("https"));
        assert_eq!(uri.authority(), Some("example.com"));
        assert_eq!(uri.path(), Some("/path/to/resource"));
        assert_eq!(uri.query(), Some("query=value"));
        assert_eq!(uri.fragment(), Some("fragment"));
    }

    #[test]
    fn test_parse_uri_without_fragment() {
        let uri = Uri::new("https://example.com/path?query=value").unwrap();
        assert_eq!(uri.scheme(), Some("https"));
        assert_eq!(uri.authority(), Some("example.com"));
        assert_eq!(uri.path(), Some("/path"));
        assert_eq!(uri.query(), Some("query=value"));
        assert_eq!(uri.fragment(), None);
    }

    #[test]
    fn test_parse_uri_without_query() {
        let uri = Uri::new("https://example.com/path#fragment").unwrap();
        assert_eq!(uri.scheme(), Some("https"));
        assert_eq!(uri.authority(), Some("example.com"));
        assert_eq!(uri.path(), Some("/path"));
        assert_eq!(uri.query(), None);
        assert_eq!(uri.fragment(), Some("fragment"));
    }

    #[test]
    fn test_parse_uri_without_path() {
        let uri = Uri::new("https://example.com?query=value#fragment").unwrap();
        assert_eq!(uri.scheme(), Some("https"));
        assert_eq!(uri.authority(), Some("example.com"));
        assert_eq!(uri.path(), None);
        assert_eq!(uri.query(), Some("query=value"));
        assert_eq!(uri.fragment(), Some("fragment"));
    }

    #[test]
    fn test_parse_relative_uri() {
        let uri = Uri::new("/path/to/resource?query=value#fragment").unwrap();
        assert_eq!(uri.scheme(), None);
        assert_eq!(uri.authority(), None);
        assert_eq!(uri.path(), Some("/path/to/resource"));
        assert_eq!(uri.query(), Some("query=value"));
        assert_eq!(uri.fragment(), Some("fragment"));
    }

    #[test]
    fn test_uri_max_length() {
        let long_uri = "a".repeat(MAX_URI_LENGTH + 1);
        let result = Uri::new(&long_uri);
        assert!(result.is_err());
    }

    #[test]
    fn test_uri_with_angle_brackets() {
        // Test that angle brackets are properly stripped
        let uri = Uri::new("<https://example.com/path>").unwrap();
        assert_eq!(uri.as_str(), "https://example.com/path");
        assert_eq!(uri.scheme(), Some("https"));
        assert_eq!(uri.authority(), Some("example.com"));
        assert_eq!(uri.path(), Some("/path"));
    }

    #[test]
    fn test_uri_with_internal_angle_brackets() {
        // Test that URIs with angle brackets in the middle are rejected
        let result = Uri::new("https://example.com/<path>");
        assert!(result.is_err());

        let result = Uri::new("https://example.com/path<invalid>");
        assert!(result.is_err());

        let result = Uri::new("<https://example.com/<path>>");
        assert!(result.is_err());
    }
}
