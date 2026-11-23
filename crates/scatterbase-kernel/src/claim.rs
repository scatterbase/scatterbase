use std::io::{self, BufRead, BufReader};
use std::fs::File;
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::Mutex;
use std::sync::Once;
use std::cell::RefCell;
use chrono::{Utc, SecondsFormat, DateTime};
// term_command has been merged into term

// Thread-local storage for the current claim hash
thread_local! {
    static CURRENT_CLAIM_HASH: RefCell<String> = RefCell::new(String::from("current"));
}

/// Sets the current claim hash for the thread
/// 
/// # Arguments
/// 
/// * `claim_hash` - The claim hash to set
pub fn set_current_claim_hash(claim_hash: &str) {
    CURRENT_CLAIM_HASH.with(|current| {
        *current.borrow_mut() = claim_hash.to_string();
    });
}

/// Gets the current claim hash for the thread
/// 
/// # Returns
/// 
/// The current claim hash, or "current" if not set
fn get_current_claim_hash() -> String {
    CURRENT_CLAIM_HASH.with(|current| {
        current.borrow().clone()
    })
}

/// Updates blank node URIs in triples to use the actual claim hash
/// 
/// This function takes a vector of triples and a claim hash, and returns a new vector
/// of triples with the blank node URIs updated to use the actual claim hash.
/// 
/// # Arguments
/// 
/// * `triples` - The triples to update
/// * `claim_hash` - The claim hash to use in the URIs
/// 
/// # Returns
/// 
/// A new vector of triples with updated URIs
pub fn update_blank_node_uris(triples: Vec<Triple>, claim_hash: &str) -> Vec<Triple> {
    const PLACEHOLDER: &str = "/.well-known/genid/current/";
    let actual = format!("/.well-known/genid/{}/", claim_hash);

    // Replace "current" with the actual claim hash, keeping the original if that fails
    let update = |node: &NamedNode| {
        if node.as_str().contains(PLACEHOLDER) {
            NamedNode::new(node.as_str().replace(PLACEHOLDER, &actual)).unwrap_or_else(|_| node.clone())
        } else {
            node.clone()
        }
    };

    triples.into_iter().map(|triple| {
        let subject = match &triple.subject {
            NamedOrBlankNode::NamedNode(node) => update(node).into(),
            blank => blank.clone(),
        };
        let object = match &triple.object {
            Term::NamedNode(node) => update(node).into(),
            other => other.clone(),
        };
        Triple::new(subject, triple.predicate, object)
    }).collect()
}

use oxrdf::{BlankNode, NamedOrBlankNode};
use oxttl::{NTriplesParser, TurtleParser};

use oxrdf::Triple;
use crate::triple::TripleExt;
use oxrdf::NamedNode;
use crate::uri::{parse_iri, rdf};
use crate::uri::prov;
use crate::uri::scatter;
use oxrdf::Literal;
use oxrdf::Term;
use crate::term::TermExt;
use crate::crypto::{get_server_signing_key, sign_with_key, hash_string};
use crate::prefix::expand_prefix;
use crate::storage::create_storage;

/// Converts a URI to prefixed form if possible
fn uri_to_prefixed(uri: &str, prefixes: &[(&str, &str)]) -> String {
    for (prefix, namespace) in prefixes {
        if uri.starts_with(namespace) {
            return format!("{}:{}", prefix, &uri[namespace.len()..]);
        }
    }
    format!("<{}>", uri)
}

/// The order triples are written in: by subject, then predicate, then object, with IRI
/// objects before literals.
fn compare_triples(a: &Triple, b: &Triple) -> std::cmp::Ordering {
    fn rank(term: &Term) -> u8 {
        match term {
            Term::NamedNode(_) => 0,
            Term::Literal(_) => 1,
            _ => 2,
        }
    }
    fn key(term: &Term) -> String {
        match term {
            Term::NamedNode(node) => node.as_str().to_string(),
            other => other.to_string(),
        }
    }

    a.subject.to_string().cmp(&b.subject.to_string())
        .then_with(|| a.predicate.as_str().cmp(b.predicate.as_str()))
        .then_with(|| rank(&a.object).cmp(&rank(&b.object)))
        .then_with(|| key(&a.object).cmp(&key(&b.object)))
}

/// Represents a ClaimEnvelope, which is the client-side part of a claim
/// It includes the submitted triples, a "TripleHash claimedIn ClaimHash" for each triple,
/// the claim attribution triple, the "ack" triple acknowledging prior use of each term in the claim,
/// and a triple with an ED25519 signature over the data and metadata triples.
pub struct ClaimEnvelope {
    pub triples: Vec<Triple>,
    pub triple_hashes: Vec<String>,
    pub claim_hash: String,
    pub attribution: String,
    pub prev_claim: String,
    pub signature: String,
    pub domain: String,
    pub server_uri: String,
    pub subject_prev: String,
    pub object_prev: String,
    pub uri_to_claim_hash: HashMap<String, String>,
}

