//! The in-memory backend.

use std::collections::HashMap;
use std::convert::Infallible;

use oxrdf::Quad;

use crate::{Pattern, QuadStore};

/// A quad store in memory.
///
/// Quads are kept in the order they were first inserted, and a lookup returns them in that
/// order. Each of subject, predicate and object is indexed, so a lookup with any of them
/// given reads only the quads that share the rarest of the given terms.
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    /// Every quad ever inserted, by position; `None` once deleted.
    slots: Vec<Option<Quad>>,
    /// The position of each quad held.
    positions: HashMap<Quad, usize>,
    /// The positions of the quads with each subject, predicate and object, by the term's
    /// N-Triples text. A position stays listed after its quad is deleted.
    by_subject: HashMap<String, Vec<usize>>,
    by_predicate: HashMap<String, Vec<usize>>,
    by_object: HashMap<String, Vec<usize>>,
}

impl MemoryStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// A store of the given quads.
    pub fn from_quads(quads: impl IntoIterator<Item = Quad>) -> Self {
        let mut store = Self::new();
        for quad in quads {
            store.insert(quad);
        }
        store
    }

    /// The quads that match the pattern, in insertion order.
    pub fn matching(&self, pattern: &Pattern<'_>) -> Vec<Quad> {
        // The shortest of the index lists for the terms the pattern gives
        let candidates = [
            pattern.subject.map(|s| self.by_subject.get(&s.to_string())),
            pattern.predicate.map(|p| self.by_predicate.get(&p.to_string())),
            pattern.object.map(|o| self.by_object.get(&o.to_string())),
        ]
        .into_iter()
        .flatten()
        .map(|list| list.map_or(&[][..], Vec::as_slice))
        .min_by_key(|list| list.len());

        let matching = |position: &usize| {
            self.slots[*position].as_ref().filter(|quad| pattern.matches(quad)).cloned()
        };
        match candidates {
            Some(positions) => positions.iter().filter_map(matching).collect(),
            None => (0..self.slots.len()).filter_map(|position| matching(&position)).collect(),
        }
    }

    /// Every quad held, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &Quad> {
        self.slots.iter().flatten()
    }

    /// The number of quads held.
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Whether the store holds no quads.
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Whether the store holds the quad.
    pub fn contains(&self, quad: &Quad) -> bool {
        self.positions.contains_key(quad)
    }

    /// Inserts a quad. Returns whether the store did not hold it.
    pub fn insert(&mut self, quad: Quad) -> bool {
        if self.positions.contains_key(&quad) {
            return false;
        }
        let position = self.slots.len();
        self.by_subject.entry(quad.subject.to_string()).or_default().push(position);
        self.by_predicate.entry(quad.predicate.to_string()).or_default().push(position);
        self.by_object.entry(quad.object.to_string()).or_default().push(position);
        self.positions.insert(quad.clone(), position);
        self.slots.push(Some(quad));
        true
    }

    /// Deletes a quad. Returns whether the store held it.
    pub fn remove(&mut self, quad: &Quad) -> bool {
        match self.positions.remove(quad) {
            Some(position) => {
                self.slots[position] = None;
                true
            },
            None => false,
        }
    }
}

impl QuadStore for MemoryStore {
    type Error = Infallible;

    fn lookup(&self, pattern: &Pattern<'_>) -> Result<Vec<Quad>, Infallible> {
        Ok(self.matching(pattern))
    }

    fn apply(&mut self, deleted: &[Quad], inserted: &[Quad]) -> Result<(), Infallible> {
        for quad in deleted {
            self.remove(quad);
        }
        for quad in inserted {
            self.insert(quad.clone());
        }
        Ok(())
    }

    fn len(&self) -> Result<u64, Infallible> {
        Ok(self.positions.len() as u64)
    }

