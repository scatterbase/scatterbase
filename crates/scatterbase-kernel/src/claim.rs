use std::io::{self, BufRead, BufReader};
use std::fs::File;
use std::collections::HashMap;
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
    let mut updated_triples = Vec::new();

    for triple in triples {
        let subject_uri = triple.subject().to_string();
        let object = triple.object();

        // Check if the subject URI contains the placeholder "current"
        let updated_subject = if subject_uri.contains("/.well-known/genid/current/") {
            // Replace "current" with the actual claim hash
            let updated_uri_str = subject_uri.replace("/.well-known/genid/current/", &format!("/.well-known/genid/{}/", claim_hash));
            match Uri::new(&updated_uri_str) {
                Ok(uri) => Term::Uri(uri),
                Err(_) => triple.subject().clone(), // Keep the original if there's an error
            }
        } else {
            triple.subject().clone()
        };

        // Check if the object is a URI and contains the placeholder "current"
        if let Term::Uri(object_uri) = object {
            let object_uri_str = object_uri.to_string();
            if object_uri_str.contains("/.well-known/genid/current/") {
                // Replace "current" with the actual claim hash
                let updated_uri_str = object_uri_str.replace("/.well-known/genid/current/", &format!("/.well-known/genid/{}/", claim_hash));
                match Uri::new(&updated_uri_str) {
                    Ok(uri) => {
                        // Create a new triple with the updated subject and object
                        if let Term::Uri(subject_uri) = &updated_subject {
                            updated_triples.push(Triple::new_uri_object(subject_uri.clone(), triple.property().clone(), uri));
                        } else {
                            updated_triples.push(Triple::new(updated_subject.clone(), triple.property().clone(), Term::Uri(uri)));
                        }
                    },
                    Err(_) => {
                        // Keep the original object if there's an error
                        if let Term::Uri(subject_uri) = &updated_subject {
                            updated_triples.push(Triple::new_uri_object(subject_uri.clone(), triple.property().clone(), object_uri.clone()));
                        } else {
                            updated_triples.push(Triple::new(updated_subject.clone(), triple.property().clone(), Term::Uri(object_uri.clone())));
                        }
                    }
                }
            } else {
                // Object URI doesn't contain the placeholder, just update the subject
                if let Term::Uri(subject_uri) = &updated_subject {
                    updated_triples.push(Triple::new_uri_object(subject_uri.clone(), triple.property().clone(), object_uri.clone()));
                } else {
                    updated_triples.push(Triple::new(updated_subject.clone(), triple.property().clone(), Term::Uri(object_uri.clone())));
                }
            }
        } else {
            // Object is a literal or triple, just update the subject
            updated_triples.push(Triple::new(updated_subject, triple.property().clone(), object.clone()));
        }
    }

    updated_triples
}
use rio_api::parser::TriplesParser;
use rio_turtle::{NTriplesParser, TurtleParser};