impl ClaimEnvelope {
    /// Converts the ClaimEnvelope to a vector of triples in the required format
    pub fn to_triples(&self) -> Vec<Triple> {
        let mut result = Vec::new();

        // Use URI objects directly
        let claimed_in_uri = scatter::CLAIMED_IN_URI.clone();
        let rdf_type_uri = rdf::TYPE_URI.clone();
        let claim_triple_type_uri = scatter::CLAIM_TRIPLE_URI.clone();
        let statement_type_uri = rdf::STATEMENT_URI.clone();
        let was_attributed_to_uri = prov::WAS_ATTRIBUTED_TO_URI.clone();
        let previous_claim_uri = scatter::PREVIOUS_CLAIM_URI.clone();
        let user_signature_uri = scatter::USER_SIGNATURE_URI.clone();
        let term_uri = scatter::TERM_URI.clone();
        let prior_claim_uri = scatter::PRIOR_CLAIM_URI.clone();
        let acknowledges_prior_use_uri = scatter::ACKNOWLEDGES_PRIOR_USE_URI.clone();

        // Create URI for the claim hash
        let claim_hash_uri = scatter::claim_hash_uri(&self.claim_hash);

        // Create URIs for the triple hashes
        let mut triple_hash_uris = Vec::new();
        for hash in &self.triple_hashes {
            let hash_uri = scatter::triple_hash_uri(hash);
            triple_hash_uris.push(hash_uri);
        }

        // Create URI for the previous claim hash
        let prev_claim_uri = scatter::claim_hash_uri(&self.prev_claim);

        // Create URI for the attribution
        let attribution_uri = if self.attribution.is_empty() {
            // Use a default URI when attribution is empty
            NamedNode::new("https://scatter.red/anonymous").unwrap()
        } else {
            NamedNode::new(&self.attribution).unwrap()
        };

        // Add the original triples
        for triple in &self.triples {
            result.push(triple.clone());
        }

        // Add the triples according to the required format
        for (i, triple_hash_uri) in triple_hash_uris.iter().enumerate() {
            // <tripleHash> <https://scatter.red/terms/v0/claimedIn> <ClaimHash> .
            let claimed_in_triple = Triple::new(triple_hash_uri.clone(), claimed_in_uri.clone(), claim_hash_uri.clone());
            result.push(claimed_in_triple);

            // <tripleHash> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/ClaimTriple> .
            let claim_triple_type_triple = Triple::new(triple_hash_uri.clone(), rdf_type_uri.clone(), claim_triple_type_uri.clone());
            result.push(claim_triple_type_triple);

            // <tripleHash> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement> .
            let statement_type_triple = Triple::new(triple_hash_uri.clone(), rdf_type_uri.clone(), statement_type_uri.clone());
            result.push(statement_type_triple);

            // Add RDF reification triples for each original triple
            if i < self.triples.len() {
                let triple = &self.triples[i];

                // <tripleHash> rdf:subject <s> .
                let subject_triple = Triple::new(triple_hash_uri.clone(),
                    rdf::SUBJECT_URI.clone(),
                    triple.subject.clone()
                );
                result.push(subject_triple);

                // <tripleHash> rdf:predicate <p> .
                let predicate_triple = Triple::new(
                    triple_hash_uri.clone(),
                    rdf::PREDICATE_URI.clone(),
                    triple.predicate.clone()
                );
                result.push(predicate_triple);

                // <tripleHash> rdf:object <o> .
                let object_triple = Triple::new(triple_hash_uri.clone(),
                    rdf::OBJECT_URI.clone(),
                    triple.object.clone()
                );
                result.push(object_triple);
            }
        }

        // <SignedClaimHash> <http://www.w3.org/ns/prov#wasAttributedTo> <User> .
        let was_attributed_to_triple = Triple::new(claim_hash_uri.clone(), was_attributed_to_uri.clone(), attribution_uri.clone());
        result.push(was_attributed_to_triple);

        // <SignedClaimHash> <https://scatter.red/terms/v0/previousClaim> <PreviousClaim> .
        let previous_claim_triple = Triple::new(claim_hash_uri.clone(), previous_claim_uri.clone(), prev_claim_uri.clone());
        result.push(previous_claim_triple);

        // <SignedClaimHash> <https://scatter.red/terms/v0/userSignature> "ed25519:UserSignatureB32z==" .
        let user_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", self.signature));
        let user_signature_triple = Triple::new(claim_hash_uri.clone(), user_signature_uri.clone(), user_signature_literal);
        result.push(user_signature_triple);

        // Create "ack" triples for each subject and object in the original triples
        for (i, triple) in self.triples.iter().enumerate() {
            // Hash the subject term's canonical string representation
            let subject_term_hash = hash_string(&triple.subject.to_string());

            // Create acknowledgment for subject
            let subject_ack_uri = scatter::ack_uri(&self.claim_hash, &subject_term_hash);

            // Get the prior claim hash for this subject, if available
            let subject_prior_claim_hash = if let Some(uri) = triple.subject.as_named_node() {
                let uri_str = uri.as_str().to_string();
                if let Some(claim_hash) = self.uri_to_claim_hash.get(&uri_str) {
                    claim_hash.clone()
                } else if let Some(claim_hash) = self.uri_to_claim_hash.get(&format!("<{}>", uri_str)) {
                    claim_hash.clone()
                } else {
                    self.subject_prev.clone()
                }
            } else {
                self.subject_prev.clone()
            };

            let subject_prior_claim_uri = scatter::claim_hash_uri(&subject_prior_claim_hash);

            // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/term> <Subject> .
            let subject_term_triple = Triple::new(subject_ack_uri.clone(), term_uri.clone(), triple.subject.clone());
            result.push(subject_term_triple);

            // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/PriorClaimSubject> .
            let subject_prior_claim_triple = Triple::new(subject_ack_uri.clone(), prior_claim_uri.clone(), subject_prior_claim_uri);
            result.push(subject_prior_claim_triple);

            // <https://scatter.red/claim/ClaimHash> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/ClaimHash/term/TermHash> .
            let subject_acknowledges_triple = Triple::new(claim_hash_uri.clone(), acknowledges_prior_use_uri.clone(), subject_ack_uri);
            result.push(subject_acknowledges_triple);

            // Create acknowledgment for object if it's a URI
            if let Some(object_uri) = triple.object.as_named_node() {
                // Hash the object term's canonical string representation
                let object_term_hash = hash_string(&object_uri.as_str().to_string());

                let object_ack_uri = scatter::ack_uri(&self.claim_hash, &object_term_hash);

                // Get the prior claim hash for this object, if available
                let object_uri_str = object_uri.as_str().to_string();
                let object_prior_claim_hash = if let Some(claim_hash) = self.uri_to_claim_hash.get(&object_uri_str) {
                    claim_hash.clone()
                } else if let Some(claim_hash) = self.uri_to_claim_hash.get(&format!("<{}>", object_uri_str)) {
                    claim_hash.clone()
                } else {
                    self.object_prev.clone()
                };

                let object_prior_claim_uri = scatter::claim_hash_uri(&object_prior_claim_hash);

                // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/term> <Object> .
                let object_term_triple = Triple::new(object_ack_uri.clone(), term_uri.clone(), object_uri.clone());
                result.push(object_term_triple);

                // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/PriorClaimObject> .
                let object_prior_claim_triple = Triple::new(object_ack_uri.clone(), prior_claim_uri.clone(), object_prior_claim_uri);
                result.push(object_prior_claim_triple);

                // <https://scatter.red/claim/ClaimHash> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/ClaimHash/term/TermHash> .
                let object_acknowledges_triple = Triple::new(claim_hash_uri.clone(), acknowledges_prior_use_uri.clone(), object_ack_uri);
                result.push(object_acknowledges_triple);
            }
        }

        result
    }

