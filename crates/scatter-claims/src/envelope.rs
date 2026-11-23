//! The envelope a client signs and the signed claim a server makes of it.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use oxrdf::NamedNode;

use crate::id::{sha256, zbase32, zbase32_decode, ClaimId};
use crate::{Claim, ClaimError};

/// The first line of the preimage of a client's signature.
const SIGNATURE_TAG: &str = "scatter-claim-signature-v0";
/// The first line of the preimage of a server's signature.
const ACCEPTANCE_TAG: &str = "scatter-claim-acceptance-v0";

/// Prior-use acknowledgments: for each term of a claim, the claim that last used the term
/// as far as the client knew, or [`ClaimId::NONE`] if it knew of none.
pub type Acknowledgments = BTreeMap<NamedNode, ClaimId>;

/// An Ed25519 signature. Its text form is `ed25519:` followed by the 64 bytes in z-base-32.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature([u8; 64]);

impl Signature {
    /// The signature from its 64 bytes.
    pub const fn from_bytes(bytes: [u8; 64]) -> Self {
        Self(bytes)
    }

    /// The 64 bytes of the signature.
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// The SHA-256 of the signature, in z-base-32: what stands for it where the signature
    /// itself may later be erased.
    pub fn digest(&self) -> String {
        zbase32(&sha256(&self.0))
    }

    fn sign(key: &SigningKey, preimage: &str) -> Self {
        Self(key.sign(preimage.as_bytes()).to_bytes())
    }

    fn verify(&self, key: &VerifyingKey, preimage: &str, whose: &'static str) -> Result<(), ClaimError> {
        key.verify(preimage.as_bytes(), &ed25519_dalek::Signature::from_bytes(&self.0))
            .map_err(|_| ClaimError::BadSignature(whose))
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ed25519:{}", zbase32(&self.0))
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({self})")
    }
}

impl FromStr for Signature {
    type Err = ClaimError;

    fn from_str(text: &str) -> Result<Self, ClaimError> {
        text.strip_prefix("ed25519:")
            .and_then(zbase32_decode)
            .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
            .map(Self)
            .ok_or_else(|| ClaimError::MalformedSignature(text.to_string()))
    }
}

/// A claim, the prior use of its terms that the client acknowledges, and the actor's
/// signature over both. This is what a client prepares and submits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    claim: Claim,
    acknowledgments: Acknowledgments,
    signature: Signature,
}

impl Envelope {
    /// Signs a claim and its acknowledgments with the actor's key.
    ///
    /// There must be one acknowledgment for each of the claim's [terms](Claim::terms) and no
    /// others.
    pub fn sign(claim: Claim, acknowledgments: Acknowledgments, key: &SigningKey) -> Result<Self, ClaimError> {
        check_acknowledgments(&claim, &acknowledgments)?;
        let signature = Signature::sign(key, &signature_preimage(&claim, &acknowledgments));
        Ok(Self { claim, acknowledgments, signature })
    }

    /// An envelope from its parts, as when it is read back. The signature is not checked:
    /// call [`Envelope::verify`].
    pub fn from_parts(claim: Claim, acknowledgments: Acknowledgments, signature: Signature) -> Result<Self, ClaimError> {
        check_acknowledgments(&claim, &acknowledgments)?;
        Ok(Self { claim, acknowledgments, signature })
    }

    /// Checks the signature against the actor's public key.
    pub fn verify(&self, key: &VerifyingKey) -> Result<(), ClaimError> {
        self.signature.verify(key, &self.signature_preimage(), "client")
    }

    /// The claim.
    pub fn claim(&self) -> &Claim {
        &self.claim
    }

    /// The claim's ID.
    pub fn id(&self) -> ClaimId {
        self.claim.id()
    }

    /// The acknowledgments, one for each term of the claim.
    pub fn acknowledgments(&self) -> &Acknowledgments {
        &self.acknowledgments
    }

    /// The actor's signature.
    pub fn signature(&self) -> Signature {
        self.signature
    }

    /// The text the actor signs. Each line ends with a line feed:
    ///
    /// ```text
    /// scatter-claim-signature-v0
    /// {claim ID in z-base-32}
    /// <{term IRI}> {prior claim ID in z-base-32}      one line per term, in ascending order of the IRI
    /// ```
    ///
    /// The claim ID commits to the triples, the attribution and the previous claim, so the
    /// signature covers those and the acknowledgments. It does not depend on how the
    /// envelope is written as RDF.
    pub fn signature_preimage(&self) -> String {
        signature_preimage(&self.claim, &self.acknowledgments)
    }
}

fn signature_preimage(claim: &Claim, acknowledgments: &Acknowledgments) -> String {
    let mut preimage = format!("{SIGNATURE_TAG}\n{}\n", claim.id());
    for (term, prior) in acknowledgments {
        preimage.push_str(&format!("{term} {prior}\n"));
    }
    preimage
}

