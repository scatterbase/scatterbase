//! The envelope and the signed claim as RDF.
//!
//! An envelope with claim ID `C` is these triples:
//!
//! ```text
//! {s} {p} {o} .                                   each triple of the claim, as claimed
//!
//! scatter_triple:{T} scatter:claimedIn scatter_claim:{C} .   for each triple, T its triple hash
//! scatter_triple:{T} rdf:type scatter:ClaimTriple , rdf:Statement .
//! scatter_triple:{T} rdf:subject {s} ; rdf:predicate {p} ; rdf:object {o} .
//!
//! scatter_claim:{C} prov:wasAttributedTo {attribution} .
//! scatter_claim:{C} scatter:previousClaim scatter_claim:{previous} .
//!
//! <https://scatter.red/ack/{C}/term/{H}> scatter:term {term} .          for each term, H its term hash
//! <https://scatter.red/ack/{C}/term/{H}> scatter:priorClaim scatter_claim:{prior} .
//! scatter_claim:{C} scatter:acknowledgesPriorUse <https://scatter.red/ack/{C}/term/{H}> .
//!
//! scatter_claim:{C} scatter:userSignature "ed25519:{signature}" .
//! ```
//!
//! A signed claim adds what the server assigned:
//!
//! ```text
//! scatter_claim:{C} scatter:seqID "{sequence ID}" .
//! scatter_claim:{C} prov:generatedAtTime "{timestamp}"^^xsd:dateTime .
//! scatter_claim:{C} scatter:serverSignature "ed25519:{signature}" .
//! ```
//!
//! [`Envelope::from_triples`] and [`SignedClaim::from_triples`] read these back. The claim's
//! triples are taken from the reification, each checked against its triple hash, and the
//! claim ID is recomputed and must be `C`, so a set of triples that reads back is the
//! envelope that was written. Signatures are not checked by reading.

use std::collections::BTreeSet;

use oxrdf::{Literal, NamedNode, NamedNodeRef, NamedOrBlankNode, Term, Triple};

use crate::claim::compare_triples;
use crate::id::{ntriples_line, ClaimId, TermHash, TripleHash, CLAIM_NS, TRIPLE_NS};
use crate::{Acknowledgments, Claim, ClaimError, Envelope, Signature, SignedClaim};

/// The IRIs the envelope's RDF form uses.
///
/// The `scatter:` terms are to come from `scatter-vocab` once it publishes them.
pub mod vocab {
    use oxrdf::NamedNodeRef;

    /// The `rdf:` namespace.
    pub const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    /// The `prov:` namespace.
    pub const PROV_NS: &str = "http://www.w3.org/ns/prov#";
    /// The `scatter:` namespace.
    pub const SCATTER_NS: &str = "https://scatter.red/terms/v0/";