    /// Returns the canonical NT representation of the ClaimEnvelope
    pub fn to_ntriples(&self) -> String {
        let mut triples = self.to_triples();
        let mut result = String::new();

        // Remove duplicate triples
        let mut unique_triples = Vec::new();
        for triple in triples {
            if !unique_triples.contains(&triple) {
                unique_triples.push(triple);
            }
        }

        // Define the comparison function for sorting triples
        let triple_cmp = compare_triples;

        // Sort all unique triples for consistent ordering
        unique_triples.sort_by(triple_cmp);

        // Separate original triples from metadata triples
        let mut original_triples: Vec<&Triple> = self.triples.iter().collect();
        let mut metadata_triples: Vec<&Triple> = unique_triples.iter()
            .filter(|t| {
                // Check if the triple is in the original triples
                // We need to compare the triples by value, not by reference
                !self.triples.iter().any(|original| original == *t)
            })
            .collect();

        // Sort original triples alphabetically
        original_triples.sort_by(|a, b| triple_cmp(*a, *b));

        // Find and remove the user signature triple from metadata triples
        let mut user_signature_triple = None;
        metadata_triples.retain(|triple| {
            let is_user_signature = triple.predicate.as_str().to_string() == scatter::USER_SIGNATURE;
            if is_user_signature {
                user_signature_triple = Some(*triple);
            }
            !is_user_signature
        });

        // Add original triples in N-Triples format
        for triple in &original_triples {
            result.push_str(&triple.to_ntriples());
            result.push('\n');
        }

        // Add metadata triples in N-Triples format
        for triple in metadata_triples {
            result.push_str(&triple.to_ntriples());
            result.push('\n');
        }

        // Add the user signature triple last, if it exists
        if let Some(triple) = user_signature_triple {
            result.push_str(&triple.to_ntriples());
            result.push('\n');
        }

        result
    }

