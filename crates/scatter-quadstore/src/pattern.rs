//! What a lookup asks for.

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNodeRef, Quad, TermRef};

/// A quad with wildcards: each position is either a term the quad must have there or
/// `None`, which any term matches.
///
/// The graph is a position like the others: `None` matches a quad in any graph, and
/// [`GraphNameRef::DefaultGraph`] only a quad in the default graph.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pattern<'a> {
    /// The subject, or any.
    pub subject: Option<NamedOrBlankNodeRef<'a>>,
    /// The predicate, or any.
    pub predicate: Option<NamedNodeRef<'a>>,
    /// The object, or any.
    pub object: Option<TermRef<'a>>,
    /// The graph, or any.
    pub graph: Option<GraphNameRef<'a>>,
}

impl<'a> Pattern<'a> {
    /// The pattern every quad matches.
    pub fn any() -> Self {
        Self::default()
    }

    /// The pattern only this quad matches.
    pub fn of(quad: &'a Quad) -> Self {
        Self {
            subject: Some(quad.subject.as_ref()),
            predicate: Some(quad.predicate.as_ref()),
            object: Some(quad.object.as_ref()),
            graph: Some(quad.graph_name.as_ref()),
        }
    }

    /// This pattern with the subject given.
    pub fn subject(mut self, subject: impl Into<NamedOrBlankNodeRef<'a>>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// This pattern with the predicate given.
    pub fn predicate(mut self, predicate: impl Into<NamedNodeRef<'a>>) -> Self {
        self.predicate = Some(predicate.into());
        self
    }

    /// This pattern with the object given.
    pub fn object(mut self, object: impl Into<TermRef<'a>>) -> Self {
        self.object = Some(object.into());
        self
    }

    /// This pattern with the graph given.
    pub fn graph(mut self, graph: impl Into<GraphNameRef<'a>>) -> Self {
        self.graph = Some(graph.into());
        self
    }

    /// Whether a quad matches.
    pub fn matches(&self, quad: &Quad) -> bool {
        self.subject.is_none_or(|subject| subject == quad.subject.as_ref())
            && self.predicate.is_none_or(|predicate| predicate == quad.predicate.as_ref())
            && self.object.is_none_or(|object| object == quad.object.as_ref())
            && self.graph.is_none_or(|graph| graph == quad.graph_name.as_ref())
    }
}