    macro_rules! terms {
        ($($(#[$doc:meta])* $name:ident = $iri:literal;)*) => {
            $($(#[$doc])* pub const $name: NamedNodeRef<'static> = NamedNodeRef::new_unchecked($iri);)*
        };
    }

    terms! {
        /// `rdf:type`
        RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        /// `rdf:Statement`
        RDF_STATEMENT = "http://www.w3.org/1999/02/22-rdf-syntax-ns#Statement";
        /// `rdf:subject`
        RDF_SUBJECT = "http://www.w3.org/1999/02/22-rdf-syntax-ns#subject";
        /// `rdf:predicate`
        RDF_PREDICATE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#predicate";
        /// `rdf:object`
        RDF_OBJECT = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
        /// `rdf:reifies`, from a reifier to the triple term it reifies (RDF 1.2)
        RDF_REIFIES = "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies";
        /// `xsd:dateTime`
        XSD_DATE_TIME = "http://www.w3.org/2001/XMLSchema#dateTime";
        /// `prov:wasAttributedTo`, from a claim to the actor making it
        PROV_WAS_ATTRIBUTED_TO = "http://www.w3.org/ns/prov#wasAttributedTo";
        /// `prov:generatedAtTime`, from a claim to when a server accepted it
        PROV_GENERATED_AT_TIME = "http://www.w3.org/ns/prov#generatedAtTime";
        /// `scatter:ClaimTriple`, the class of a triple as claimed
        CLAIM_TRIPLE = "https://scatter.red/terms/v0/ClaimTriple";
        /// `scatter:claimedIn`, from a claimed triple to its claim
        CLAIMED_IN = "https://scatter.red/terms/v0/claimedIn";
        /// `scatter:previousClaim`, from a claim to the one before it in the actor's chain
        PREVIOUS_CLAIM = "https://scatter.red/terms/v0/previousClaim";
        /// `scatter:userSignature`, the actor's signature
        USER_SIGNATURE = "https://scatter.red/terms/v0/userSignature";
        /// `scatter:serverSignature`, the accepting server's signature
        SERVER_SIGNATURE = "https://scatter.red/terms/v0/serverSignature";
        /// `scatter:seqID`, the sequence ID the accepting server assigned
        SEQ_ID = "https://scatter.red/terms/v0/seqID";
        /// `scatter:term`, from an acknowledgment to the term it is about
        TERM = "https://scatter.red/terms/v0/term";
        /// `scatter:priorClaim`, from an acknowledgment to the claim that last used the term
        PRIOR_CLAIM = "https://scatter.red/terms/v0/priorClaim";
        /// `scatter:acknowledgesPriorUse`, from a claim to each of its acknowledgments
        ACKNOWLEDGES_PRIOR_USE = "https://scatter.red/terms/v0/acknowledgesPriorUse";
    }
}

/// The prefixes the Turtle form declares, in the order it declares them.
const PREFIXES: [(&str, &str); 5] = [
    ("rdf", vocab::RDF_NS),
    ("prov", vocab::PROV_NS),
    ("scatter", vocab::SCATTER_NS),
    ("scatter_triple", TRIPLE_NS),
    ("scatter_claim", CLAIM_NS),
];

impl Envelope {
    /// The envelope as triples: the claim's triples, then the triples about them, in the
    /// order they are written.
    pub fn to_triples(&self) -> Vec<Triple> {
        self.groups().into_iter().flatten().collect()
    }

    /// The envelope as N-Triples, one triple per line.
    pub fn to_ntriples(&self) -> String {
        ntriples(&self.groups())
    }

    /// The envelope as Turtle, one triple per line under the prefixes `rdf`, `prov`,
    /// `scatter`, `scatter_triple` and `scatter_claim`.
    pub fn to_turtle(&self) -> String {
        turtle(&self.groups())
    }

    /// Reads an envelope back from its triples. Triples that are not part of an envelope
    /// are ignored, so a signed claim's triples read back as its envelope. The signature is
    /// not checked: call [`Envelope::verify`].
    pub fn from_triples(triples: &[Triple]) -> Result<Self, ClaimError> {
        let graph = Graph(triples);
        let candidates: BTreeSet<ClaimId> = triples.iter()
            .filter(|t| t.predicate.as_ref() == vocab::USER_SIGNATURE)
            .filter_map(|t| claim_id_of(&t.subject))
            .collect();

        let mut error = ClaimError::Malformed("no claim with a scatter:userSignature".into());
        for id in candidates {
            match graph.envelope(id) {
                Ok(envelope) => return Ok(envelope),
                Err(e) => error = e,
            }
        }
        Err(error)
    }

    /// The triples in the groups they are written in: the claim's, those about the claim,
    /// and the signature.
    fn groups(&self) -> Vec<Vec<Triple>> {
        let claim = self.claim();
        let id = claim.id();
        let claim_iri = id.iri();
        let mut about = Vec::new();

        for triple in claim.triples() {
            let hash_iri = TripleHash::of(triple).iri();
            about.push(Triple::new(hash_iri.clone(), vocab::CLAIMED_IN, claim_iri.clone()));
            about.push(Triple::new(hash_iri.clone(), vocab::RDF_TYPE, vocab::CLAIM_TRIPLE));
            about.push(Triple::new(hash_iri.clone(), vocab::RDF_TYPE, vocab::RDF_STATEMENT));
            about.push(Triple::new(hash_iri.clone(), vocab::RDF_SUBJECT, triple.subject.clone()));
            about.push(Triple::new(hash_iri.clone(), vocab::RDF_PREDICATE, triple.predicate.clone()));
            about.push(Triple::new(hash_iri, vocab::RDF_OBJECT, triple.object.clone()));
        }

        about.push(Triple::new(claim_iri.clone(), vocab::PROV_WAS_ATTRIBUTED_TO, claim.attribution().clone()));
        about.push(Triple::new(claim_iri.clone(), vocab::PREVIOUS_CLAIM, claim.previous().iri()));

        for (term, prior) in self.acknowledgments() {
            let ack_iri = TermHash::of(term.as_ref()).ack_iri(&id);
            about.push(Triple::new(ack_iri.clone(), vocab::TERM, term.clone()));
            about.push(Triple::new(ack_iri.clone(), vocab::PRIOR_CLAIM, prior.iri()));
            about.push(Triple::new(claim_iri.clone(), vocab::ACKNOWLEDGES_PRIOR_USE, ack_iri));
        }
        about.sort_by(compare_triples);

        let signature = Triple::new(
            claim_iri,
            vocab::USER_SIGNATURE,
            Literal::new_simple_literal(self.signature().to_string()),
        );
        vec![claim.triples().to_vec(), about, vec![signature]]
    }
}

impl SignedClaim {
    /// The signed claim as triples: the envelope's, then the server's.
    pub fn to_triples(&self) -> Vec<Triple> {
        self.groups().into_iter().flatten().collect()
    }

    /// The signed claim as N-Triples, one triple per line.
    pub fn to_ntriples(&self) -> String {
        ntriples(&self.groups())
    }

    /// The signed claim as Turtle; see [`Envelope::to_turtle`].
    pub fn to_turtle(&self) -> String {
        turtle(&self.groups())
    }

    /// Reads a signed claim back from its triples. Neither signature is checked.
    pub fn from_triples(triples: &[Triple]) -> Result<Self, ClaimError> {
        let envelope = Envelope::from_triples(triples)?;
        let graph = Graph(triples);
        let claim_iri = envelope.id().iri();

        let sequence_id = graph.literal(&claim_iri, vocab::SEQ_ID)?;
        let timestamp = graph.literal(&claim_iri, vocab::PROV_GENERATED_AT_TIME)?;
        let server_signature: Signature = graph.literal(&claim_iri, vocab::SERVER_SIGNATURE)?.parse()?;
        Self::from_parts(envelope, sequence_id, timestamp, server_signature)
    }

    fn groups(&self) -> Vec<Vec<Triple>> {
        let claim_iri = self.id().iri();
        let mut groups = self.envelope().groups();
        groups.push(vec![
            Triple::new(
                claim_iri.clone(),
                vocab::PROV_GENERATED_AT_TIME,
                Literal::new_typed_literal(self.timestamp(), vocab::XSD_DATE_TIME),
            ),
            Triple::new(claim_iri.clone(), vocab::SEQ_ID, Literal::new_simple_literal(self.sequence_id())),
        ]);
        groups.push(vec![Triple::new(
            claim_iri,
            vocab::SERVER_SIGNATURE,
            Literal::new_simple_literal(self.server_signature().to_string()),
        )]);
        groups
    }
}

fn claim_id_of(node: &NamedOrBlankNode) -> Option<ClaimId> {
    match node {
        NamedOrBlankNode::NamedNode(iri) => ClaimId::from_iri(iri.as_ref()),
        NamedOrBlankNode::BlankNode(_) => None,
    }
}

/// A set of triples, searched by subject and predicate.
struct Graph<'a>(&'a [Triple]);

impl<'a> Graph<'a> {
    fn objects(&self, subject: &'a NamedNode, predicate: NamedNodeRef<'a>) -> impl Iterator<Item = &'a Term> + 'a {
        self.0.iter()
            .filter(move |t| t.predicate.as_ref() == predicate && t.subject == NamedOrBlankNode::NamedNode(subject.clone()))
            .map(|t| &t.object)
    }

    /// The one object of a subject and predicate.
    fn object(&self, subject: &'a NamedNode, predicate: NamedNodeRef<'a>) -> Result<&'a Term, ClaimError> {
        let mut objects = self.objects(subject, predicate);
        match (objects.next(), objects.next()) {
            (Some(object), None) => Ok(object),
            (None, _) => Err(ClaimError::Malformed(format!("{subject} has no {predicate}"))),
            (Some(_), Some(_)) => Err(ClaimError::Malformed(format!("{subject} has more than one {predicate}"))),
        }
    }

    fn iri(&self, subject: &'a NamedNode, predicate: NamedNodeRef<'a>) -> Result<&'a NamedNode, ClaimError> {
        match self.object(subject, predicate)? {
            Term::NamedNode(iri) => Ok(iri),
            other => Err(ClaimError::Malformed(format!("the {predicate} of {subject} is {other}, not an IRI"))),
        }
    }

    fn literal(&self, subject: &'a NamedNode, predicate: NamedNodeRef<'a>) -> Result<&'a str, ClaimError> {
        match self.object(subject, predicate)? {
            Term::Literal(literal) => Ok(literal.value()),
            other => Err(ClaimError::Malformed(format!("the {predicate} of {subject} is {other}, not a literal"))),
        }
    }