    /// Outputs the ClaimEnvelope to stdout in TTL format
    pub fn to_ttl(&self) -> String {
        let mut triples = self.to_triples();
        let mut result = String::new();

        // Remove duplicate triples
        let mut unique_triples = Vec::new();
        for triple in triples {
            if !unique_triples.contains(&triple) {
                unique_triples.push(triple);
            }
        }

        // Define the comparison function for sorting triples
        let triple_cmp = compare_triples;

        // Sort all unique triples for consistent ordering
        unique_triples.sort_by(triple_cmp);

        // Define prefixes
        let prefixes = [
            ("rdf", rdf::TYPE.strip_suffix("type").unwrap_or(rdf::TYPE)),
            ("prov", prov::NS),
            ("scatter", scatter::TERMS_BASE),
            // "scatter_ack" prefix removed as slashes are not legal in CURIEs, using full URIs instead
            ("scatter_triple", "https://scatter.red/triple/"),
            ("scatter_claim", "https://scatter.red/claim/")
        ];

        // Add prefixes
        for (prefix, uri) in &prefixes {
            result.push_str(&format!("@prefix {}: <{}> .\n", prefix, uri));
        }
        result.push_str("\n");

        // Separate original triples from metadata triples
        let mut original_triples: Vec<&Triple> = self.triples.iter().collect();
        let mut metadata_triples: Vec<&Triple> = unique_triples.iter()
            .filter(|t| {
                // Check if the triple is in the original triples
                // We need to compare the triples by value, not by reference
                !self.triples.iter().any(|original| original == *t)
            })
            .collect();

        // Sort original triples alphabetically
        original_triples.sort_by(|a, b| triple_cmp(*a, *b));

        // Find and remove the user signature triple from metadata triples
        let mut user_signature_triple = None;
        metadata_triples.retain(|triple| {
            let is_user_signature = triple.predicate.as_str().to_string() == scatter::USER_SIGNATURE;
            if is_user_signature {
                user_signature_triple = Some(*triple);
            }
            !is_user_signature
        });

        // Add original triples using prefixes
        for triple in &original_triples {
            // For original triples, we want to format the subject as a URI, not as a quoted triple
            let subject = match &triple.subject {
                NamedOrBlankNode::NamedNode(uri) => uri_to_prefixed(&uri.as_str().to_string(), &prefixes),
                NamedOrBlankNode::BlankNode(blank) => blank.to_string(),
            };
            let property = uri_to_prefixed(&triple.predicate.as_str().to_string(), &prefixes);

            let object = match &triple.object {
                Term::NamedNode(uri) => uri_to_prefixed(&uri.as_str().to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                other => other.to_string(),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        // Add metadata triples using prefixes
        for triple in metadata_triples {
            // For metadata triples, we need to check the type of the subject
            let subject = match &triple.subject {
                NamedOrBlankNode::NamedNode(uri) => {
                    // Format all URIs in the subject position of metadata triples as regular URIs
                    let uri_str = uri.as_str().to_string();
                    if uri_str.starts_with("https://scatter.red/ack/") {
                        // Use full URI for acknowledgments as slashes are not legal in CURIEs
                        format!("<{}>", uri_str)
                    } else if uri_str.starts_with("https://scatter.red/triple/") {
                        format!("scatter_triple:{}", &uri_str["https://scatter.red/triple/".len()..])
                    } else if uri_str.starts_with("https://scatter.red/claim/") {
                        format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                    } else {
                        uri_to_prefixed(&uri_str, &prefixes)
                    }
                },
                NamedOrBlankNode::BlankNode(blank) => blank.to_string(),
            };
            let property = uri_to_prefixed(&triple.predicate.as_str().to_string(), &prefixes);

            let object = match &triple.object {
                Term::NamedNode(uri) => uri_to_prefixed(&uri.as_str().to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                other => other.to_string(),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        // Add the user signature triple last, if it exists
        if let Some(triple) = user_signature_triple {
            // Format the user signature triple
            let subject = match &triple.subject {
                NamedOrBlankNode::NamedNode(uri) => {
                    let uri_str = uri.as_str().to_string();
                    if uri_str.starts_with("https://scatter.red/claim/") {
                        format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                    } else {
                        uri_to_prefixed(&uri_str, &prefixes)
                    }
                },
                NamedOrBlankNode::BlankNode(blank) => blank.to_string(),
            };
            let property = uri_to_prefixed(&triple.predicate.as_str().to_string(), &prefixes);

            let object = match &triple.object {
                Term::NamedNode(uri) => uri_to_prefixed(&uri.as_str().to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                other => other.to_string(),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        result
    }
}

/// Represents a SignedClaim, which is the server-side part of a claim
/// It consists of the ClaimEnvelope, the SequenceID, the Timestamp, and the server's ED25519 signature
/// over the submitter's signature and the sequence ID.
pub struct SignedClaim {
    pub envelope: ClaimEnvelope,
    pub sequence_id: String,
    pub timestamp: String,
    pub server_signature: String,
}

impl SignedClaim {
    /// Outputs the SignedClaim to stdout in TTL format with canonical ordering
    /// First prints the ClaimEnvelope, then the server-added triples in alphabetical order
    pub fn to_ttl(&self) -> String {
        let mut result = String::new();

        // First, add the ClaimEnvelope triples
        result.push_str(&self.envelope.to_ttl());

        // Now add the server-added triples
        let mut server_triples = Vec::new();

        // Create the claim hash URI
        let claim_hash_uri = scatter::claim_hash_uri(&self.envelope.claim_hash);

        // Add the sequence ID triple
        let sequence_id_uri = scatter::SEQ_ID_URI.clone();
        let string_datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap();
        let sequence_id_literal = Literal::new_typed_literal(&self.sequence_id, string_datatype);
        let sequence_id_triple = Triple::new(claim_hash_uri.clone(), sequence_id_uri, sequence_id_literal);
        server_triples.push(sequence_id_triple);

        // Add the timestamp triple
        let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
        let datatype_datetime = rdf::XSD_DATETIME_URI.clone();
        let timestamp_literal = Literal::new_typed_literal(&self.timestamp, datatype_datetime);
        let timestamp_triple = Triple::new(claim_hash_uri.clone(), generated_at_time_uri, timestamp_literal);
        server_triples.push(timestamp_triple);

        // Create the server signature triple (but don't add it to server_triples yet)
        let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();
        let server_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", self.server_signature));
        let server_signature_triple = Triple::new(claim_hash_uri.clone(), server_signature_uri, server_signature_literal);

        // Sort server triples for consistent ordering
        server_triples.sort_by(compare_triples);

        // Define prefixes
        let prefixes = [
            ("rdf", rdf::TYPE.strip_suffix("type").unwrap_or(rdf::TYPE)),
            ("prov", prov::NS),
            ("scatter", scatter::TERMS_BASE),
            ("scatter_triple", "https://scatter.red/triple/"),
            ("scatter_claim", "https://scatter.red/claim/")
        ];

        // Add server triples using prefixes
        for triple in &server_triples {
            let subject = match &triple.subject {
                NamedOrBlankNode::NamedNode(uri) => {
                    let uri_str = uri.as_str().to_string();
                    if uri_str.starts_with("https://scatter.red/claim/") {
                        format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                    } else {
                        uri_to_prefixed(&uri_str, &prefixes)
                    }
                },
                NamedOrBlankNode::BlankNode(blank) => blank.to_string(),
            };
            let property = uri_to_prefixed(&triple.predicate.as_str().to_string(), &prefixes);

            let object = match &triple.object {
                Term::NamedNode(uri) => uri_to_prefixed(&uri.as_str().to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                other => other.to_string(),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        // Add the server signature triple last
        let subject = match &server_signature_triple.subject {
            NamedOrBlankNode::NamedNode(uri) => {
                let uri_str = uri.as_str().to_string();
                if uri_str.starts_with("https://scatter.red/claim/") {
                    format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                } else {
                    uri_to_prefixed(&uri_str, &prefixes)
                }
            },
            NamedOrBlankNode::BlankNode(blank) => blank.to_string(),
        };
        let property = uri_to_prefixed(&server_signature_triple.predicate.as_str().to_string(), &prefixes);

        let object = match &server_signature_triple.object {
            Term::NamedNode(uri) => uri_to_prefixed(&uri.as_str().to_string(), &prefixes),
            Term::Literal(literal) => literal.to_string(),
            other => other.to_string(),
        };

        result.push_str(&format!("{} {} {} .\n", subject, property, object));

        result
    }

    /// Returns the canonical NT representation of the SignedClaim
    /// First prints the ClaimEnvelope, then the server-added triples in alphabetical order
    pub fn to_ntriples(&self) -> String {
        let mut result = String::new();

        // First, add the ClaimEnvelope triples
        result.push_str(&self.envelope.to_ntriples());

        // Now add the server-added triples
        let mut server_triples = Vec::new();

        // Create the claim hash URI
        let claim_hash_uri = scatter::claim_hash_uri(&self.envelope.claim_hash);

        // Add the sequence ID triple
        let sequence_id_uri = scatter::SEQ_ID_URI.clone();
        let string_datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#string").unwrap();
        let sequence_id_literal = Literal::new_typed_literal(&self.sequence_id, string_datatype);
        let sequence_id_triple = Triple::new(claim_hash_uri.clone(), sequence_id_uri, sequence_id_literal);
        server_triples.push(sequence_id_triple);

        // Add the timestamp triple
        let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
        let datatype_datetime = rdf::XSD_DATETIME_URI.clone();
        let timestamp_literal = Literal::new_typed_literal(&self.timestamp, datatype_datetime);
        let timestamp_triple = Triple::new(claim_hash_uri.clone(), generated_at_time_uri, timestamp_literal);
        server_triples.push(timestamp_triple);

        // Create the server signature triple (but don't add it to server_triples yet)
        let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();
        let server_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", self.server_signature));
        let server_signature_triple = Triple::new(claim_hash_uri.clone(), server_signature_uri, server_signature_literal);

        // Sort server triples for consistent ordering
        server_triples.sort_by(compare_triples);

        // Add server triples in N-Triples format
        for triple in &server_triples {
            result.push_str(&triple.to_ntriples());
            result.push('\n');
        }

        // Add the server signature triple last
        result.push_str(&server_signature_triple.to_ntriples());
        result.push('\n');

        result
    }
}

// Static variable to keep track of the last used sequence ID
static LAST_SEQUENCE_ID: Mutex<Option<(String, u8)>> = Mutex::new(None);
static INIT: Once = Once::new();

/// Handles the submission of a claim envelope directly
/// 
/// # Arguments
/// 
/// * `envelope` - The claim envelope to submit
/// * `storage_file` - The path to the storage file
/// 
/// # Returns
/// 
/// Result of the operation
pub fn handle_claim_submit_envelope(envelope: ClaimEnvelope, storage_file: &str) -> io::Result<()> {
    // Create a storage instance
    let mut storage = create_storage(storage_file);

    // Initialize the storage with the domain
    if let Err(e) = storage.init(&envelope.domain) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error initializing storage with domain: {}", e)
        ));
    }

    // Extract the claim hash
    let claim_hash = envelope.claim_hash.clone();

    // Check if a claim with this hash already exists
    match storage.claim_exists(&claim_hash) {
        Ok(exists) => {
            if exists {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("A claim with the hash '{}' already exists in the database.", claim_hash)
                ));
            }
        },
        Err(e) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Error checking if claim exists: {}", e)
            ));
        }
    }

    // Generate a timestamp
    let now = Utc::now();
    let timestamp = now.to_rfc3339_opts(SecondsFormat::Millis, true);

    // Generate a sequence ID
    let sequence_id = generate_unique_sequence_id(Some(now));

    // Get the server's signing key
    let server_key = get_server_signing_key();

    // Convert the envelope to triples
    let triples = envelope.to_triples();

    // Store all the triples
    for triple in &triples {
        if let Err(e) = storage.append(triple) {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Error storing triple: {}", e)
            ));
        }
    }

    // Create a string representation of the data to sign
    // This should include the claim envelope (with the user signature), the seqID, and the timestamp
    let data_to_sign = format!("{}\n{}\n{}", 
        triples.iter().map(|t| t.to_ntriples()).collect::<Vec<_>>().join("\n"),
        sequence_id,
        timestamp);

    // Sign the data
    let server_signature = sign_with_key(&server_key, &data_to_sign);

    // Create the claim hash URI
    let claim_hash_uri = NamedNode::new(&format!("https://scatter.red/claim/{}", claim_hash)).unwrap();

    // Create server-side triples (sequence ID, timestamp, server signature)
    let seq_id_uri = scatter::SEQ_ID_URI.clone();
    let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
    let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();

    // Create literals for the values
    let seq_id_literal = Literal::new_simple_literal(&sequence_id);
    let timestamp_literal = Literal::new_simple_literal(&timestamp);
    let server_signature_literal = Literal::new_simple_literal(&format!("ed25519:{}", server_signature));

    // Create the triples
    let seq_id_triple = Triple::new(
        claim_hash_uri.clone(),
        seq_id_uri,
        seq_id_literal
    );
    let timestamp_triple = Triple::new(
        claim_hash_uri.clone(),
        generated_at_time_uri,
        timestamp_literal
    );
    let server_signature_triple = Triple::new(
        claim_hash_uri.clone(),
        server_signature_uri,
        server_signature_literal
    );

    // Store the triples
    if let Err(e) = storage.append(&seq_id_triple) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error storing sequence ID triple: {}", e)
        ));
    }

    if let Err(e) = storage.append(&timestamp_triple) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error storing timestamp triple: {}", e)
        ));
    }

    if let Err(e) = storage.append(&server_signature_triple) {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error storing server signature triple: {}", e)
        ));
    }

    println!("Claim submitted successfully with hash: {}", claim_hash);
    Ok(())
}