fn check_acknowledgments(claim: &Claim, acknowledgments: &Acknowledgments) -> Result<(), ClaimError> {
    let terms = claim.terms();
    if let Some(missing) = terms.iter().find(|term| !acknowledgments.contains_key(*term)) {
        return Err(ClaimError::MissingAcknowledgment(missing.as_str().to_string()));
    }
    if let Some(extra) = acknowledgments.keys().find(|term| !terms.contains(*term)) {
        return Err(ClaimError::UnexpectedAcknowledgment(extra.as_str().to_string()));
    }
    Ok(())
}

/// An envelope a server accepted: the envelope, the sequence ID and time the server
/// assigned, and the server's signature over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedClaim {
    envelope: Envelope,
    sequence_id: String,
    timestamp: String,
    server_signature: Signature,
}

impl SignedClaim {
    /// Accepts an envelope: records the sequence ID and timestamp and signs with the
    /// server's key.
    ///
    /// The client's signature is not checked here, since which key an actor signs with is
    /// the caller's to look up: call [`Envelope::verify`] first.
    pub fn accept(
        envelope: Envelope,
        sequence_id: impl Into<String>,
        timestamp: impl Into<String>,
        server_key: &SigningKey,
    ) -> Result<Self, ClaimError> {
        let (sequence_id, timestamp) = (sequence_id.into(), timestamp.into());
        check_acceptance(&sequence_id, &timestamp)?;
        let server_signature = Signature::sign(server_key, &acceptance_preimage(&envelope, &sequence_id, &timestamp));
        Ok(Self { envelope, sequence_id, timestamp, server_signature })
    }

    /// A signed claim from its parts, as when it is read back. Neither signature is checked.
    pub fn from_parts(
        envelope: Envelope,
        sequence_id: impl Into<String>,
        timestamp: impl Into<String>,
        server_signature: Signature,
    ) -> Result<Self, ClaimError> {
        let (sequence_id, timestamp) = (sequence_id.into(), timestamp.into());
        check_acceptance(&sequence_id, &timestamp)?;
        Ok(Self { envelope, sequence_id, timestamp, server_signature })
    }

    /// Checks the server's signature against the server's public key.
    pub fn verify(&self, server_key: &VerifyingKey) -> Result<(), ClaimError> {
        self.server_signature.verify(server_key, &self.acceptance_preimage(), "server")
    }

    /// The envelope the server accepted.
    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    /// The claim's ID.
    pub fn id(&self) -> ClaimId {
        self.envelope.id()
    }

    /// The sequence ID the server assigned.
    pub fn sequence_id(&self) -> &str {
        &self.sequence_id
    }

    /// When the server accepted the claim, as it wrote it.
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    /// The server's signature.
    pub fn server_signature(&self) -> Signature {
        self.server_signature
    }

    /// The server's acceptance: what remains of a signed claim when everything that can be
    /// erased is.
    pub fn acceptance(&self) -> Acceptance {
        Acceptance {
            claim: self.id(),
            client_signature: self.envelope.signature().digest(),
            sequence_id: self.sequence_id.clone(),
            timestamp: self.timestamp.clone(),
            signature: self.server_signature,
        }
    }

    /// The text the server signs; see [`Acceptance::preimage`].
    pub fn acceptance_preimage(&self) -> String {
        self.acceptance().preimage()
    }
}

fn acceptance_preimage(envelope: &Envelope, sequence_id: &str, timestamp: &str) -> String {
    format!("{ACCEPTANCE_TAG}\n{}\n{}\n{sequence_id}\n{timestamp}\n", envelope.id(), envelope.signature().digest())
}

/// A server's acceptance of a claim: the claim's ID, the digest of the actor's signature,
/// the sequence ID and time the server assigned, and the server's signature over them.
///
/// It names the claim and the actor's signature by hashes alone, so it can be kept, and
/// its signature checked, after the claim's triples and the actor's signature are erased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acceptance {
    /// The claim's ID.
    pub claim: ClaimId,
    /// The [digest](Signature::digest) of the actor's signature.
    pub client_signature: String,
    /// The sequence ID the server assigned.
    pub sequence_id: String,
    /// When the server accepted the claim, as it wrote it.
    pub timestamp: String,
    /// The server's signature.
    pub signature: Signature,
}

impl Acceptance {
    /// The text the server signs. Each line ends with a line feed:
    ///
    /// ```text
    /// scatter-claim-acceptance-v0
    /// {claim ID in z-base-32}
    /// {SHA-256 of the actor's signature, in z-base-32}
    /// {sequence ID}
    /// {timestamp}
    /// ```
    pub fn preimage(&self) -> String {
        format!(
            "{ACCEPTANCE_TAG}\n{}\n{}\n{}\n{}\n",
            self.claim, self.client_signature, self.sequence_id, self.timestamp,
        )
    }

    /// Checks the server's signature against the server's public key.
    pub fn verify(&self, server_key: &VerifyingKey) -> Result<(), ClaimError> {
        self.signature.verify(server_key, &self.preimage(), "server")
    }
}

