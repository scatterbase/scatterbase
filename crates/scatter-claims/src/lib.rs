//! Signed claims over RDF triples.
//!
//! A **claim** is a set of triples asserted by one actor, the *attribution*, as the next
//! link in that actor's chain of claims. This crate defines what a claim is, how it is
//! identified, what its signatures cover, and how it is written as RDF. It is pure: no
//! I/O, no clock, no globals. Storage, key lookup and the acceptance policy belong to the
//! caller.
//!
//! - [`id`]: SHA-256 hashes in z-base-32, and the triple hash, claim ID and term hash made
//!   from them.
//! - [`Claim`]: the triples, the attribution and the previous claim; [`Claim::id`] is the
//!   one function that computes a claim ID. [`Claim::from_parsed`] turns triples as a
//!   parser gives them (blank nodes, quoted triples) into the form a claim holds.
//! - [`Envelope`]: a claim, its prior-use acknowledgments and the actor's signature over
//!   both, made by the client.
//! - [`SignedClaim`]: an envelope a server accepted, with the sequence ID and time it
//!   assigned and its signature over them.
//! - [`rdf`]: the envelope as triples, read back from triples, and as N-Triples and Turtle.
//!
//! RDF terms are [`oxrdf`]'s, and the `scatter:` and `prov:` terms are `scatter-vocab`'s.

mod claim;
mod envelope;
pub mod id;
pub mod rdf;

pub use claim::Claim;
pub use envelope::{Acceptance, Acknowledgments, Envelope, Signature, SignedClaim};
pub use rdf::Redacted;
pub use id::{ClaimId, TermHash, TripleHash};

/// Why a claim, an envelope or a signed claim was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimError {
    /// A hash was not 52 characters of z-base-32.
    #[error("not a z-base-32 SHA-256 hash: {0}")]
    BadHash(String),
    /// A claim was given no triples.
    #[error("a claim needs at least one triple")]
    Empty,
    /// A triple holds a term a claim cannot hold.
    #[error("a claim holds IRIs and literals only, not {0}")]
    UnsupportedTerm(String),
    /// An IRI built from the input is not valid.
    #[error("not a valid IRI: {0}")]
    BadIri(String),
    /// A term of the claim has no acknowledgment.
    #[error("no acknowledgment for the term <{0}>")]
    MissingAcknowledgment(String),
    /// An acknowledgment names a term the claim does not use.
    #[error("an acknowledgment for <{0}>, which the claim does not use")]
    UnexpectedAcknowledgment(String),
    /// A signature is not `ed25519:` followed by 64 bytes in z-base-32.
    #[error("not an ed25519 signature: {0}")]
    MalformedSignature(String),
    /// A signature does not verify against the key.
    #[error("the {0} signature does not verify")]
    BadSignature(&'static str),
    /// A value the server assigns holds a line break.
    #[error("a sequence ID or timestamp must be one line")]
    BadAcceptance,
    /// A set of triples is not the RDF form of an envelope or a signed claim.
    #[error("not a claim envelope: {0}")]
    Malformed(String),
}