/// Generates a unique sequence ID in the format YYYYMMDDHHmmsssss00
/// If two submissions come in at the same millisecond, the second one will be 01, and so on.
/// 
/// # Arguments
///
/// * `timestamp` - Optional DateTime to use for generating the sequence ID. If None, current time is used.
pub fn generate_unique_sequence_id(timestamp: Option<DateTime<Utc>>) -> String {
    // Initialize the mutex once
    INIT.call_once(|| {
        // No initialization needed, just ensuring the mutex is initialized
    });

    // Get the time to use (either provided timestamp or current time)
    let now = timestamp.unwrap_or_else(|| Utc::now());
    let base_sequence_id = now.format("%Y%m%d%H%M%S%3f").to_string();

    // Lock the mutex to access the last sequence ID
    let mut last_sequence_id = LAST_SEQUENCE_ID.lock().unwrap();

    // Check if the base sequence ID is the same as the last one
    let suffix = match *last_sequence_id {
        Some((ref last_base, ref mut last_suffix)) if *last_base == base_sequence_id => {
            // Increment the suffix
            let new_suffix = (*last_suffix + 1) % 100; // Wrap around at 99
            *last_suffix = new_suffix;
            new_suffix
        },
        _ => {
            // New base sequence ID, reset suffix to 0
            *last_sequence_id = Some((base_sequence_id.clone(), 0));
            0
        }
    };

    // Format the sequence ID with the suffix
    format!("{}{:02}", base_sequence_id, suffix)
}

