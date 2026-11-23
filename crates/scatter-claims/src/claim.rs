//! The claim: triples, attribution and previous claim, and the claim ID.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use oxrdf::{BlankNode, NamedNode, NamedOrBlankNode, Term, Triple};

use crate::id::{sha256, ClaimId, TripleHash};
use crate::rdf::vocab;
use crate::ClaimError;

/// The first line of the claim ID preimage.
const CLAIM_TAG: &str = "scatter-claim-v0";

/// The path segment that stands for the claim in a skolem IRI while the claim's
/// provisional ID is computed ([`Claim::from_parsed`]).
const PROVISIONAL: &str = "current";

/// A set of triples asserted by one actor as the next link in its chain of claims.
///
/// The triples are a set in a fixed order, hold IRIs and literals only, and are never
/// empty. A claim does not change once made: its [ID](Claim::id) is the hash of all of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    triples: Vec<Triple>,
    attribution: NamedNode,
    previous: ClaimId,
}

impl Claim {
    /// A claim of `triples` by `attribution`, following `previous` in that actor's chain
    /// ([`ClaimId::NONE`] for the actor's first claim).
    ///
    /// Duplicate triples are dropped and the rest put in order. A triple with a blank node
    /// or a triple term is refused: [`Claim::from_parsed`] replaces those first.
    pub fn new(
        triples: impl IntoIterator<Item = Triple>,
        attribution: NamedNode,
        previous: ClaimId,
    ) -> Result<Self, ClaimError> {
        let mut triples: Vec<Triple> = triples.into_iter().collect();
        for triple in &triples {
            if let NamedOrBlankNode::BlankNode(blank) = &triple.subject {
                return Err(ClaimError::UnsupportedTerm(format!("the blank node {blank}")));
            }
            match &triple.object {
                Term::NamedNode(_) | Term::Literal(_) => {},
                other => return Err(ClaimError::UnsupportedTerm(other.to_string())),
            }
        }
        triples.sort_by(compare_triples);
        triples.dedup();
        if triples.is_empty() {
            return Err(ClaimError::Empty);
        }
        Ok(Self { triples, attribution, previous })
    }

    /// A claim of triples as a parser gives them.
    ///
    /// Three things a claim cannot hold are replaced:
    ///
    /// - **A quoted triple** becomes its [triple hash IRI](TripleHash::iri), the IRI the
    ///   claim envelope gives that triple when it is claimed. In its RDF 1.2 form a quoted
    ///   triple is a reifier with an `rdf:reifies` triple whose object is the triple term:
    ///   the reifier becomes the triple hash IRI and the `rdf:reifies` triple is dropped,
    ///   so `<< s p o >> q v` is claimed as `<https://scatter.red/triple/{hash}> q v`.
    /// - **A blank node written without a label** (`[ … ]`, a collection) is labelled
    ///   `anon1`, `anon2`, … in order of first appearance, skipping any label the input
    ///   itself uses, since parsers label such nodes at random.
    /// - **A blank node** becomes the skolem IRI
    ///   `{base}/.well-known/genid/{claim}/{label}`, where `{claim}` is the claim's
    ///   *provisional ID*: the ID of the same claim with `current` in place of `{claim}`.
    ///   The claim's own ID is computed over the final IRIs and so differs from it.
    pub fn from_parsed(
        parsed: Vec<Triple>,
        attribution: NamedNode,
        previous: ClaimId,
        base: &str,
    ) -> Result<Self, ClaimError> {
        let parsed = label_anonymous_nodes(parsed);
        let has_blank_nodes = parsed.iter().any(mentions_blank_node);

        let provisional = Self::new(
            replace_terms(&parsed, base, PROVISIONAL)?,
            attribution.clone(),
            previous,
        )?;
        if !has_blank_nodes {
            return Ok(provisional);
        }
        let segment = provisional.id().to_string();
        Self::new(replace_terms(&parsed, base, &segment)?, attribution, previous)
    }