    fn claim_id(&self, subject: &'a NamedNode, predicate: NamedNodeRef<'a>) -> Result<ClaimId, ClaimError> {
        let iri = self.iri(subject, predicate)?;
        ClaimId::from_iri(iri.as_ref())
            .ok_or_else(|| ClaimError::Malformed(format!("the {predicate} of {subject} is {iri}, not a claim")))
    }

    /// The envelope of the claim with this ID.
    fn envelope(&self, id: ClaimId) -> Result<Envelope, ClaimError> {
        let claim_iri = id.iri();
        let claim_term = Term::from(claim_iri.clone());

        // The claim's triples, from their reification
        let mut triples = Vec::new();
        for statement in self.0.iter().filter(|t| t.predicate.as_ref() == vocab::CLAIMED_IN && t.object == claim_term) {
            let NamedOrBlankNode::NamedNode(hash_iri) = &statement.subject else { continue };
            let hash = TripleHash::from_iri(hash_iri.as_ref())
                .ok_or_else(|| ClaimError::Malformed(format!("{hash_iri} is claimed but is not a triple hash")))?;
            triples.push(self.statement(hash_iri, hash)?);
        }

        let attribution = self.iri(&claim_iri, vocab::PROV_WAS_ATTRIBUTED_TO)?.clone();
        let previous = self.claim_id(&claim_iri, vocab::PREVIOUS_CLAIM)?;
        let claim = Claim::new(triples, attribution, previous)?;
        if claim.id() != id {
            return Err(ClaimError::Malformed(format!(
                "the triples claimed in {claim_iri} make the claim {}", claim.id(),
            )));
        }

        let mut acknowledgments = Acknowledgments::new();
        for ack in self.objects(&claim_iri, vocab::ACKNOWLEDGES_PRIOR_USE) {
            let Term::NamedNode(ack_iri) = ack else {
                return Err(ClaimError::Malformed(format!("the acknowledgment {ack} is not an IRI")));
            };
            let term = self.iri(ack_iri, vocab::TERM)?;
            if TermHash::of(term.as_ref()).ack_iri(&id) != *ack_iri {
                return Err(ClaimError::Malformed(format!("{ack_iri} is not the acknowledgment of {term}")));
            }
            acknowledgments.insert(term.clone(), self.claim_id(ack_iri, vocab::PRIOR_CLAIM)?);
        }

        let signature: Signature = self.literal(&claim_iri, vocab::USER_SIGNATURE)?.parse()?;
        Envelope::from_parts(claim, acknowledgments, signature)
    }