/// Reads triples from a file
pub fn read_triples_from_file(file_path: &str, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    if file_path.ends_with(".ttl") {
        parse_turtle(reader, prefixes)
    } else {
        parse_ntriples(reader, prefixes)
    }
}

/// Reads triples from stdin
pub fn read_triples_from_stdin(prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    let reader = BufReader::new(io::stdin());
    parse_ntriples(reader, prefixes)
}

/// Canonicalizes a blank node identifier to create a globally unique URI
/// 
/// This function takes a blank node identifier and a domain, and returns a URI
/// that is globally unique for that blank node in the context of the domain.
/// 
/// # Arguments
/// 
/// * `blank_node_id` - The blank node identifier (e.g., "abc123")
/// * `domain` - The domain to use for canonicalization (e.g., "https://example.org")
/// 
/// # Returns
/// 
/// A URI that is globally unique for the blank node
fn canonicalize_blank_node(blank_node_id: &str, domain: &str) -> NamedNode {
    // Extract protocol and domain parts
    let protocol_end = domain.find("://").unwrap_or(0);
    let protocol = if protocol_end > 0 {
        &domain[0..protocol_end]
    } else {
        "https" // Default protocol if not found
    };

    let domain_part = if protocol_end > 0 {
        &domain[protocol_end + 3..]
    } else {
        domain
    };

    // Get the current claim hash from thread-local storage
    let claim_id = get_current_claim_hash();

    // Format: {protocol}://{domain}/.well-known/genid/{claim-id}/{local-id}
    let uri_str = format!("{}://{}/{}",
                         protocol, domain_part, format!(".well-known/genid/{}/{}", claim_id, blank_node_id));

    NamedNode::new(&uri_str).unwrap_or_else(|_| {
        // If there's an error creating the URI, use a fallback
        let fallback = format!("{}://{}/{}",
                              protocol, domain_part, format!(".well-known/genid/{}/{}", claim_id, "unknown"));
        NamedNode::new(&fallback).expect("Failed to create fallback URI for blank node")
    })
}

