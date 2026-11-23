//! Lookups over what the store holds.
//!
//! A [`Snapshot`] is the quad store read once into memory. Its lookups answer what
//! accepting and preparing a claim need to know: which claim last used a term, which claim
//! an actor made last, and which key an actor has registered. "Last" is by the sequence ID
//! the server assigned. Each is a few pattern lookups over the store's indexes
//! ([`scatter_quadstore::Pattern`]); they are the kernel's own access paths, built on the
//! quad-store contract and not part of it.

use std::io;
use std::path::Path;

use ed25519_dalek::VerifyingKey;
use oxrdf::{NamedNodeRef, NamedOrBlankNode, Quad, Term, Triple};
use scatter_claims::id::ACK_NS;
use scatter_claims::{ClaimId, TripleHash};
use scatter_quadstore::{MemoryStore, NQuadsFile, Pattern};
use scatter_vocab::{prov, scatter};

use crate::keys::public_key_from_iri;

/// The quads of the store, as they were when it was read.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    quads: MemoryStore,
}

impl Snapshot {
    /// Reads the store's file. A file that does not exist is an empty store.
    pub fn load(storage_file: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self { quads: NQuadsFile::open(storage_file.as_ref())?.into_quads() })
    }

    /// A snapshot of the given quads.
    pub fn of(quads: MemoryStore) -> Self {
        Self { quads }
    }

    /// The quads.
    pub fn quads(&self) -> &MemoryStore {
        &self.quads
    }

    /// The triples of the quads, whatever their graph.
    pub fn triples(&self) -> Vec<Triple> {
        self.quads.iter().cloned().map(Triple::from).collect()
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// Whether the store holds the claim with this ID.
    pub fn contains_claim(&self, id: ClaimId) -> bool {
        let claim = id.iri();
        !self.quads.matching(&Pattern::any().subject(&claim)).is_empty()
    }

    /// The claim that last used a term, or [`ClaimId::NONE`] if none has.
    ///
    /// Every claim acknowledges each term it uses, so the claims that used a term are those
    /// with an acknowledgment of it.
    pub fn latest_claim_using(&self, term: NamedNodeRef<'_>) -> ClaimId {
        let acknowledgments = self.quads.matching(&Pattern::any().predicate(scatter::TERM).object(term));
        self.latest(acknowledgments.iter().filter_map(|quad| acknowledging_claim(&quad.subject)))
    }

    /// The claim an actor made last, or [`ClaimId::NONE`] if it has made none.
    pub fn latest_claim_by(&self, actor: NamedNodeRef<'_>) -> ClaimId {
        let attributions = self.quads.matching(&Pattern::any().predicate(prov::WAS_ATTRIBUTED_TO).object(actor));
        self.latest(attributions.iter().filter_map(|quad| claim_of(&quad.subject)))
    }

    /// The public key an actor registered last, if it has registered one.
    pub fn public_key(&self, actor: NamedNodeRef<'_>) -> Option<VerifyingKey> {
        self.quads.matching(&Pattern::any().subject(actor).predicate(scatter::HAS_PUBLIC_KEY))
            .into_iter()
            .filter_map(|registration| {
                let Term::NamedNode(key_iri) = &registration.object else { return None };
                let key = public_key_from_iri(key_iri.as_ref())?;
                // The registration is as recent as the latest claim holding it
                let triple_iri = TripleHash::of(&Triple::from(registration.clone())).iri();
                let sequence = self.quads.matching(&Pattern::any().subject(&triple_iri).predicate(scatter::CLAIMED_IN))
                    .iter()
                    .filter_map(|quad| match &quad.object {
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
        let claim = claim.iri();
        self.quads.matching(&Pattern::any().subject(&claim).predicate(scatter::SEQ_ID))
            .iter()
            .find_map(|quad| match &quad.object {
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

impl From<Vec<Quad>> for Snapshot {
    fn from(quads: Vec<Quad>) -> Self {
        Self::of(MemoryStore::from_quads(quads))
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