    /// The triple a reification describes, which must have the given hash.
    ///
    /// The store keeps one reification per triple however many claims hold it, and a claim
    /// may itself say something about a reified triple, so the subject, predicate and
    /// object are whichever of those present hash to the triple hash.
    fn statement(&self, hash_iri: &'a NamedNode, hash: TripleHash) -> Result<Triple, ClaimError> {
        for subject in self.objects(hash_iri, vocab::RDF_SUBJECT) {
            let Term::NamedNode(subject) = subject else { continue };
            for predicate in self.objects(hash_iri, vocab::RDF_PREDICATE) {
                let Term::NamedNode(predicate) = predicate else { continue };
                for object in self.objects(hash_iri, vocab::RDF_OBJECT) {
                    let triple = Triple::new(subject.clone(), predicate.clone(), object.clone());
                    if TripleHash::of(&triple) == hash {
                        return Ok(triple);
                    }
                }
            }
        }
        Err(ClaimError::Malformed(format!("{hash_iri} has no reification that hashes to it")))
    }
}

fn ntriples(groups: &[Vec<Triple>]) -> String {
    let mut text = String::new();
    for triple in groups.iter().flatten() {
        text.push_str(&ntriples_line(triple));
        text.push('\n');
    }
    text
}

fn turtle(groups: &[Vec<Triple>]) -> String {
    let mut text = String::new();
    for (prefix, namespace) in PREFIXES {
        text.push_str(&format!("@prefix {prefix}: <{namespace}> .\n"));
    }
    text.push('\n');
    for triple in groups.iter().flatten() {
        let subject = match &triple.subject {
            NamedOrBlankNode::NamedNode(iri) => abbreviate(iri.as_ref()),
            NamedOrBlankNode::BlankNode(blank) => blank.to_string(),
        };
        let object = match &triple.object {
            Term::NamedNode(iri) => abbreviate(iri.as_ref()),
            other => other.to_string(),
        };
        text.push_str(&format!("{subject} {} {object} .\n", abbreviate(triple.predicate.as_ref())));
    }
    text
}

