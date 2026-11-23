//! Lookups over what the store holds.
//!
//! A [`Snapshot`] is the store's triples read once. Its lookups answer what accepting and
//! preparing a claim need to know: which claim last used a term, which claim an actor made
//! last, and which key an actor has registered. "Last" is by the sequence ID the server
//! assigned.

use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

use ed25519_dalek::VerifyingKey;
use oxrdf::{NamedNodeRef, NamedOrBlankNode, Term, Triple};
use oxttl::NTriplesParser;
use scatter_claims::id::ACK_NS;
use scatter_claims::rdf::vocab;
use scatter_claims::{ClaimId, TripleHash};

use crate::keys::public_key_from_iri;
use crate::uri::scatter;

/// The triples of the store, as they were when it was read.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    triples: Vec<Triple>,
}

impl Snapshot {
    /// Reads the store's N-Triples file. A file that does not exist is an empty store.
    pub fn load(storage_file: impl AsRef<Path>) -> io::Result<Self> {
        let path = storage_file.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }
        let triples = NTriplesParser::new()
            .for_reader(BufReader::new(File::open(path)?))
            .collect::<Result<Vec<Triple>, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Error reading {}: {}", path.display(), e)))?;
        Ok(Self { triples })
    }

    /// A snapshot of the given triples.
    pub fn of(triples: Vec<Triple>) -> Self {
        Self { triples }
    }

    /// The triples.
    pub fn triples(&self) -> &[Triple] {
        &self.triples
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.triples.is_empty()
    }

    /// Whether the store holds the claim with this ID.
    pub fn contains_claim(&self, id: ClaimId) -> bool {
        let claim = NamedOrBlankNode::from(id.iri());
        self.triples.iter().any(|t| t.subject == claim)
    }

    /// The claim that last used a term, or [`ClaimId::NONE`] if none has.
    ///
    /// Every claim acknowledges each term it uses, so the claims that used a term are those
    /// with an acknowledgment of it.
    pub fn latest_claim_using(&self, term: NamedNodeRef<'_>) -> ClaimId {
        let term = Term::from(term);
        self.latest(self.triples.iter()
            .filter(|t| t.predicate.as_ref() == vocab::TERM && t.object == term)
            .filter_map(|t| acknowledging_claim(&t.subject)))
    }

    /// The claim an actor made last, or [`ClaimId::NONE`] if it has made none.
    pub fn latest_claim_by(&self, actor: NamedNodeRef<'_>) -> ClaimId {
        let actor = Term::from(actor);
        self.latest(self.triples.iter()
            .filter(|t| t.predicate.as_ref() == vocab::PROV_WAS_ATTRIBUTED_TO && t.object == actor)
            .filter_map(|t| claim_of(&t.subject)))
    }

    /// The public key an actor registered last, if it has registered one.
    pub fn public_key(&self, actor: NamedNodeRef<'_>) -> Option<VerifyingKey> {
        let actor = NamedOrBlankNode::from(actor);
        self.triples.iter()
            .filter(|t| t.subject == actor && t.predicate.as_str() == scatter::HAS_PUBLIC_KEY)
            .filter_map(|registration| {
                let Term::NamedNode(key_iri) = &registration.object else { return None };
                let key = public_key_from_iri(key_iri.as_ref())?;
                // The registration is as recent as the latest claim holding it
                let triple_iri = NamedOrBlankNode::from(TripleHash::of(registration).iri());
                let sequence = self.triples.iter()
                    .filter(|t| t.subject == triple_iri && t.predicate.as_ref() == vocab::CLAIMED_IN)
                    .filter_map(|t| match &t.object {
                        Term::NamedNode(claim) => ClaimId::from_iri(claim.as_ref()),
                        _ => None,
                    })
                    .filter_map(|claim| self.sequence_id(claim))
                    .max()?;
                Some((sequence, key))
            })
            .max_by_key(|(sequence, _)| *sequence)
            .map(|(_, key)| key)
    }

    /// The sequence ID the server gave a claim, which is its offset in the claim log, if
    /// the store holds the claim.
    pub fn sequence_id(&self, claim: ClaimId) -> Option<u64> {
        let claim = NamedOrBlankNode::from(claim.iri());
        self.triples.iter()
            .filter(|t| t.subject == claim && t.predicate.as_ref() == vocab::SEQ_ID)
            .find_map(|t| match &t.object {
                Term::Literal(literal) => literal.value().parse().ok(),
                _ => None,
            })
    }

    /// The accepted claim with the greatest sequence ID among those given.
    fn latest(&self, claims: impl Iterator<Item = ClaimId>) -> ClaimId {
        claims
            .filter_map(|claim| Some((self.sequence_id(claim)?, claim)))
            .max()
            .map_or(ClaimId::NONE, |(_, claim)| claim)
    }
}

fn claim_of(subject: &NamedOrBlankNode) -> Option<ClaimId> {
    match subject {
        NamedOrBlankNode::NamedNode(iri) => ClaimId::from_iri(iri.as_ref()),
        NamedOrBlankNode::BlankNode(_) => None,
    }
}

/// The claim an acknowledgment IRI, `https://scatter.red/ack/{claim}/term/{term hash}`,
/// belongs to.
fn acknowledging_claim(ack: &NamedOrBlankNode) -> Option<ClaimId> {
    let NamedOrBlankNode::NamedNode(iri) = ack else { return None };
    let (claim, _) = iri.as_str().strip_prefix(ACK_NS)?.split_once("/term/")?;
    claim.parse().ok()
}