    fn contains(&self, quad: &Quad) -> Result<bool, Infallible> {
        Ok(MemoryStore::contains(self, quad))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use oxrdf::{GraphName, GraphNameRef, Literal, NamedNode, NamedNodeRef, Term};

    pub(crate) fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    pub(crate) fn quad(s: &str, p: &str, o: impl Into<Term>) -> Quad {
        Quad::new(iri(s), iri(p), o, GraphName::DefaultGraph)
    }

    /// Five quads about `a` and `b`, the last in a named graph.
    pub(crate) fn sample() -> Vec<Quad> {
        vec![
            quad("http://example.org/a", "http://schema.org/name", Literal::new_simple_literal("A")),
            quad("http://example.org/a", "http://schema.org/knows", iri("http://example.org/b")),
            quad("http://example.org/b", "http://schema.org/name", Literal::new_simple_literal("B")),
            quad("http://example.org/b", "http://schema.org/knows", iri("http://example.org/a")),
            Quad::new(
                iri("http://example.org/a"),
                iri("http://schema.org/name"),
                Literal::new_simple_literal("Ay"),
                iri("http://example.org/graph"),
            ),
        ]
    }

    /// What any backend must do. `store` is empty.
    pub(crate) fn contract<S: QuadStore>(mut store: S) {
        let quads = sample();
        let a = NamedNodeRef::new_unchecked("http://example.org/a");
        let name = NamedNodeRef::new_unchecked("http://schema.org/name");
        let knows = NamedNodeRef::new_unchecked("http://schema.org/knows");
        let graph = NamedNodeRef::new_unchecked("http://example.org/graph");

        assert!(store.is_empty().unwrap());
        store.apply(&[], &quads).unwrap();
        assert_eq!(store.len().unwrap(), 5);
        assert!(store.contains(&quads[0]).unwrap());

        // Each position alone, in insertion order
        assert_eq!(store.lookup(&Pattern::any()).unwrap(), quads);
        assert_eq!(store.lookup(&Pattern::any().subject(a)).unwrap(), [quads[0].clone(), quads[1].clone(), quads[4].clone()]);
        assert_eq!(store.lookup(&Pattern::any().predicate(knows)).unwrap(), [quads[1].clone(), quads[3].clone()]);
        assert_eq!(store.lookup(&Pattern::any().object(a)).unwrap(), [quads[3].clone()]);
        assert_eq!(store.lookup(&Pattern::any().graph(graph)).unwrap(), [quads[4].clone()]);

        // Positions together; the default graph is a graph, not a wildcard
        assert_eq!(store.lookup(&Pattern::any().subject(a).predicate(name)).unwrap(), [quads[0].clone(), quads[4].clone()]);
        assert_eq!(
            store.lookup(&Pattern::any().subject(a).predicate(name).graph(GraphNameRef::DefaultGraph)).unwrap(),
            [quads[0].clone()],
        );
        assert_eq!(store.lookup(&Pattern::of(&quads[2])).unwrap(), [quads[2].clone()]);
        assert!(store.lookup(&Pattern::any().subject(NamedNodeRef::new_unchecked("http://example.org/none"))).unwrap().is_empty());

        // Inserting what is held and deleting what is not change nothing
        store.apply(&[quad("http://example.org/none", "http://schema.org/name", iri("http://example.org/a"))], &quads[..2]).unwrap();
        assert_eq!(store.lookup(&Pattern::any()).unwrap(), quads);

        // A delta deletes, then inserts
        let added = quad("http://example.org/c", "http://schema.org/knows", iri("http://example.org/a"));
        store.apply(&[quads[1].clone(), quads[4].clone()], std::slice::from_ref(&added)).unwrap();
        assert_eq!(store.len().unwrap(), 4);
        assert!(!store.contains(&quads[1]).unwrap());
        assert_eq!(store.lookup(&Pattern::any().subject(a)).unwrap(), [quads[0].clone()]);
        assert_eq!(store.lookup(&Pattern::any().object(a)).unwrap(), [quads[3].clone(), added.clone()]);

        // A quad in both lists is held afterwards, and a deleted quad can come back
        store.apply(std::slice::from_ref(&added), &[added.clone(), quads[1].clone()]).unwrap();
        assert!(store.contains(&added).unwrap());
        assert_eq!(store.lookup(&Pattern::any().predicate(knows)).unwrap().len(), 3);
    }

    #[test]
    fn memory_store_keeps_the_contract() {
        contract(MemoryStore::new());
    }

    #[test]
    fn from_quads_drops_duplicates_and_keeps_order() {
        let quads = sample();
        let store = MemoryStore::from_quads([quads[1].clone(), quads[0].clone(), quads[1].clone()]);
        assert_eq!(store.len(), 2);
        assert_eq!(store.iter().cloned().collect::<Vec<_>>(), [quads[1].clone(), quads[0].clone()]);
    }
}
