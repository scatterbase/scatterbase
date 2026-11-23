# Wikibase Extension for Scatterbase

> **Status.** This note predates the crate refactor. The goal still holds, but the Wikibase model, RDF mapping and Wikidata import it proposes to write exist as the shared `scatter-wikibase-*`, `scatter-providers` and `scatter-ingest` crates in the triplespace repository, which this would be built on. See [architecture.md](architecture.md).

This document proposes a Scatterbase extension that implements the Wikibase ontology, allowing a Scatterbase instance to function as a Wikibase-compatible site — storing items, properties, lexemes, statements with qualifiers and references, ranks, sitelinks, and labels/descriptions/aliases in the same structural model used by Wikidata.

## Background

Wikibase is the software behind Wikidata. Its data model goes beyond plain RDF triples: every statement has a rank, optional qualifiers, and optional references. Labels, descriptions, and aliases are language-tagged. Sitelinks connect items to pages on external wikis. The Wikibase RDF ontology (prefix `wikibase:`) formalizes all of this.

Scatterbase already stores signed, provenanced RDF triples in claim envelopes. The goal of this extension is to layer the Wikibase structural conventions on top of Scatterbase's existing triple store and claim system, so that:

1. Wikibase-structured data can be authored, stored, and queried natively.
2. Data can be round-tripped with Wikidata RDF dumps without loss.
3. The Scatterbase provenance model (signed claims, prior-use acknowledgments) adds capabilities that upstream Wikibase lacks.

## Wikibase Ontology Recap

The key elements of the Wikibase data model, expressed in RDF:

### Entities

- **Items** (`Q` identifiers) — things in the world. RDF type: `wikibase:Item`.
- **Properties** (`P` identifiers) — attributes or relationships. RDF type: `wikibase:Property`. Each property has a datatype (`wikibase:propertyType`).
- **Lexemes** (`L` identifiers) — dictionary entries. RDF type: `wikibase:Lexeme`. Contain forms and senses.

### Statements

A statement is a claim about an entity. In Wikibase RDF, a statement is a reified node:

```turtle
wd:Q42 p:P31 wds:Q42-some-guid .          # entity → statement node
wds:Q42-some-guid ps:P31 wd:Q5 .           # statement → value (property-statement)
wds:Q42-some-guid wikibase:rank wikibase:NormalRank .
```

The property appears in multiple URI variants:
- `wdt:P31` — direct/truthy shortcut (only best-rank values)
- `p:P31` — entity-to-statement link
- `ps:P31` — statement-to-value
- `pq:P31` — qualifier usage
- `pr:P31` — reference usage
- `pqv:`, `psv:`, `prv:` — full value node variants (for quantities, coordinates, times)

### Qualifiers

Additional property–value pairs attached to a statement node:

```turtle
wds:Q42-some-guid pq:P580 "1952-03-11"^^xsd:dateTime .  # start date qualifier
```

### References

A statement can cite sources via reference nodes:

```turtle
wds:Q42-some-guid prov:wasDerivedFrom wdref:some-ref-hash .
wdref:some-ref-hash pr:P248 wd:Q36578 .   # stated in: Encyclopædia Britannica
```

### Ranks

Every statement has exactly one rank: `wikibase:PreferredRank`, `wikibase:NormalRank`, or `wikibase:DeprecatedRank`. The `wdt:` (direct/truthy) namespace only reflects preferred-rank values (or normal-rank if no preferred exists).

### Labels, Descriptions, Aliases

```turtle
wd:Q42 rdfs:label "Douglas Adams"@en .
wd:Q42 schema:description "English author and humourist"@en .
wd:Q42 skos:altLabel "Douglas Noël Adams"@en .
```

### Sitelinks

```turtle
wd:Q42 schema:about <https://en.wikipedia.org/wiki/Douglas_Adams> .
```

## Proposed Extension Architecture

Following the modularity strategy, this extension is a new crate: **`scatterbase-wikibase`**. It depends on `scatterbase-core` and implements/extends its trait interfaces.

```toml
[workspace]
members = [
    # ... existing crates ...
    "scatterbase-wikibase",   # Wikibase ontology extension
]
```

### Component 1: Wikibase Namespace Definitions

Register all Wikibase-related RDF namespaces. These build on the existing entity namespace registry (from the Librarybase proposal) and the standard RDF prefix system.

**Entity namespaces** (via entity-ns registry):

| Prefix | URI Pattern | Purpose |
|--------|-------------|---------|
| Q | `{base}/entity/Q{id}` | Items |
| P | `{base}/entity/P{id}` | Properties |
| L | `{base}/entity/L{id}` | Lexemes |

Where `{base}` is the site's base URI (e.g., `https://my-wiki.example.org`).

**RDF prefix namespaces** (standard prefix system):