    /// The triples, in order and without duplicates.
    pub fn triples(&self) -> &[Triple] {
        &self.triples
    }

    /// The actor making the claim.
    pub fn attribution(&self) -> &NamedNode {
        &self.attribution
    }

    /// The claim before this one in the actor's chain, or [`ClaimId::NONE`].
    pub fn previous(&self) -> ClaimId {
        self.previous
    }

    /// The claim's ID: SHA-256 of [`Claim::preimage`].
    pub fn id(&self) -> ClaimId {
        ClaimId::from_bytes(sha256(self.preimage().as_bytes()))
    }

    /// The text a claim ID is the hash of. Each line ends with a line feed:
    ///
    /// ```text
    /// scatter-claim-v0
    /// {attribution IRI}
    /// {previous claim ID in z-base-32}
    /// {triple hash in z-base-32}      one line per triple, in ascending order of the text
    /// ```
    ///
    /// Nothing a server assigns when it accepts the claim is part of it, so the ID is known
    /// before the claim is submitted and is the same wherever the claim is accepted.
    pub fn preimage(&self) -> String {
        let mut hashes: Vec<String> = self.triples.iter().map(|t| TripleHash::of(t).to_string()).collect();
        hashes.sort();

        let mut preimage = format!("{CLAIM_TAG}\n{}\n{}\n", self.attribution.as_str(), self.previous);
        for hash in hashes {
            preimage.push_str(&hash);
            preimage.push('\n');
        }
        preimage
    }

    /// The terms whose prior use the claim acknowledges: every IRI in subject or object
    /// position.
    pub fn terms(&self) -> BTreeSet<NamedNode> {
        let mut terms = BTreeSet::new();
        for triple in &self.triples {
            if let NamedOrBlankNode::NamedNode(subject) = &triple.subject {
                terms.insert(subject.clone());
            }
            if let Term::NamedNode(object) = &triple.object {
                terms.insert(object.clone());
            }
        }
        terms
    }
}

