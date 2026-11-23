//! Lookups over what the store holds.
//!
//! A [`Snapshot`] is the quad store ([`crate::storage`]) read once into memory. Its lookups answer what
//! accepting and preparing a claim need to know: which claim last used a term and which
//! claim an actor made last. "Last" is by the sequence ID the server assigned. (An actor's
//! keys are not looked up here: they are read from the log, [`crate::log`].) Each is a few pattern lookups over the store's indexes
//! ([`scatter_quadstore::Pattern`]); they are the kernel's own access paths, built on the
//! quad-store contract and not part of it.

use std::io;
use std::path::Path;

use oxrdf::{NamedNode, NamedNodeRef, NamedOrBlankNode, Term, Triple};
use scatter_claims::id::ACK_NS;
use scatter_claims::ClaimId;
use scatter_quadstore::{MemoryStore, NQuadsFile, Pattern};
use scatter_vocab::{prov, scatter};

use crate::instance::{Instance, GRAPH_META};

/// The quads of the store, as they were when it was read.
///
/// The lookups read one graph only: `meta`, which holds what was recorded about each
/// accepted claim. A claim's own triples are never taken for that, whatever they say.
#[derive(Debug, Clone)]
pub struct Snapshot {
    quads: MemoryStore,
    meta: NamedNode,
}

impl Snapshot {
    /// Reads the instance's store. A file that does not exist is an empty store.
    pub fn load(instance: &Instance) -> io::Result<Self> {
        Ok(Self::of(instance, NQuadsFile::open(Path::new(&instance.storage_file))?.into_quads()))
    }

    /// A snapshot of the given quads, read as the instance's store.
    pub fn of(instance: &Instance, quads: MemoryStore) -> Self {
        Self {
            quads,
            meta: instance.graph(GRAPH_META),
        }
    }

    /// The quads.
    pub fn quads(&self) -> &MemoryStore {
        &self.quads
    }

    /// The triples of one of the instance's graphs.
    pub fn triples_in(&self, graph: &NamedNode) -> Vec<Triple> {
        self.quads.matching(&Pattern::any().graph(graph)).into_iter().map(Triple::from).collect()
    }

    /// The triples recorded about the accepted claims: the `meta` graph.
    pub fn meta(&self) -> Vec<Triple> {
        self.triples_in(&self.meta)
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    /// A pattern over what was recorded about claims.
    fn about(&self) -> Pattern<'_> {
        Pattern::any().graph(&self.meta)
    }

    /// Whether the store holds the claim with this ID.
    pub fn contains_claim(&self, id: ClaimId) -> bool {
        let claim = id.iri();
        !self.quads.matching(&self.about().subject(&claim)).is_empty()
    }

    /// The claim that last used a term, or [`ClaimId::NONE`] if none has.
    ///
    /// Every claim acknowledges each term it uses, so the claims that used a term are those
    /// with an acknowledgment of it.
    pub fn latest_claim_using(&self, term: NamedNodeRef<'_>) -> ClaimId {
        let acknowledgments = self.quads.matching(&self.about().predicate(scatter::TERM).object(term));
        self.latest(acknowledgments.iter().filter_map(|quad| acknowledging_claim(&quad.subject)))
    }

    /// The claim an actor made last, or [`ClaimId::NONE`] if it has made none.
    pub fn latest_claim_by(&self, actor: NamedNodeRef<'_>) -> ClaimId {
        self.latest(self.claims_by(actor).into_iter())
    }

    /// The claims an actor made.
    fn claims_by(&self, actor: NamedNodeRef<'_>) -> Vec<ClaimId> {
        self.quads.matching(&self.about().predicate(prov::WAS_ATTRIBUTED_TO).object(actor))
            .iter()
            .filter_map(|quad| claim_of(&quad.subject))
            .collect()
    }

    /// The sequence ID the server gave a claim, which is its offset in the claim log, if
    /// the store holds the claim.
    pub fn sequence_id(&self, claim: ClaimId) -> Option<u64> {
        let claim = claim.iri();
        self.quads.matching(&self.about().subject(&claim).predicate(scatter::SEQ_ID))
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