| Prefix | URI Pattern | Purpose |
|--------|-------------|---------|
| `wd:` | `{base}/entity/` | Entity URIs |
| `wdt:` | `{base}/prop/direct/` | Direct/truthy property values |
| `p:` | `{base}/prop/` | Entity-to-statement links |
| `ps:` | `{base}/prop/statement/` | Statement-to-value |
| `pq:` | `{base}/prop/qualifier/` | Qualifier property usage |
| `pr:` | `{base}/prop/reference/` | Reference property usage |
| `psv:` | `{base}/prop/statement/value/` | Full value nodes (statement) |
| `pqv:` | `{base}/prop/qualifier/value/` | Full value nodes (qualifier) |
| `prv:` | `{base}/prop/reference/value/` | Full value nodes (reference) |
| `wds:` | `{base}/entity/statement/` | Statement node URIs |
| `wdref:` | `{base}/reference/` | Reference node URIs |
| `wdv:` | `{base}/value/` | Value nodes (quantities, times, coords) |
| `wikibase:` | `http://wikiba.se/ontology#` | Wikibase ontology terms |

**CLI setup** for a new Wikibase site:

```bash
scatterbase wikibase init --base-uri https://my-wiki.example.org
```

This single command registers all the above namespaces, sets the site base URI, and creates the foundational configuration.

### Component 2: Wikibase Data Types

Implement the Wikibase property datatype system. Each property has a declared type that constrains its values:

| Wikibase Type | RDF Representation |
|---|---|
| `WikibaseItem` | Object is an item URI (`wd:Q*`) |
| `String` | `xsd:string` literal |
| `MonolingualText` | Language-tagged literal |
| `ExternalId` | `xsd:string` with normalized URI |
| `Url` | `xsd:anyURI` |
| `Time` | Value node with `wikibase:timeValue`, `wikibase:timePrecision`, `wikibase:timeCalendarModel` |
| `Quantity` | Value node with `wikibase:quantityAmount`, `wikibase:quantityUnit`, optional bounds |
| `GlobeCoordinate` | Value node with `wikibase:geoLatitude`, `wikibase:geoLongitude`, `wikibase:geoPrecision`, `wikibase:geoGlobe` |
| `CommonsMedia` | URL to Wikimedia Commons file |

The extension provides:
- A `WikibaseDatatype` enum with serialization/deserialization
- Validation logic: when writing a statement, check that the value matches the property's declared type
- Value node construction: for complex types (Time, Quantity, GlobeCoordinate), automatically generate the full value node triples

### Component 3: Statement Builder

A high-level API for constructing Wikibase-structured statements, hiding the verbose RDF reification:

```rust
pub struct StatementBuilder {
    entity: EntityId,       // e.g., Q42
    property: EntityId,     // e.g., P31
    value: WikibaseValue,
    rank: Rank,             // Preferred, Normal, Deprecated
    qualifiers: Vec<(EntityId, WikibaseValue)>,
    references: Vec<Vec<(EntityId, WikibaseValue)>>,
}

impl StatementBuilder {
    pub fn new(entity: EntityId, property: EntityId, value: WikibaseValue) -> Self;
    pub fn rank(self, rank: Rank) -> Self;
    pub fn qualifier(self, property: EntityId, value: WikibaseValue) -> Self;
    pub fn reference(self, snaks: Vec<(EntityId, WikibaseValue)>) -> Self;
    pub fn build(self) -> Vec<Triple>;  // Expands to full Wikibase RDF triples
}
```

The `build()` method generates:
1. The entity-to-statement triple (`wd:Q42 p:P31 wds:Q42-{guid}`)
2. The statement-to-value triple (`wds:Q42-{guid} ps:P31 wd:Q5`)
3. The direct/truthy triple if applicable (`wd:Q42 wdt:P31 wd:Q5`)
4. The rank triple
5. Qualifier triples
6. Reference node triples
7. Value node triples for complex datatypes

All generated triples are then wrapped in a Scatterbase claim envelope via the existing `claim prepare` / `claim write` pipeline.

### Component 4: Statement-Aware CLI Commands

New CLI subcommands under `scatterbase wikibase`:

```bash
# Create a new item with labels
scatterbase wikibase create-item \
  --label en "Douglas Adams" \
  --label de "Douglas Adams" \
  --description en "English author and humourist" \
  --alias en "Douglas Noël Adams"

# Create a new property
scatterbase wikibase create-property \
  --label en "instance of" \
  --datatype WikibaseItem

# Add a statement with qualifiers and references
scatterbase wikibase add-statement Q42 P31 Q5 \
  --qualifier P580 "1952-03-11T00:00:00Z" \
  --reference P248 Q36578

# Set rank
scatterbase wikibase set-rank Q42 P31 Q5 preferred

# Add a sitelink
scatterbase wikibase add-sitelink Q42 enwiki "Douglas Adams"

# View an item in Wikibase-structured format
scatterbase wikibase show Q42
```

