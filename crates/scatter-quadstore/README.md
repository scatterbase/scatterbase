# scatter-quadstore

The quad-store contract: a set of RDF quads that is searched by pattern and changed by deltas, with two backends.

The contract is two calls, on the `QuadStore` trait:

- **`lookup`** — the quads matching a `Pattern`, in which each of the subject, predicate, object and graph is either given or a wildcard;
- **`apply`** — a delta: the quads to delete, then the quads to insert. Applying the same delta twice leaves the store as applying it once does.

Access paths particular to what the quads mean are built on `lookup` by the crates that need them, and are not added to the contract.

The backends:

- **`MemoryStore`** keeps quads in memory in the order they were inserted, with an index on each of subject, predicate and object.
- **`NQuadsFile`** is a `MemoryStore` kept in an N-Quads file: opening reads the file, an insertion appends to it, and a deletion writes it again. A file whose quads are all in the default graph is also an N-Triples file.

One test checks the contract against both. RDF terms are [`oxrdf`](https://crates.io/crates/oxrdf)'s.

## Example

```rust
use oxrdf::{GraphName, Literal, NamedNode, Quad};
use scatter_quadstore::{MemoryStore, Pattern, QuadStore};

let ada = NamedNode::new("https://example.org/ada")?;
let name = NamedNode::new("http://schema.org/name")?;
let quad = Quad::new(ada.clone(), name.clone(), Literal::new_simple_literal("Ada Lovelace"), GraphName::DefaultGraph);

let mut store = MemoryStore::new();
store.apply(&[], &[quad.clone()])?;

// Everything about a subject, whatever the predicate, object and graph
assert_eq!(store.lookup(&Pattern::any().subject(&ada))?, [quad.clone()]);
// A subject and a predicate together
assert_eq!(store.lookup(&Pattern::any().subject(&ada).predicate(&name))?.len(), 1);

// A delta deletes, then inserts
let renamed = Quad::new(ada, name, Literal::new_simple_literal("Ada King"), GraphName::DefaultGraph);
store.apply(&[quad], &[renamed.clone()])?;
assert_eq!(store.lookup(&Pattern::any())?, [renamed]);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Status

Not published. The contract follows the one [Triplespace](https://github.com/scatterbase/triplespace)'s documentation describes for this crate. There is no Oxigraph backend and no SPARQL yet.

## License

GPL-3.0-or-later.
