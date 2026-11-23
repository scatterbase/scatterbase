//! Hashes and the identifiers made from them.
//!
//! Every hash is SHA-256. In text and in IRIs a hash is written in z-base-32, 52 characters
//! with no padding. Three kinds of hash are identifiers:
//!
//! | Type | Hash of | IRI |
//! |---|---|---|
//! | [`TripleHash`] | the triple's N-Triples line ([`TripleHash::of`]) | `https://scatter.red/triple/{hash}` |
//! | [`ClaimId`] | the claim preimage ([`crate::Claim::id`]) | `https://scatter.red/claim/{hash}` |
//! | [`TermHash`] | the term's N-Triples form ([`TermHash::of`]) | part of an acknowledgment IRI |
//!
//! Each preimage is produced by exactly one function, named in the table.

use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use data_encoding::{Encoding, Specification};
use oxrdf::{NamedNode, NamedNodeRef, Triple};
use sha2::{Digest, Sha256};

use crate::ClaimError;

/// The IRI prefix of a triple hash.
pub const TRIPLE_NS: &str = "https://scatter.red/triple/";
/// The IRI prefix of a claim ID.
pub const CLAIM_NS: &str = "https://scatter.red/claim/";
/// The IRI prefix of a prior-use acknowledgment.
pub const ACK_NS: &str = "https://scatter.red/ack/";

static ZBASE32: LazyLock<Encoding> = LazyLock::new(|| {
    let mut spec = Specification::new();
    spec.symbols.push_str("ybndrfg8ejkmcpqxot1uwisza345h769");
    spec.encoding().expect("the z-base-32 alphabet is a valid encoding")
});

/// Encodes bytes in z-base-32, without padding.
pub fn zbase32(bytes: &[u8]) -> String {
    ZBASE32.encode(bytes)
}

/// Decodes z-base-32. `None` if the text is not z-base-32.
pub fn zbase32_decode(text: &str) -> Option<Vec<u8>> {
    ZBASE32.decode(text.as_bytes()).ok()
}

/// SHA-256 of a byte string.
pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

macro_rules! hash_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            /// The hash from its 32 bytes.
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// The 32 bytes of the hash.
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        /// The hash in z-base-32.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&zbase32(&self.0))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self)
            }
        }

        impl FromStr for $name {
            type Err = ClaimError;

            fn from_str(text: &str) -> Result<Self, ClaimError> {
                zbase32_decode(text)
                    .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                    .map(Self)
                    .ok_or_else(|| ClaimError::BadHash(text.to_string()))
            }
        }
    };
}

hash_type! {
    /// The hash of one triple.
    TripleHash
}

hash_type! {
    /// The identifier of a claim: the hash of its preimage ([`crate::Claim::id`]).
    ClaimId
}

hash_type! {
    /// The hash of one term, which names the term's acknowledgment within a claim.
    TermHash
}

impl TripleHash {
    /// The hash of a triple: SHA-256 of its N-Triples line, `<s> <p> o .` with one space
    /// between the terms and before the dot, and no line ending.
    pub fn of(triple: &Triple) -> Self {
        Self(sha256(ntriples_line(triple).as_bytes()))
    }

    /// The IRI of the triple, `https://scatter.red/triple/{hash}`.
    pub fn iri(&self) -> NamedNode {
        NamedNode::new_unchecked(format!("{TRIPLE_NS}{self}"))
    }

    /// The hash an IRI names, if it is a triple hash IRI.
    pub fn from_iri(iri: NamedNodeRef<'_>) -> Option<Self> {
        iri.as_str().strip_prefix(TRIPLE_NS)?.parse().ok()
    }
}

impl ClaimId {
    /// The claim a first claim follows and a never-used term was last used in: no claim.
    /// It is 32 zero bytes, `yyyy…y` in z-base-32.
    pub const NONE: ClaimId = ClaimId([0; 32]);

    /// The IRI of the claim, `https://scatter.red/claim/{hash}`.
    pub fn iri(&self) -> NamedNode {
        NamedNode::new_unchecked(format!("{CLAIM_NS}{self}"))
    }

    /// The ID an IRI names, if it is a claim IRI.
    pub fn from_iri(iri: NamedNodeRef<'_>) -> Option<Self> {
        iri.as_str().strip_prefix(CLAIM_NS)?.parse().ok()
    }

    /// Whether this is [`ClaimId::NONE`].
    pub fn is_none(&self) -> bool {
        *self == Self::NONE
    }
}

impl TermHash {
    /// The hash of a term: SHA-256 of its N-Triples form, `<iri>` with the angle brackets.
    pub fn of(term: NamedNodeRef<'_>) -> Self {
        Self(sha256(term.to_string().as_bytes()))
    }

    /// The IRI of the acknowledgment of this term in a claim,
    /// `https://scatter.red/ack/{claim}/term/{term hash}`.
    pub fn ack_iri(&self, claim: &ClaimId) -> NamedNode {
        NamedNode::new_unchecked(format!("{ACK_NS}{claim}/term/{self}"))
    }
}

/// A triple as one N-Triples line without its line ending: the preimage of a triple hash.
pub fn ntriples_line(triple: &Triple) -> String {
    format!("{triple} .")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::Literal;

    #[test]
    fn zbase32_round_trips() {
        assert_eq!(zbase32(&[0; 32]), "y".repeat(52));
        assert_eq!(zbase32(b"hello"), "pb1sa5dx");
        assert_eq!(zbase32_decode("pb1sa5dx").unwrap(), b"hello");
        assert_eq!(zbase32_decode("not z-base-32!"), None);
    }

    #[test]
    fn ids_parse_and_print() {
        let id = ClaimId::from_bytes(sha256(b"x"));
        assert_eq!(id.to_string().len(), 52);
        assert_eq!(id.to_string().parse::<ClaimId>().unwrap(), id);
        assert_eq!(ClaimId::from_iri(id.iri().as_ref()), Some(id));
        assert_eq!(ClaimId::from_iri(NamedNodeRef::new_unchecked("https://example.org/x")), None);
        assert!("short".parse::<ClaimId>().is_err());
        assert!(ClaimId::NONE.is_none());
        assert_eq!(ClaimId::NONE.to_string(), "y".repeat(52));
    }

    #[test]
    fn triple_hash_is_of_the_ntriples_line() {
        let triple = Triple::new(
            NamedNode::new_unchecked("http://example.org/a"),
            NamedNode::new_unchecked("http://schema.org/name"),
            Literal::new_simple_literal("A"),
        );
        assert_eq!(ntriples_line(&triple), "<http://example.org/a> <http://schema.org/name> \"A\" .");
        let expected = sha256(b"<http://example.org/a> <http://schema.org/name> \"A\" .");
        assert_eq!(TripleHash::of(&triple).as_bytes(), &expected);
        assert_eq!(TripleHash::from_iri(TripleHash::of(&triple).iri().as_ref()), Some(TripleHash::of(&triple)));
    }

    #[test]
    fn term_hash_is_of_the_bracketed_iri() {
        let term = NamedNodeRef::new_unchecked("http://example.org/a");
        assert_eq!(TermHash::of(term).as_bytes(), &sha256(b"<http://example.org/a>"));
        let claim = ClaimId::from_bytes(sha256(b"claim"));
        assert_eq!(
            TermHash::of(term).ack_iri(&claim).as_str(),
            format!("https://scatter.red/ack/{claim}/term/{}", TermHash::of(term)),
        );
    }
}