The `show` command reconstructs the Wikibase entity view from the underlying triples — grouping statements by property, displaying qualifiers and references inline, and showing labels/descriptions/aliases by language.

### Component 5: Truthy Triple Maintenance

The `wdt:` (direct/truthy) namespace is a materialized view: it reflects only the "best" value for each property on each entity (preferred-rank if any exist, otherwise normal-rank, never deprecated). This requires maintenance logic:

- **On statement write**: Recompute the truthy triple for the affected entity+property pair. If the new statement is preferred-rank, it replaces any existing truthy triple. If normal-rank and no preferred exists, it becomes the truthy value.
- **On rank change**: Recompute truthy triples for the affected entity+property.
- **On statement removal** (via retraction claim): Recompute truthy triples.

This is implemented as an `EventListener` (from the modularity plan) that reacts to `TripleAppended` events involving statement nodes.

### Component 6: Wikidata Import Adapter

Tooling to import data from Wikidata (or any Wikibase instance) into a Scatterbase-backed Wikibase site:

```bash
# Import from a Wikidata RDF dump
scatterbase wikibase import --source wikidata-dump.ttl.gz

# Import specific entities from the Wikidata API
scatterbase wikibase import --source wikidata --entities Q42,Q5,P31

# Import with URI rewriting (foreign Wikidata → local site)
scatterbase wikibase import --source wikidata-dump.ttl.gz --rewrite-base https://my-wiki.example.org
```

The import adapter:
- Parses Wikibase RDF and reconstructs the statement structure
- Optionally rewrites URIs from the source base to the local base
- Wraps all imported data in signed claim envelopes, preserving the provenance chain (the claim records that the data originated from an external Wikibase)
- Handles incremental imports by checking which entities have changed

For foreign references (keeping Wikidata URIs without rewriting), the entity namespace registry from the Librarybase proposal is used: `WDQ42` refers to Wikidata's Q42, while `Q42` refers to the local site's Q42.

### Component 7: Wikibase Query Compatibility

Extend the planned `QueryEngine` trait to support Wikibase-idiomatic query patterns:

- **Entity lookup**: Given `Q42`, return all statements grouped by property with qualifiers and references
- **Property value lookup**: Given `Q42` and `P31`, return all values with their ranks
- **Truthy queries**: Query only `wdt:` triples for simple "what is the current best value" lookups
- **Reverse lookup**: Given `P31 = Q5`, find all entities that are instances of Q5
- **Label service**: Resolve entity IDs to human-readable labels in a requested language, with fallback chains (e.g., `en` → `en-gb` → `mul`)

These query patterns map to the underlying triple store but require the extension to understand the Wikibase statement structure rather than treating everything as flat triples.

## Interaction with Scatterbase Provenance

The Wikibase extension gains significant capabilities from Scatterbase's claim envelope system that upstream Wikibase lacks:

1. **Cryptographic provenance**: Every statement addition, rank change, or label edit is a signed claim. You can verify exactly who asserted "Q42 P31 Q5" and when.

2. **Multi-source trust**: Different signers can make conflicting statements about the same entity. The trust model determines which claims are visible — unlike Wikibase, where the latest edit wins globally.

3. **Federated editing**: Multiple Scatterbase nodes can independently add statements to the same entity. Network synchronization (from the roadmap) merges these without the single-master bottleneck of MediaWiki.

4. **Retraction without deletion**: Wikibase deletes revisions; Scatterbase appends retraction claims. The full history is always available for audit.

## Priority Order

1. **Wikibase namespace definitions** — foundational; all other components reference these URIs
2. **Wikibase data types** — needed before statements can be validated
3. **Statement builder** — the core API; everything else builds on structured statement creation
4. **Statement-aware CLI** — makes the system usable for manual data entry and inspection
5. **Truthy triple maintenance** — required for correct Wikibase query semantics
6. **Wikidata import adapter** — enables bootstrapping from existing Wikibase data
7. **Wikibase query compatibility** — enables useful structured queries over the data

## Dependencies on Other Scatterbase Work

| Dependency | Status | Required By |
|---|---|---|
| Entity namespace registry | Proposed (Librarybase doc) | Namespace definitions, import adapter |
| Compact entity ID parser | Proposed (Librarybase doc) | CLI commands, statement builder |
| Cargo workspace restructure | Proposed (modularity doc) | Extension crate structure |
| `EventListener` hooks | Proposed (modularity doc) | Truthy triple maintenance |
| `QueryEngine` trait | Proposed (modularity doc) | Query compatibility |
| Storage trait Phase 1 | Proposed (storage-evolution doc) | All read operations |
| Storage trait Phase 2 | Proposed (storage-evolution doc) | Entity-centric queries |
| Retraction support | Proposed (storage-evolution Phase 4) | Statement removal, rank changes |