/// Parses N-Triples format
pub fn parse_ntriples<R: BufRead>(reader: R, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    // Get domain from environment for blank node canonicalization
    let domain = env::var("DOMAIN").unwrap_or_else(|_| "https://scatter.red".to_string());

    convert_parsed(NTriplesParser::new().for_reader(reader), prefixes, &domain)
}

/// Parses Turtle format
pub fn parse_turtle<R: BufRead>(reader: R, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    // Get domain from environment for blank node canonicalization
    let domain = env::var("DOMAIN").unwrap_or_else(|_| "https://scatter.red".to_string());

    convert_parsed(TurtleParser::new().for_reader(reader), prefixes, &domain)
}

/// Converts parsed triples to the form the kernel stores: every IRI with its prefix
/// expanded, every blank node replaced by its skolem IRI, and every quoted triple replaced
/// by its triple hash IRI.
///
/// A quoted triple arrives from the parser in its RDF 1.2 form, a reifier with an
/// `rdf:reifies` triple whose object is the triple term. The reifier becomes the triple hash
/// IRI of the quoted triple and the `rdf:reifies` triple itself is dropped, so
/// `<< s p o >> q v` is stored as `<https://scatter.red/triple/{hash}> q v`.
///
/// A triple with a term that cannot be converted is reported and skipped.
fn convert_parsed<E>(
    parsed: impl Iterator<Item = Result<Triple, E>>,
    prefixes: &HashMap<String, String>,
    domain: &str,
) -> io::Result<Vec<Triple>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    let parsed = parsed
        .collect::<Result<Vec<Triple>, E>>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let parsed = label_anonymous_nodes(parsed);

    let mut triples = Vec::new();
    // Reifiers of quoted triples, by blank node ID, and the triple hash IRI of each.
    let mut reifiers: HashMap<String, NamedNode> = HashMap::new();

    for parsed_triple in parsed {

        if parsed_triple.predicate.as_str() == rdf::REIFIES {
            if let (NamedOrBlankNode::BlankNode(reifier), Term::Triple(quoted)) = (&parsed_triple.subject, &parsed_triple.object) {
                match convert_triple(quoted, prefixes, domain, &reifiers) {
                    Ok(quoted) => {
                        reifiers.insert(reifier.as_str().to_string(), scatter::triple_hash_uri(&hash_triple(&quoted)));
                    },
                    Err(e) => println!("Error parsing quoted triple: {}", e),
                }
                continue;
            }
        }

        match convert_triple(&parsed_triple, prefixes, domain, &reifiers) {
            Ok(triple) => triples.push(triple),
            Err(e) => println!("{}", e),
        }
    }

    Ok(triples)
}

/// Gives every blank node that was written without a label (`[ ... ]`, a collection, the
/// reifier of a quoted triple) the label `anon1`, `anon2`, … in order of first appearance.
///
/// The parser labels such nodes at random, and the label becomes part of the node's skolem
/// IRI, so without this the same file would hash differently each time it is read. A
/// number already used as a label in the input is skipped.
fn label_anonymous_nodes(triples: Vec<Triple>) -> Vec<Triple> {
    fn collect<'a>(triple: &'a Triple, written: &mut HashSet<&'a str>) {
        if let NamedOrBlankNode::BlankNode(blank) = &triple.subject {
            if blank.as_ref().unique_id().is_none() {
                written.insert(blank.as_str());
            }
        }
        match &triple.object {
            Term::BlankNode(blank) if blank.as_ref().unique_id().is_none() => {
                written.insert(blank.as_str());
            },
            Term::Triple(quoted) => collect(quoted, written),
            _ => {},
        }
    }

    struct Labels {
        written: HashSet<String>,
        assigned: HashMap<u128, BlankNode>,
        next: u64,
    }

    impl Labels {
        fn label(&mut self, blank: &BlankNode) -> BlankNode {
            let Some(id) = blank.as_ref().unique_id() else {
                return blank.clone();
            };
            if let Some(label) = self.assigned.get(&id) {
                return label.clone();
            }
            let label = loop {
                self.next += 1;
                let candidate = format!("anon{}", self.next);
                if !self.written.contains(&candidate) {
                    break BlankNode::new_unchecked(candidate);
                }
            };
            self.assigned.insert(id, label.clone());
            label
        }

        fn relabel(&mut self, triple: Triple) -> Triple {
            let subject = match triple.subject {
                NamedOrBlankNode::BlankNode(blank) => self.label(&blank).into(),
                named => named,
            };
            let object = match triple.object {
                Term::BlankNode(blank) => self.label(&blank).into(),
                Term::Triple(quoted) => Term::Triple(Box::new(self.relabel(*quoted))),
                other => other,
            };
            Triple::new(subject, triple.predicate, object)
        }
    }

    let mut written = HashSet::new();
    for triple in &triples {
        collect(triple, &mut written);
    }
    let mut labels = Labels {
        written: written.into_iter().map(str::to_string).collect(),
        assigned: HashMap::new(),
        next: 0,
    };
    triples.into_iter().map(|triple| labels.relabel(triple)).collect()
}