use crate::triple::Triple;
use crate::uri::Uri;
use crate::uri::rdf;
use crate::uri::prov;
use crate::uri::scatter;
use crate::literal::Literal;
use crate::term::Term;
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
            Uri::new("https://scatter.red/anonymous").unwrap()
        } else {
            Uri::new(&self.attribution).unwrap()
        };

        // Add the original triples
        for triple in &self.triples {
            result.push(triple.clone());
        }

        // Add the triples according to the required format
        for (i, triple_hash_uri) in triple_hash_uris.iter().enumerate() {
            // <tripleHash> <https://scatter.red/terms/v0/claimedIn> <ClaimHash> .
            let claimed_in_triple = Triple::new_uri_object(triple_hash_uri.clone(), claimed_in_uri.clone(), claim_hash_uri.clone());
            result.push(claimed_in_triple);

            // <tripleHash> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://scatter.red/terms/v0/ClaimTriple> .
            let claim_triple_type_triple = Triple::new_uri_object(triple_hash_uri.clone(), rdf_type_uri.clone(), claim_triple_type_uri.clone());
            result.push(claim_triple_type_triple);

            // <tripleHash> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement> .
            let statement_type_triple = Triple::new_uri_object(triple_hash_uri.clone(), rdf_type_uri.clone(), statement_type_uri.clone());
            result.push(statement_type_triple);

            // Add RDF reification triples for each original triple
            if i < self.triples.len() {
                let triple = &self.triples[i];

                // <tripleHash> rdf:subject <s> .
                let subject_triple = Triple::new(
                    Term::Uri(triple_hash_uri.clone()),
                    rdf::SUBJECT_URI.clone(),
                    triple.subject().clone()
                );
                result.push(subject_triple);

                // <tripleHash> rdf:predicate <p> .
                let predicate_triple = Triple::new_uri_object(
                    triple_hash_uri.clone(),
                    rdf::PREDICATE_URI.clone(),
                    triple.property().clone()
                );
                result.push(predicate_triple);

                // <tripleHash> rdf:object <o> .
                let object_triple = Triple::new(
                    Term::Uri(triple_hash_uri.clone()),
                    rdf::OBJECT_URI.clone(),
                    triple.object().clone()
                );
                result.push(object_triple);
            }
        }

        // <SignedClaimHash> <http://www.w3.org/ns/prov#wasAttributedTo> <User> .
        let was_attributed_to_triple = Triple::new_uri_object(claim_hash_uri.clone(), was_attributed_to_uri.clone(), attribution_uri.clone());
        result.push(was_attributed_to_triple);

        // <SignedClaimHash> <https://scatter.red/terms/v0/previousClaim> <PreviousClaim> .
        let previous_claim_triple = Triple::new_uri_object(claim_hash_uri.clone(), previous_claim_uri.clone(), prev_claim_uri.clone());
        result.push(previous_claim_triple);

        // <SignedClaimHash> <https://scatter.red/terms/v0/userSignature> "ed25519:UserSignatureB32z==" .
        let user_signature_literal = Literal::new(&format!("ed25519:{}", self.signature));
        let user_signature_triple = Triple::new_literal_object(claim_hash_uri.clone(), user_signature_uri.clone(), user_signature_literal);
        result.push(user_signature_triple);

        // Create "ack" triples for each subject and object in the original triples
        for (i, triple) in self.triples.iter().enumerate() {
            // Hash the subject term's canonical string representation
            let subject_term_hash = hash_string(&triple.subject().to_string());

            // Create acknowledgment for subject
            let subject_ack_uri = scatter::ack_uri(&self.claim_hash, &subject_term_hash);

            // Get the prior claim hash for this subject, if available
            let subject_prior_claim_hash = if let Some(uri) = triple.subject().as_uri() {
                let uri_str = uri.to_string();
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
            let subject_term_triple = Triple::new(Term::Uri(subject_ack_uri.clone()), term_uri.clone(), triple.subject().clone());
            result.push(subject_term_triple);

            // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/PriorClaimSubject> .
            let subject_prior_claim_triple = Triple::new_uri_object(subject_ack_uri.clone(), prior_claim_uri.clone(), subject_prior_claim_uri);
            result.push(subject_prior_claim_triple);

            // <https://scatter.red/claim/ClaimHash> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/ClaimHash/term/TermHash> .
            let subject_acknowledges_triple = Triple::new_uri_object(claim_hash_uri.clone(), acknowledges_prior_use_uri.clone(), subject_ack_uri);
            result.push(subject_acknowledges_triple);

            // Create acknowledgment for object if it's a URI
            if let Some(object_uri) = triple.object().as_uri() {
                // Hash the object term's canonical string representation
                let object_term_hash = hash_string(&object_uri.to_string());

                let object_ack_uri = scatter::ack_uri(&self.claim_hash, &object_term_hash);

                // Get the prior claim hash for this object, if available
                let object_uri_str = object_uri.to_string();
                let object_prior_claim_hash = if let Some(claim_hash) = self.uri_to_claim_hash.get(&object_uri_str) {
                    claim_hash.clone()
                } else if let Some(claim_hash) = self.uri_to_claim_hash.get(&format!("<{}>", object_uri_str)) {
                    claim_hash.clone()
                } else {
                    self.object_prev.clone()
                };

                let object_prior_claim_uri = scatter::claim_hash_uri(&object_prior_claim_hash);

                // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/term> <Object> .
                let object_term_triple = Triple::new_uri_object(object_ack_uri.clone(), term_uri.clone(), object_uri.clone());
                result.push(object_term_triple);

                // <https://scatter.red/ack/ClaimHash/term/TermHash> <https://scatter.red/terms/v0/priorClaim> <https://scatter.red/claim/PriorClaimObject> .
                let object_prior_claim_triple = Triple::new_uri_object(object_ack_uri.clone(), prior_claim_uri.clone(), object_prior_claim_uri);
                result.push(object_prior_claim_triple);

                // <https://scatter.red/claim/ClaimHash> <https://scatter.red/terms/v0/acknowledgesPriorUse> <https://scatter.red/ack/ClaimHash/term/TermHash> .
                let object_acknowledges_triple = Triple::new_uri_object(claim_hash_uri.clone(), acknowledges_prior_use_uri.clone(), object_ack_uri);
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
        let triple_cmp = |a: &Triple, b: &Triple| {
            // First compare subjects
            let subject_cmp = a.subject().to_string().cmp(&b.subject().to_string());
            if subject_cmp != std::cmp::Ordering::Equal {
                return subject_cmp;
            }

            // Then compare properties
            let property_cmp = a.property().to_string().cmp(&b.property().to_string());
            if property_cmp != std::cmp::Ordering::Equal {
                return property_cmp;
            }

            // Finally compare objects
            match (a.object(), b.object()) {
                (Term::Uri(a_uri), Term::Uri(b_uri)) => {
                    a_uri.to_string().cmp(&b_uri.to_string())
                },
                (Term::Literal(a_lit), Term::Literal(b_lit)) => {
                    a_lit.to_string().cmp(&b_lit.to_string())
                },
                (Term::Uri(_), Term::Literal(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Literal(_)) => std::cmp::Ordering::Greater,
                (Term::Uri(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Triple(a_triple), Term::Triple(b_triple)) => {
                    a_triple.to_ntriples().cmp(&b_triple.to_ntriples())
                },
            }
        };

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
            let is_user_signature = triple.property().to_string() == scatter::USER_SIGNATURE;
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
        let triple_cmp = |a: &Triple, b: &Triple| {
            // First compare subjects
            let subject_cmp = a.subject().to_string().cmp(&b.subject().to_string());
            if subject_cmp != std::cmp::Ordering::Equal {
                return subject_cmp;
            }

            // Then compare properties
            let property_cmp = a.property().to_string().cmp(&b.property().to_string());
            if property_cmp != std::cmp::Ordering::Equal {
                return property_cmp;
            }

            // Finally compare objects
            match (a.object(), b.object()) {
                (Term::Uri(a_uri), Term::Uri(b_uri)) => {
                    a_uri.to_string().cmp(&b_uri.to_string())
                },
                (Term::Literal(a_lit), Term::Literal(b_lit)) => {
                    a_lit.to_string().cmp(&b_lit.to_string())
                },
                (Term::Uri(_), Term::Literal(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Literal(_)) => std::cmp::Ordering::Greater,
                (Term::Uri(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Triple(a_triple), Term::Triple(b_triple)) => {
                    a_triple.to_ntriples().cmp(&b_triple.to_ntriples())
                },
            }
        };

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
            let is_user_signature = triple.property().to_string() == scatter::USER_SIGNATURE;
            if is_user_signature {
                user_signature_triple = Some(*triple);
            }
            !is_user_signature
        });

        // Add original triples using prefixes
        for triple in &original_triples {
            // For original triples, we want to format the subject as a URI, not as a quoted triple
            let subject = match triple.subject() {
                Term::Uri(uri) => uri_to_prefixed(&uri.to_string(), &prefixes),
                // If the subject is a Triple, we should never get here for original triples
                // But just in case, format it as a quoted triple
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
                _ => triple.subject().to_string(),
            };
            let property = uri_to_prefixed(&triple.property().to_string(), &prefixes);

            let object = match triple.object() {
                Term::Uri(uri) => uri_to_prefixed(&uri.to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        // Add metadata triples using prefixes
        for triple in metadata_triples {
            // For metadata triples, we need to check the type of the subject
            let subject = match triple.subject() {
                Term::Uri(uri) => {
                    // Format all URIs in the subject position of metadata triples as regular URIs
                    let uri_str = uri.to_string();
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
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
                _ => triple.subject().to_string(),
            };
            let property = uri_to_prefixed(&triple.property().to_string(), &prefixes);

            let object = match triple.object() {
                Term::Uri(uri) => uri_to_prefixed(&uri.to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        // Add the user signature triple last, if it exists
        if let Some(triple) = user_signature_triple {
            // Format the user signature triple
            let subject = match triple.subject() {
                Term::Uri(uri) => {
                    let uri_str = uri.to_string();
                    if uri_str.starts_with("https://scatter.red/claim/") {
                        format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                    } else {
                        uri_to_prefixed(&uri_str, &prefixes)
                    }
                },
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
                _ => triple.subject().to_string(),
            };
            let property = uri_to_prefixed(&triple.property().to_string(), &prefixes);

            let object = match triple.object() {
                Term::Uri(uri) => uri_to_prefixed(&uri.to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
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
        let string_datatype = Uri::new("http://www.w3.org/2001/XMLSchema#string").unwrap();
        let sequence_id_literal = Literal::with_datatype(&self.sequence_id, string_datatype);
        let sequence_id_triple = Triple::new_literal_object(claim_hash_uri.clone(), sequence_id_uri, sequence_id_literal);
        server_triples.push(sequence_id_triple);

        // Add the timestamp triple
        let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
        let datatype_datetime = rdf::XSD_DATETIME_URI.clone();
        let timestamp_literal = Literal::with_datatype(&self.timestamp, datatype_datetime);
        let timestamp_triple = Triple::new_literal_object(claim_hash_uri.clone(), generated_at_time_uri, timestamp_literal);
        server_triples.push(timestamp_triple);

        // Create the server signature triple (but don't add it to server_triples yet)
        let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();
        let server_signature_literal = Literal::new(&format!("ed25519:{}", self.server_signature));
        let server_signature_triple = Triple::new_literal_object(claim_hash_uri.clone(), server_signature_uri, server_signature_literal);

        // Sort server triples for consistent ordering
        server_triples.sort_by(|a, b| {
            // First compare subjects
            let subject_cmp = a.subject().to_string().cmp(&b.subject().to_string());
            if subject_cmp != std::cmp::Ordering::Equal {
                return subject_cmp;
            }

            // Then compare properties
            let property_cmp = a.property().to_string().cmp(&b.property().to_string());
            if property_cmp != std::cmp::Ordering::Equal {
                return property_cmp;
            }

            // Finally compare objects
            match (a.object(), b.object()) {
                (Term::Uri(a_uri), Term::Uri(b_uri)) => {
                    a_uri.to_string().cmp(&b_uri.to_string())
                },
                (Term::Literal(a_lit), Term::Literal(b_lit)) => {
                    a_lit.to_string().cmp(&b_lit.to_string())
                },
                (Term::Uri(_), Term::Literal(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Literal(_)) => std::cmp::Ordering::Greater,
                (Term::Uri(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Triple(a_triple), Term::Triple(b_triple)) => {
                    a_triple.to_ntriples().cmp(&b_triple.to_ntriples())
                },
            }
        });

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
            let subject = match triple.subject() {
                Term::Uri(uri) => {
                    let uri_str = uri.to_string();
                    if uri_str.starts_with("https://scatter.red/claim/") {
                        format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                    } else {
                        uri_to_prefixed(&uri_str, &prefixes)
                    }
                },
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
                _ => triple.subject().to_string(),
            };
            let property = uri_to_prefixed(&triple.property().to_string(), &prefixes);

            let object = match triple.object() {
                Term::Uri(uri) => uri_to_prefixed(&uri.to_string(), &prefixes),
                Term::Literal(literal) => literal.to_string(),
                Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
            };

            result.push_str(&format!("{} {} {} .\n", subject, property, object));
        }

        // Add the server signature triple last
        let subject = match server_signature_triple.subject() {
            Term::Uri(uri) => {
                let uri_str = uri.to_string();
                if uri_str.starts_with("https://scatter.red/claim/") {
                    format!("scatter_claim:{}", &uri_str["https://scatter.red/claim/".len()..])
                } else {
                    uri_to_prefixed(&uri_str, &prefixes)
                }
            },
            Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
            _ => server_signature_triple.subject().to_string(),
        };
        let property = uri_to_prefixed(&server_signature_triple.property().to_string(), &prefixes);

        let object = match server_signature_triple.object() {
            Term::Uri(uri) => uri_to_prefixed(&uri.to_string(), &prefixes),
            Term::Literal(literal) => literal.to_string(),
            Term::Triple(triple) => format!("<<{}>>", triple.to_ntriples()),
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
        let string_datatype = Uri::new("http://www.w3.org/2001/XMLSchema#string").unwrap();
        let sequence_id_literal = Literal::with_datatype(&self.sequence_id, string_datatype);
        let sequence_id_triple = Triple::new_literal_object(claim_hash_uri.clone(), sequence_id_uri, sequence_id_literal);
        server_triples.push(sequence_id_triple);

        // Add the timestamp triple
        let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
        let datatype_datetime = rdf::XSD_DATETIME_URI.clone();
        let timestamp_literal = Literal::with_datatype(&self.timestamp, datatype_datetime);
        let timestamp_triple = Triple::new_literal_object(claim_hash_uri.clone(), generated_at_time_uri, timestamp_literal);
        server_triples.push(timestamp_triple);

        // Create the server signature triple (but don't add it to server_triples yet)
        let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();
        let server_signature_literal = Literal::new(&format!("ed25519:{}", self.server_signature));
        let server_signature_triple = Triple::new_literal_object(claim_hash_uri.clone(), server_signature_uri, server_signature_literal);

        // Sort server triples for consistent ordering
        server_triples.sort_by(|a, b| {
            // First compare subjects
            let subject_cmp = a.subject().to_string().cmp(&b.subject().to_string());
            if subject_cmp != std::cmp::Ordering::Equal {
                return subject_cmp;
            }

            // Then compare properties
            let property_cmp = a.property().to_string().cmp(&b.property().to_string());
            if property_cmp != std::cmp::Ordering::Equal {
                return property_cmp;
            }

            // Finally compare objects
            match (a.object(), b.object()) {
                (Term::Uri(a_uri), Term::Uri(b_uri)) => {
                    a_uri.to_string().cmp(&b_uri.to_string())
                },
                (Term::Literal(a_lit), Term::Literal(b_lit)) => {
                    a_lit.to_string().cmp(&b_lit.to_string())
                },
                (Term::Uri(_), Term::Literal(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Uri(_)) => std::cmp::Ordering::Greater,
                (Term::Triple(_), Term::Literal(_)) => std::cmp::Ordering::Greater,
                (Term::Uri(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Literal(_), Term::Triple(_)) => std::cmp::Ordering::Less,
                (Term::Triple(a_triple), Term::Triple(b_triple)) => {
                    a_triple.to_ntriples().cmp(&b_triple.to_ntriples())
                },
            }
        });

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
    let claim_hash_uri = Uri::new(&format!("https://scatter.red/claim/{}", claim_hash)).unwrap();

    // Create server-side triples (sequence ID, timestamp, server signature)
    let seq_id_uri = scatter::SEQ_ID_URI.clone();
    let generated_at_time_uri = prov::GENERATED_AT_TIME_URI.clone();
    let server_signature_uri = scatter::SERVER_SIGNATURE_URI.clone();

    // Create literals for the values
    let seq_id_literal = Literal::new(&sequence_id);
    let timestamp_literal = Literal::new(&timestamp);
    let server_signature_literal = Literal::new(&format!("ed25519:{}", server_signature));

    // Create the triples
    let seq_id_triple = Triple::new_literal_object(
        claim_hash_uri.clone(),
        seq_id_uri,
        seq_id_literal
    );
    let timestamp_triple = Triple::new_literal_object(
        claim_hash_uri.clone(),
        generated_at_time_uri,
        timestamp_literal
    );
    let server_signature_triple = Triple::new_literal_object(
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
fn canonicalize_blank_node(blank_node_id: &str, domain: &str) -> Uri {
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

    Uri::new(&uri_str).unwrap_or_else(|_| {
        // If there's an error creating the URI, use a fallback
        let fallback = format!("{}://{}/{}",
                              protocol, domain_part, format!(".well-known/genid/{}/{}", claim_id, "unknown"));
        Uri::new(&fallback).expect("Failed to create fallback URI for blank node")
    })
}

/// Parses N-Triples format
pub fn parse_ntriples<R: BufRead>(reader: R, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    let mut triples = Vec::new();

    // Get domain from environment for blank node canonicalization
    let domain = env::var("DOMAIN").unwrap_or_else(|_| "https://scatter.red".to_string());

    let mut parser = NTriplesParser::new(reader);

    parser.parse_all(&mut |rio_triple| {
        // Convert RioTriple to our Triple format
        let subject = match rio_triple.subject {
            rio_api::model::Subject::NamedNode(node) => {
                let uri_str = expand_prefix(node.iri, prefixes);
                match Uri::new(&uri_str) {
                    Ok(uri) => uri,
                    Err(e) => {
                        println!("Error parsing subject URI: {}", e);
                        return Ok(());
                    }
                }
            },
            rio_api::model::Subject::BlankNode(node) => {
                // Canonicalize the blank node
                canonicalize_blank_node(node.id, &domain)
            },
            _ => {
                println!("Unknown subject type, not supported");
                return Ok(());
            }
        };

        let property = match Uri::new(rio_triple.predicate.iri) {
            Ok(uri) => uri,
            Err(e) => {
                println!("Error parsing predicate URI: {}", e);
                return Ok(());
            }
        };

        // Check if this is a user signature triple
        let is_user_signature = property.to_string() == "https://scatter.red/terms/v0/userSignature";

        let object = match rio_triple.object {
            rio_api::model::Term::NamedNode(node) => {
                let uri_str = expand_prefix(node.iri, prefixes);
                match Uri::new(&uri_str) {
                    Ok(uri) => uri,
                    Err(e) => {
                        println!("Error parsing object URI: {}", e);
                        return Ok(());
                    }
                }
            },
            rio_api::model::Term::BlankNode(node) => {
                // Canonicalize the blank node
                canonicalize_blank_node(node.id, &domain)
            },
            rio_api::model::Term::Literal(literal) if is_user_signature => {
                // For user signature triples, use the actual signature value
                let literal_obj = match literal {
                    rio_api::model::Literal::Simple { value } => {
                        // Simple literal
                        Literal::new(value)
                    },
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        // Language-tagged literal
                        Literal::with_language(value, language)
                    },
                    rio_api::model::Literal::Typed { value, datatype } => {
                        // Typed literal
                        let datatype_uri = match Uri::new(datatype.iri) {
                            Ok(uri) => uri,
                            Err(e) => {
                                println!("Error parsing datatype URI: {}", e);
                                return Ok(());
                            }
                        };
                        Literal::with_datatype(value, datatype_uri)
                    },
                };
                triples.push(Triple::new_literal_object(subject.clone(), property.clone(), literal_obj));
                return Ok(());
            },
            rio_api::model::Term::Literal(literal) => {
                // Handle regular literals with their actual values
                let literal_obj = match literal {
                    rio_api::model::Literal::Simple { value } => {
                        // Simple literal
                        Literal::new(value)
                    },
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        // Language-tagged literal
                        Literal::with_language(value, language)
                    },
                    rio_api::model::Literal::Typed { value, datatype } => {
                        // Typed literal
                        let datatype_uri = match Uri::new(datatype.iri) {
                            Ok(uri) => uri,
                            Err(e) => {
                                println!("Error parsing datatype URI: {}", e);
                                return Ok(());
                            }
                        };
                        Literal::with_datatype(value, datatype_uri)
                    },
                };
                triples.push(Triple::new_literal_object(subject.clone(), property.clone(), literal_obj));
                return Ok(());
            },
            _ => {
                // Just log a message but continue processing
                println!("Note: Unknown term type in object position, skipping");
                return Ok(());
            }
        };

        triples.push(Triple::new_uri_object(subject, property, object));
        Ok(())
    }).map_err(|e: Box<dyn std::error::Error + Send + Sync>| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(triples)
}

/// Parses Turtle format
pub fn parse_turtle<R: BufRead>(reader: R, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    let mut triples = Vec::new();

    // Get domain from environment for blank node canonicalization
    let domain = env::var("DOMAIN").unwrap_or_else(|_| "https://scatter.red".to_string());

    // Create a TurtleParser with RDF-star support enabled
    // The "generalized" feature should be enabled in Cargo.toml
    let mut parser = TurtleParser::new(reader, None);

    parser.parse_all(&mut |rio_triple| {
        // Convert RioTriple to our Triple format
        let subject = match rio_triple.subject {
            rio_api::model::Subject::NamedNode(node) => {
                let uri_str = expand_prefix(node.iri, prefixes);
                match Uri::new(&uri_str) {
                    Ok(uri) => uri,
                    Err(e) => {
                        println!("Error parsing subject URI: {}", e);
                        return Ok(());
                    }
                }
            },
            rio_api::model::Subject::BlankNode(node) => {
                // Canonicalize the blank node
                canonicalize_blank_node(node.id, &domain)
            },
            rio_api::model::Subject::Triple(triple) => {
                // Handle quoted triple in subject position (RDF-star)
                // Convert the triple to a standard triple hash URI
                let triple_str = triple.to_string();
                let triple_hash = crate::crypto::hash_string(&triple_str);
                let hash_uri_str = format!("https://scatter.red/triple/{}", triple_hash);
                match Uri::new(&hash_uri_str) {
                    Ok(uri) => uri,
                    Err(e) => {
                        println!("Error creating URI for quoted triple in subject position: {}", e);
                        return Ok(());
                    }
                }
            },
            _ => {
                println!("Unknown subject type, not supported");
                return Ok(());
            }
        };

        let property = match Uri::new(rio_triple.predicate.iri) {
            Ok(uri) => uri,
            Err(e) => {
                println!("Error parsing predicate URI: {}", e);
                return Ok(());
            }
        };

        // Check if this is a user signature triple
        let is_user_signature = property.to_string() == "https://scatter.red/terms/v0/userSignature";

        let object = match rio_triple.object {
            rio_api::model::Term::NamedNode(node) => {
                let uri_str = expand_prefix(node.iri, prefixes);
                match Uri::new(&uri_str) {
                    Ok(uri) => uri,
                    Err(e) => {
                        println!("Error parsing object URI: {}", e);
                        return Ok(());
                    }
                }
            },
            rio_api::model::Term::BlankNode(node) => {
                // Canonicalize the blank node
                canonicalize_blank_node(node.id, &domain)
            },
            rio_api::model::Term::Literal(literal) if is_user_signature => {
                // For user signature triples, use the actual signature value
                let literal_obj = match literal {
                    rio_api::model::Literal::Simple { value } => {
                        // Simple literal
                        Literal::new(value)
                    },
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        // Language-tagged literal
                        Literal::with_language(value, language)
                    },
                    rio_api::model::Literal::Typed { value, datatype } => {
                        // Typed literal
                        let datatype_uri = match Uri::new(datatype.iri) {
                            Ok(uri) => uri,
                            Err(e) => {
                                println!("Error parsing datatype URI: {}", e);
                                return Ok(());
                            }
                        };
                        Literal::with_datatype(value, datatype_uri)
                    },
                };
                triples.push(Triple::new_literal_object(subject.clone(), property.clone(), literal_obj));
                return Ok(());
            },
            rio_api::model::Term::Literal(literal) => {
                // Handle regular literals with their actual values
                let literal_obj = match literal {
                    rio_api::model::Literal::Simple { value } => {
                        // Simple literal
                        Literal::new(value)
                    },
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        // Language-tagged literal
                        Literal::with_language(value, language)
                    },
                    rio_api::model::Literal::Typed { value, datatype } => {
                        // Typed literal
                        let datatype_uri = match Uri::new(datatype.iri) {
                            Ok(uri) => uri,
                            Err(e) => {
                                println!("Error parsing datatype URI: {}", e);
                                return Ok(());
                            }
                        };
                        Literal::with_datatype(value, datatype_uri)
                    },
                };
                triples.push(Triple::new_literal_object(subject.clone(), property.clone(), literal_obj));
                return Ok(());
            },
            rio_api::model::Term::Triple(triple) => {
                // Handle quoted triple in object position (RDF-star)
                // Convert the triple to a standard triple hash URI
                let triple_str = triple.to_string();
                let triple_hash = crate::crypto::hash_string(&triple_str);
                let hash_uri_str = format!("https://scatter.red/triple/{}", triple_hash);
                match Uri::new(&hash_uri_str) {
                    Ok(uri) => uri,
                    Err(e) => {
                        println!("Error creating URI for quoted triple in object position: {}", e);
                        return Ok(());
                    }
                }
            },
            _ => {
                // Just log a message but continue processing
                println!("Note: Unknown term type in object position, skipping");
                return Ok(());
            }
        };

        triples.push(Triple::new_uri_object(subject, property, object));
        Ok(())
    }).map_err(|e: Box<dyn std::error::Error + Send + Sync>| io::Error::new(io::ErrorKind::InvalidData, e))?;

    Ok(triples)
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
            assert!(triple.object().is_literal());

            match i {
                0 => {
                    // Simple literal
                    if let Some(literal) = triple.object().as_literal() {
                        assert_eq!(literal.lexical_form(), "Simple literal");
                        assert_eq!(literal.datatype(), None);
                        assert_eq!(literal.language_tag(), None);
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                1 => {
                    // Typed literal
                    if let Some(literal) = triple.object().as_literal() {
                        assert_eq!(literal.lexical_form(), "42");
                        assert!(literal.datatype().is_some());
                        if let Some(datatype) = literal.datatype() {
                            assert_eq!(datatype.to_string(), "http://www.w3.org/2001/XMLSchema#integer");
                        }
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                2 => {
                    // Language-tagged literal
                    if let Some(literal) = triple.object().as_literal() {
                        assert_eq!(literal.lexical_form(), "Hello");
                        assert_eq!(literal.language_tag(), Some("en"));
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
            assert!(triple.object().is_literal());

            match i {
                0 => {
                    // Simple literal
                    if let Some(literal) = triple.object().as_literal() {
                        assert_eq!(literal.lexical_form(), "Simple literal");
                        assert_eq!(literal.datatype(), None);
                        assert_eq!(literal.language_tag(), None);
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                1 => {
                    // Typed literal
                    if let Some(literal) = triple.object().as_literal() {
                        assert_eq!(literal.lexical_form(), "42");
                        assert!(literal.datatype().is_some());
                        if let Some(datatype) = literal.datatype() {
                            assert_eq!(datatype.to_string(), "http://www.w3.org/2001/XMLSchema#integer");
                        }
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                2 => {
                    // Language-tagged literal
                    if let Some(literal) = triple.object().as_literal() {
                        assert_eq!(literal.lexical_form(), "Hello");
                        assert_eq!(literal.language_tag(), Some("en"));
                    } else {
                        panic!("Expected a literal object");
                    }
                },
                _ => panic!("Unexpected triple index"),
            }
        }
    }
}
