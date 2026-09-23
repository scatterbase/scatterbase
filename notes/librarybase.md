# Librarybase Migration to Scatterbase

Librarybase is a bibliographic metadata database aiming to be the comprehensive source for "everything". It incorporates structured data from external sources including Wikidata and OpenAlex. This document outlines the work required to support Librarybase on Scatterbase.

## Background

Librarybase-native entities use standard P (property) and Q (item) identifiers. Foreign entities from directly incorporated databases use three-letter prefixes:

- **WDQ** — Wikidata item (e.g., `WDQ42` → `<http://www.wikidata.org/entity/Q42>`)
- **WDP** — Wikidata property (e.g., `WDP31` → `<http://www.wikidata.org/entity/P31>`)
- **OAW** — OpenAlex Work (e.g., `OAW2345` → `<https://openalex.org/W2345>`)
- **OAF** — OpenAlex Funder (e.g., `OAF456` → `<https://openalex.org/F456>`)

These foreign entity prefixes are assigned sparingly, only when a foreign database is directly incorporated into Librarybase. This allows direct reference to existing entities without forced reification of millions of alias URIs.

## What Already Exists in Scatterbase

- RDF triple storage and N-Triples format
- Signed claim envelopes with provenance, timestamps, and ED25519 signatures
- Batch import from `.nt`/`.ttl` files (`claim write --file`)
- RDF prefix management (`scatterbase prefix add`)
- Term lookup (`scatterbase term latest`)
- Key management

## Work Required

### 1. Entity Namespace Registry

Scatterbase has RDF prefix management (`schema:name` → `http://schema.org/name`), but this is standard RDF namespace shorthand. Librarybase needs a distinct entity namespace registry that maps three-letter prefixes to URI expansion patterns.

The key structural difference: the prefix *includes* the entity-type letter (the `Q` in `WDQ`), and the local part is purely numeric (`WDQ42`, not `WDQ:Q42`).

Registry entries would look like:

| Prefix | URI Pattern |
|--------|-------------|
| WDQ | `http://www.wikidata.org/entity/Q{id}` |
| WDP | `http://www.wikidata.org/entity/P{id}` |
| OAW | `https://openalex.org/W{id}` |
| OAF | `https://openalex.org/F{id}` |

CLI interface:

```bash
scatterbase entity-ns add WDQ http://www.wikidata.org/entity/Q{id}
scatterbase entity-ns add WDP http://www.wikidata.org/entity/P{id}
scatterbase entity-ns add OAW https://openalex.org/W{id}
scatterbase entity-ns add OAF https://openalex.org/F{id}
```

### 2. Compact Entity Identifier Parser and Resolver

Logic to recognize and resolve compact identifiers bidirectionally:

- **Expansion**: `WDQ42` → `<http://www.wikidata.org/entity/Q42>`
- **Compaction**: `<http://www.wikidata.org/entity/Q42>` → `WDQ42`

The parser must:

- Separate the three-letter prefix from the numeric suffix
- Look up the prefix in the entity namespace registry
- Expand to the full canonical URI (or compact from it)
- Recognize native Librarybase identifiers (`Q42`, `P31`) as using the existing single-letter scheme without a foreign prefix

### 3. Entity-Aware CLI Commands

Extend existing commands to accept compact entity identifiers in all input positions (subject, predicate, object):

```bash
scatterbase term latest WDQ42           # look up Wikidata item Q42
scatterbase triple write WDQ42 P31 WDQ5 # native property, foreign entities
scatterbase claim prepare --file triples.nt  # already works with full URIs
```

Commands that need compact ID support: `triple write`, `claim prepare`, `claim write`, `term latest`.

### 4. Foreign Entity Import Pipeline

Tooling to bulk-fetch and ingest entities from external sources:

- **Wikidata**: Fetch from RDF dumps, SPARQL endpoint, or entity API; map URIs to `WDQ`/`WDP` identifiers
- **OpenAlex**: Fetch from API or data snapshots; map URIs to `OAW`/`OAF` identifiers
- Wrap imported data in signed claim envelopes (already works via `claim write --file`)
- Handle incremental updates (detect what has changed since last import)

### 5. Advanced Querying (Planned, Not Built)

Already listed in the Scatterbase roadmap. For a bibliographic metadata database, this is essential — queries need to work across native and foreign entities, e.g., "find all Librarybase works (`Q*`) that cite a Wikidata entity (`WDQ*`) authored by an OpenAlex-tracked funder (`OAF*`)."

### 6. Network Synchronization (Planned, Not Built)

Also listed in the Scatterbase roadmap. Required for incorporating external sources that update over time, whether through periodic re-import or a more sophisticated P2P protocol.

## Priority Order

1. Entity namespace registry — foundational, everything else depends on it
2. Compact entity ID parser/resolver — needed before CLI integration
3. Entity-aware CLI commands — makes the system usable for Librarybase workflows
4. Foreign entity import pipeline — enables actual data ingestion
5. Advanced querying — enables useful cross-source queries
6. Network synchronization — enables ongoing updates from external sources