/// Converts one parsed triple; see [`convert_parsed`].
fn convert_triple(
    triple: &Triple,
    prefixes: &HashMap<String, String>,
    domain: &str,
    reifiers: &HashMap<String, NamedNode>,
) -> Result<Triple, String> {
    let node = |blank: &BlankNode| match reifiers.get(blank.as_str()) {
        Some(triple_hash_uri) => triple_hash_uri.clone(),
        None => canonicalize_blank_node(blank.as_str(), domain),
    };

    let subject = match &triple.subject {
        NamedOrBlankNode::NamedNode(named) => parse_iri(&expand_prefix(named.as_str(), prefixes))
            .map_err(|e| format!("Error parsing subject URI: {}", e))?,
        NamedOrBlankNode::BlankNode(blank) => node(blank),
    };

    let object: Term = match &triple.object {
        Term::NamedNode(named) => parse_iri(&expand_prefix(named.as_str(), prefixes))
            .map_err(|e| format!("Error parsing object URI: {}", e))?
            .into(),
        Term::BlankNode(blank) => node(blank).into(),
        Term::Literal(literal) => literal.clone().into(),
        Term::Triple(quoted) => {
            let quoted = convert_triple(quoted, prefixes, domain, reifiers)?;
            scatter::triple_hash_uri(&hash_triple(&quoted)).into()
        },
    };

    Ok(Triple::new(subject, triple.predicate.clone(), object))
}

/// Hashes a triple
pub fn hash_triple(triple: &Triple) -> String {
    let triple_str = triple.to_ntriples();
    hash_string(&triple_str)
}

/// Generates a claim hash from triple hashes and metadata
pub fn generate_claim_hash(triple_hashes: &[String], attribution: &str, prev_claim: &str, timestamp: &str, sequence_id: &str) -> String {
    // Sort the triple hashes to ensure deterministic ordering
    let mut sorted_hashes = triple_hashes.to_vec();
    sorted_hashes.sort();

    // Concatenate all the hashes
    let mut combined = String::new();
    for hash in &sorted_hashes {
        combined.push_str(hash);
    }

    // Add the metadata
    combined.push_str(attribution);
    combined.push_str(prev_claim);
    combined.push_str(timestamp);
    combined.push_str(sequence_id);

    // Hash the combined string
    hash_string(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_ntriples_with_literal_objects() {
        // Create a simple N-Triples string with different types of literals
        let ntriples = r#"
<http://example.org/subject> <http://example.org/predicate> "Simple literal" .
<http://example.org/subject> <http://example.org/predicate> "42"^^<http://www.w3.org/2001/XMLSchema#integer> .
<http://example.org/subject> <http://example.org/predicate> "Hello"@en .
"#;

        // Parse the N-Triples
        let reader = Cursor::new(ntriples);
        let prefixes = HashMap::new();

        // Use the parse_ntriples function directly
        let triples = parse_ntriples(reader, &prefixes).unwrap();

        // Verify that we got three triples
        assert_eq!(triples.len(), 3);

        // Verify that all objects are literals with the correct values
        for (i, triple) in triples.iter().enumerate() {
            assert!(triple.object.is_literal());

            match i {
                0 => {
                    // Simple literal
                    if let Some(literal) = triple.object.as_literal() {
                        assert_eq!(literal.value(), "Simple literal");
                        assert_eq!(literal.datatype().as_str(), "http://www.w3.org/2001/XMLSchema#string");
                        assert_eq!(literal.language(), None);
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                1 => {
                    // Typed literal
                    if let Some(literal) = triple.object.as_literal() {
                        assert_eq!(literal.value(), "42");
                        let datatype = literal.datatype();
                        assert_eq!(datatype.as_str().to_string(), "http://www.w3.org/2001/XMLSchema#integer");
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                2 => {
                    // Language-tagged literal
                    if let Some(literal) = triple.object.as_literal() {
                        assert_eq!(literal.value(), "Hello");
                        assert_eq!(literal.language(), Some("en"));
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                _ => panic!("Unexpected triple index"),
            }
        }
    }

    #[test]
    fn test_parse_turtle_with_literal_objects() {
        // Create a simple Turtle string with different types of literals
        let turtle = r#"
@prefix ex: <http://example.org/> .
ex:subject ex:predicate "Simple literal" .
ex:subject ex:predicate "42"^^<http://www.w3.org/2001/XMLSchema#integer> .
ex:subject ex:predicate "Hello"@en .
"#;

        // Parse the Turtle
        let reader = Cursor::new(turtle);
        let prefixes = HashMap::new();

        // Use the parse_turtle function directly
        let triples = parse_turtle(reader, &prefixes).unwrap();

        // Verify that we got three triples
        assert_eq!(triples.len(), 3);

        // Verify that all objects are literals with the correct values
        for (i, triple) in triples.iter().enumerate() {
            assert!(triple.object.is_literal());

            match i {
                0 => {
                    // Simple literal
                    if let Some(literal) = triple.object.as_literal() {
                        assert_eq!(literal.value(), "Simple literal");
                        assert_eq!(literal.datatype().as_str(), "http://www.w3.org/2001/XMLSchema#string");
                        assert_eq!(literal.language(), None);
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                1 => {
                    // Typed literal
                    if let Some(literal) = triple.object.as_literal() {
                        assert_eq!(literal.value(), "42");
                        let datatype = literal.datatype();
                        assert_eq!(datatype.as_str().to_string(), "http://www.w3.org/2001/XMLSchema#integer");
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                2 => {
                    // Language-tagged literal
                    if let Some(literal) = triple.object.as_literal() {
                        assert_eq!(literal.value(), "Hello");
                        assert_eq!(literal.language(), Some("en"));
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                _ => panic!("Unexpected triple index"),
            }
        }
    }
}