fn check_acceptance(sequence_id: &str, timestamp: &str) -> Result<(), ClaimError> {
    if sequence_id.contains(['\n', '\r']) || timestamp.contains(['\n', '\r']) {
        return Err(ClaimError::BadAcceptance);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use oxrdf::{Literal, Triple};

    pub(crate) fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    pub(crate) fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    pub(crate) fn claim() -> Claim {
        Claim::new(
            [
                Triple::new(iri("http://example.org/a"), iri("http://schema.org/name"), Literal::new_simple_literal("A")),
                Triple::new(iri("http://example.org/a"), iri("http://schema.org/knows"), iri("http://example.org/b")),
            ],
            iri("http://example.org/alice"),
            ClaimId::NONE,
        ).unwrap()
    }

    pub(crate) fn acknowledgments() -> Acknowledgments {
        let prior = ClaimId::from_bytes([7; 32]);
        [(iri("http://example.org/a"), prior), (iri("http://example.org/b"), ClaimId::NONE)].into()
    }

    #[test]
    fn signature_text_round_trips() {
        let signature = Envelope::sign(claim(), acknowledgments(), &key(1)).unwrap().signature();
        let text = signature.to_string();
        assert!(text.starts_with("ed25519:"));
        assert_eq!(text.parse::<Signature>().unwrap(), signature);
        assert!("ed25519:yyyy".parse::<Signature>().is_err());
        assert!(text["ed25519:".len()..].parse::<Signature>().is_err());
    }

    #[test]
    fn signature_preimage_is_tag_claim_id_and_sorted_acknowledgments() {
        let envelope = Envelope::sign(claim(), acknowledgments(), &key(1)).unwrap();
        assert_eq!(
            envelope.signature_preimage(),
            format!(
                "scatter-claim-signature-v0\n{}\n<http://example.org/a> {}\n<http://example.org/b> {}\n",
                claim().id(), ClaimId::from_bytes([7; 32]), ClaimId::NONE,
            ),
        );
    }

    #[test]
    fn client_signature_verifies_only_for_its_key_and_content() {
        let envelope = Envelope::sign(claim(), acknowledgments(), &key(1)).unwrap();
        assert_eq!(envelope.verify(&key(1).verifying_key()), Ok(()));
        assert_eq!(envelope.verify(&key(2).verifying_key()), Err(ClaimError::BadSignature("client")));

        // The same signature over different acknowledgments does not verify
        let mut other = acknowledgments();
        other.insert(iri("http://example.org/b"), ClaimId::from_bytes([9; 32]));
        let tampered = Envelope::from_parts(claim(), other, envelope.signature()).unwrap();
        assert_eq!(tampered.verify(&key(1).verifying_key()), Err(ClaimError::BadSignature("client")));
    }

    #[test]
    fn acknowledgments_must_match_the_terms() {
        let mut missing = acknowledgments();
        missing.remove(&iri("http://example.org/b"));
        assert_eq!(
            Envelope::sign(claim(), missing, &key(1)),
            Err(ClaimError::MissingAcknowledgment("http://example.org/b".into())),
        );

        let mut extra = acknowledgments();
        extra.insert(iri("http://example.org/c"), ClaimId::NONE);
        assert_eq!(
            Envelope::sign(claim(), extra, &key(1)),
            Err(ClaimError::UnexpectedAcknowledgment("http://example.org/c".into())),
        );
    }

    #[test]
    fn server_signature_covers_the_client_signature_sequence_and_time() {
        let envelope = Envelope::sign(claim(), acknowledgments(), &key(1)).unwrap();
        let signed = SignedClaim::accept(envelope.clone(), "2026112300000000000", "2026-11-23T00:00:00.000Z", &key(9)).unwrap();
        assert_eq!(
            signed.acceptance_preimage(),
            format!(
                "scatter-claim-acceptance-v0\n{}\n{}\n2026112300000000000\n2026-11-23T00:00:00.000Z\n",
                envelope.id(), envelope.signature().digest(),
            ),
        );
        assert_eq!(signed.verify(&key(9).verifying_key()), Ok(()));
        assert_eq!(signed.verify(&key(1).verifying_key()), Err(ClaimError::BadSignature("server")));
        // The acceptance alone is enough to check the server's signature
        assert_eq!(signed.acceptance().verify(&key(9).verifying_key()), Ok(()));
        assert_eq!(signed.acceptance().client_signature.len(), 52);

        let moved = SignedClaim::from_parts(envelope.clone(), "2026112300000000001", signed.timestamp(), signed.server_signature()).unwrap();
        assert_eq!(moved.verify(&key(9).verifying_key()), Err(ClaimError::BadSignature("server")));

        assert_eq!(SignedClaim::accept(envelope, "1\n2", "t", &key(9)), Err(ClaimError::BadAcceptance));
    }
}