/// An IRI as a prefixed name under one of [`PREFIXES`], or in angle brackets if it is under
/// none of them or its local part is not letters, digits, `_` and `-` alone.
fn abbreviate(iri: NamedNodeRef<'_>) -> String {
    for (prefix, namespace) in PREFIXES {
        if let Some(local) = iri.as_str().strip_prefix(namespace) {
            let plain = !local.is_empty()
                && !local.starts_with('-')
                && local.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            if plain {
                return format!("{prefix}:{local}");
            }
        }
    }
    iri.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::tests::{acknowledgments, claim, iri, key};

    fn envelope() -> Envelope {
        Envelope::sign(claim(), acknowledgments(), &key(1)).unwrap()
    }

    fn signed() -> SignedClaim {
        SignedClaim::accept(envelope(), "2026112300000000000", "2026-11-23T00:00:00.000Z", &key(9)).unwrap()
    }

    #[test]
    fn envelope_round_trips_through_triples() {
        let envelope = envelope();
        let triples = envelope.to_triples();
        // 2 claimed, 6 about each, attribution and previous, 3 per acknowledgment, signature
        assert_eq!(triples.len(), 2 + 12 + 2 + 6 + 1);
        let read = Envelope::from_triples(&triples).unwrap();
        assert_eq!(read, envelope);
        assert_eq!(read.verify(&key(1).verifying_key()), Ok(()));
    }

    #[test]
    fn signed_claim_round_trips_through_triples() {
        let signed = signed();
        let triples = signed.to_triples();
        let read = SignedClaim::from_triples(&triples).unwrap();
        assert_eq!(read, signed);
        assert_eq!(read.verify(&key(9).verifying_key()), Ok(()));
        assert_eq!(Envelope::from_triples(&triples).unwrap(), *signed.envelope());
        assert!(SignedClaim::from_triples(&signed.envelope().to_triples()).is_err());
    }

    #[test]
    fn reading_ignores_order_and_other_triples() {
        let mut triples = signed().to_triples();
        triples.reverse();
        triples.push(Triple::new(iri("http://example.org/x"), iri("http://schema.org/name"), Literal::new_simple_literal("X")));
        assert_eq!(SignedClaim::from_triples(&triples).unwrap(), signed());
    }

    #[test]
    fn a_changed_triple_does_not_read_back() {
        let envelope = envelope();
        let changed: Vec<Triple> = envelope.to_triples().into_iter()
            .map(|t| if t.predicate.as_ref() == vocab::RDF_OBJECT && t.object.is_literal() {
                Triple::new(t.subject, t.predicate, Literal::new_simple_literal("B"))
            } else {
                t
            })
            .collect();
        assert!(matches!(Envelope::from_triples(&changed), Err(ClaimError::Malformed(_))));
    }

    #[test]
    fn a_changed_attribution_does_not_read_back() {
        let changed: Vec<Triple> = envelope().to_triples().into_iter()
            .map(|t| if t.predicate.as_ref() == vocab::PROV_WAS_ATTRIBUTED_TO {
                Triple::new(t.subject, t.predicate, iri("http://example.org/mallory"))
            } else {
                t
            })
            .collect();
        assert!(matches!(Envelope::from_triples(&changed), Err(ClaimError::Malformed(_))));
    }

    #[test]
    fn two_claims_sharing_a_triple_both_read_back() {
        let first = envelope();
        let later = Claim::new(claim().triples().to_vec(), iri("http://example.org/bob"), first.id()).unwrap();
        let second = Envelope::sign(later, acknowledgments(), &key(2)).unwrap();

        let mut store = first.to_triples();
        store.extend(second.to_triples());
        let graph = Graph(&store);
        assert_eq!(graph.envelope(first.id()).unwrap(), first);
        assert_eq!(graph.envelope(second.id()).unwrap(), second);
    }

    #[test]
    fn turtle_uses_prefixes_and_ends_with_the_signatures() {
        let signed = signed();
        let id = signed.id();
        let text = signed.to_turtle();
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(lines[0], "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .");
        assert_eq!(lines[5], "");
        // The claim's triples come first, in order
        assert_eq!(lines[6], "<http://example.org/a> <http://schema.org/knows> <http://example.org/b> .");
        assert_eq!(lines[7], "<http://example.org/a> <http://schema.org/name> \"A\" .");
        assert!(text.contains(&format!("scatter_claim:{id} prov:wasAttributedTo <http://example.org/alice> .\n")));
        assert!(text.contains(&format!("scatter_claim:{id} scatter:previousClaim scatter_claim:{} .\n", ClaimId::NONE)));
        assert!(text.contains(" rdf:type scatter:ClaimTriple .\n"));
        // Acknowledgment IRIs have slashes, so they are written in full
        assert!(text.contains(&format!("<https://scatter.red/ack/{id}/term/")));

        let n = lines.len();
        assert_eq!(lines[n - 4], format!("scatter_claim:{id} scatter:userSignature \"{}\" .", signed.envelope().signature()));
        assert_eq!(lines[n - 3], format!("scatter_claim:{id} prov:generatedAtTime \"2026-11-23T00:00:00.000Z\"^^<http://www.w3.org/2001/XMLSchema#dateTime> ."));
        assert_eq!(lines[n - 2], format!("scatter_claim:{id} scatter:seqID \"2026112300000000000\" ."));
        assert_eq!(lines[n - 1], format!("scatter_claim:{id} scatter:serverSignature \"{}\" .", signed.server_signature()));
    }

    #[test]
    fn ntriples_is_one_full_triple_per_line() {
        let envelope = envelope();
        let text = envelope.to_ntriples();
        assert_eq!(text.lines().count(), envelope.to_triples().len());
        assert!(text.lines().all(|line| line.ends_with(" .") && !line.contains("scatter_claim:")));
    }
}