/// The order triples are kept and written in: by subject, then predicate, then object, with
/// IRI objects before literals.
pub fn compare_triples(a: &Triple, b: &Triple) -> Ordering {
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

fn mentions_blank_node(triple: &Triple) -> bool {
    triple.subject.is_blank_node()
        || match &triple.object {
            Term::BlankNode(_) => true,
            Term::Triple(quoted) => mentions_blank_node(quoted),
            _ => false,
        }
}

/// Replaces quoted triples by triple hash IRIs and blank nodes by skolem IRIs under
/// `{base}/.well-known/genid/{segment}/`.
fn replace_terms(parsed: &[Triple], base: &str, segment: &str) -> Result<Vec<Triple>, ClaimError> {
    // Reifiers of quoted triples, by blank node label, and the triple hash IRI of each.
    let mut reifiers: HashMap<String, NamedNode> = HashMap::new();
    let mut triples = Vec::new();

    for triple in parsed {
        if triple.predicate.as_ref() == vocab::RDF_REIFIES {
            if let (NamedOrBlankNode::BlankNode(reifier), Term::Triple(quoted)) = (&triple.subject, &triple.object) {
                let quoted = replace_in_triple(quoted, base, segment, &reifiers)?;
                reifiers.insert(reifier.as_str().to_string(), TripleHash::of(&quoted).iri());
                continue;
            }
        }
        triples.push(replace_in_triple(triple, base, segment, &reifiers)?);
    }
    Ok(triples)
}

fn replace_in_triple(
    triple: &Triple,
    base: &str,
    segment: &str,
    reifiers: &HashMap<String, NamedNode>,
) -> Result<Triple, ClaimError> {
    let node = |blank: &BlankNode| match reifiers.get(blank.as_str()) {
        Some(triple_hash_iri) => Ok(triple_hash_iri.clone()),
        None => skolem_iri(base, segment, blank.as_str()),
    };

    let subject = match &triple.subject {
        NamedOrBlankNode::NamedNode(named) => named.clone(),
        NamedOrBlankNode::BlankNode(blank) => node(blank)?,
    };
    let object: Term = match &triple.object {
        Term::BlankNode(blank) => node(blank)?.into(),
        Term::Triple(quoted) => TripleHash::of(&replace_in_triple(quoted, base, segment, reifiers)?).iri().into(),
        other => other.clone(),
    };
    Ok(Triple::new(subject, triple.predicate.clone(), object))
}

/// The skolem IRI of a blank node: `{base}/.well-known/genid/{segment}/{label}`.
fn skolem_iri(base: &str, segment: &str, label: &str) -> Result<NamedNode, ClaimError> {
    let iri = format!("{}/.well-known/genid/{segment}/{label}", base.trim_end_matches('/'));
    NamedNode::new(&iri).map_err(|_| ClaimError::BadIri(iri))
}

/// Gives every blank node written without a label the label `anon1`, `anon2`, … in order of
/// first appearance, skipping any label written in the input.
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
        fn label(&mut self, blank: BlankNode) -> BlankNode {
            let Some(id) = blank.as_ref().unique_id() else {
                return blank;
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
                NamedOrBlankNode::BlankNode(blank) => self.label(blank).into(),
                named => named,
            };
            let object = match triple.object {
                Term::BlankNode(blank) => self.label(blank).into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::Literal;

    fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    fn name(subject: &str, value: &str) -> Triple {
        Triple::new(iri(subject), iri("http://schema.org/name"), Literal::new_simple_literal(value))
    }

    fn alice() -> NamedNode {
        iri("http://example.org/alice")
    }

    #[test]
    fn triples_are_a_sorted_set() {
        let a = name("http://example.org/a", "A");
        let b = name("http://example.org/b", "B");
        let claim = Claim::new([b.clone(), a.clone(), b.clone()], alice(), ClaimId::NONE).unwrap();
        assert_eq!(claim.triples(), &[a.clone(), b.clone()]);
        assert_eq!(claim.id(), Claim::new([a, b], alice(), ClaimId::NONE).unwrap().id());
    }

    #[test]
    fn iri_objects_sort_before_literals() {
        let knows = Triple::new(iri("http://example.org/a"), iri("http://schema.org/p"), iri("http://example.org/z"));
        let literal = Triple::new(iri("http://example.org/a"), iri("http://schema.org/p"), Literal::new_simple_literal("a"));
        let claim = Claim::new([literal.clone(), knows.clone()], alice(), ClaimId::NONE).unwrap();
        assert_eq!(claim.triples(), &[knows, literal]);
    }

    #[test]
    fn empty_and_unsupported_claims_are_refused() {
        assert_eq!(Claim::new([], alice(), ClaimId::NONE), Err(ClaimError::Empty));
        let blank = Triple::new(BlankNode::new_unchecked("x"), iri("http://schema.org/p"), iri("http://example.org/z"));
        assert!(matches!(Claim::new([blank], alice(), ClaimId::NONE), Err(ClaimError::UnsupportedTerm(_))));
    }

    #[test]
    fn preimage_is_tag_attribution_previous_and_sorted_triple_hashes() {
        let a = name("http://example.org/a", "A");
        let b = name("http://example.org/b", "B");
        let previous = ClaimId::from_bytes(sha256(b"previous"));
        let claim = Claim::new([a.clone(), b.clone()], alice(), previous).unwrap();

        let mut hashes = [TripleHash::of(&a).to_string(), TripleHash::of(&b).to_string()];
        hashes.sort();
        let expected = format!(
            "scatter-claim-v0\nhttp://example.org/alice\n{previous}\n{}\n{}\n",
            hashes[0], hashes[1],
        );
        assert_eq!(claim.preimage(), expected);
        assert_eq!(claim.id().as_bytes(), &sha256(expected.as_bytes()));
    }

    #[test]
    fn id_depends_on_every_part() {
        let a = name("http://example.org/a", "A");
        let base = Claim::new([a.clone()], alice(), ClaimId::NONE).unwrap().id();
        let other_triples = Claim::new([name("http://example.org/a", "B")], alice(), ClaimId::NONE).unwrap().id();
        let other_actor = Claim::new([a.clone()], iri("http://example.org/bob"), ClaimId::NONE).unwrap().id();
        let other_previous = Claim::new([a], alice(), base).unwrap().id();
        assert_ne!(base, other_triples);
        assert_ne!(base, other_actor);
        assert_ne!(base, other_previous);
    }

    #[test]
    fn terms_are_subject_and_object_iris() {
        let knows = Triple::new(iri("http://example.org/a"), iri("http://schema.org/knows"), iri("http://example.org/b"));
        let claim = Claim::new([knows, name("http://example.org/a", "A")], alice(), ClaimId::NONE).unwrap();
        let terms: Vec<String> = claim.terms().into_iter().map(NamedNode::into_string).collect();
        assert_eq!(terms, ["http://example.org/a", "http://example.org/b"]);
    }

    #[test]
    fn blank_nodes_become_skolem_iris_under_the_provisional_id() {
        let parsed = vec![Triple::new(BlankNode::new_unchecked("x"), iri("http://schema.org/name"), Literal::new_simple_literal("blank"))];

        let provisional = Claim::new(
            [name("http://example.org/.well-known/genid/current/x", "blank")],
            alice(),
            ClaimId::NONE,
        ).unwrap().id();

        let claim = Claim::from_parsed(parsed.clone(), alice(), ClaimId::NONE, "http://example.org/").unwrap();
        assert_eq!(
            claim.triples()[0].subject.to_string(),
            format!("<http://example.org/.well-known/genid/{provisional}/x>"),
        );
        assert_ne!(claim.id(), provisional);
        assert_eq!(claim, Claim::from_parsed(parsed, alice(), ClaimId::NONE, "http://example.org").unwrap());
    }

    #[test]
    fn unlabelled_blank_nodes_are_labelled_in_order() {
        let p = iri("http://schema.org/name");
        let first = BlankNode::default();
        let second = BlankNode::default();
        let parsed = vec![
            Triple::new(first.clone(), p.clone(), Literal::new_simple_literal("first")),
            Triple::new(second, p.clone(), Literal::new_simple_literal("second")),
            Triple::new(BlankNode::new_unchecked("anon1"), p.clone(), Literal::new_simple_literal("written")),
            Triple::new(first, p, Literal::new_simple_literal("first again")),
        ];
        let labelled = label_anonymous_nodes(parsed);
        let labels: Vec<String> = labelled.iter().map(|t| t.subject.to_string()).collect();
        assert_eq!(labels, ["_:anon2", "_:anon3", "_:anon1", "_:anon2"]);
    }

    #[test]
    fn quoted_triples_become_triple_hash_iris() {
        let quoted = Triple::new(iri("http://example.org/a"), iri("http://schema.org/knows"), iri("http://example.org/b"));
        let reifier = BlankNode::default();
        let since = iri("http://schema.org/startDate");
        let parsed = vec![
            Triple::new(reifier.clone(), vocab::RDF_REIFIES.into_owned(), Term::Triple(Box::new(quoted.clone()))),
            Triple::new(reifier, since.clone(), Literal::new_simple_literal("2020")),
            Triple::new(iri("http://example.org/c"), iri("http://schema.org/about"), Term::Triple(Box::new(quoted.clone()))),
        ];
        let claim = Claim::from_parsed(parsed, alice(), ClaimId::NONE, "http://example.org").unwrap();
        let hash_iri = TripleHash::of(&quoted).iri();
        assert_eq!(claim.triples(), &[
            Triple::new(iri("http://example.org/c"), iri("http://schema.org/about"), hash_iri.clone()),
            Triple::new(hash_iri, since, Literal::new_simple_literal("2020")),
        ]);
    }
}
