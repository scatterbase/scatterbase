# Storage Trait Evolution Plan

The current `Storage` trait provides three methods: `init`, `append`, and `claim_exists`. This is a write-only interface with a single existence check — sufficient for the CLI prototype but insufficient for production use. This document outlines the planned expansion in phases tied to actual feature needs.

## Current Trait

```rust
pub trait Storage {
    fn init(&mut self, domain: &str) -> io::Result<()>;
    fn append(&mut self, triple: &Triple) -> io::Result<()>;
    fn claim_exists(&self, claim_hash: &str) -> io::Result<bool>;
}
```

## Phase 1: PostgreSQL Adapter (Immediate)

Add the minimum read and batch operations needed for the PostgreSQL adapter to be useful.

### New methods

- **`get_triples_by_subject(&self, subject: &str) -> io::Result<Vec<Triple>>`** — retrieve all triples for an entity; the core "look up an item" operation
- **`get_claim(&self, claim_hash: &str) -> io::Result<Option<Claim>>`** — retrieve the full claim envelope, not just check existence
- **`append_batch(&mut self, triples: &[Triple]) -> io::Result<()>`** — allow backends to optimize bulk inserts (PostgreSQL `COPY` / multi-row `INSERT`, SQLite transactions)

### Rationale

Without read operations, commands like `term latest` cannot go through the storage abstraction. Without batch operations, importing millions of Wikidata/OpenAlex triples means millions of individual `append` calls, each with a duplicate check.

## Phase 2: Librarybase Entity Support

Add targeted query methods needed for entity-centric workflows.

### New methods

- **`get_triples_by_predicate(&self, subject: &str, predicate: &str) -> io::Result<Vec<Triple>>`** — retrieve specific property values for an entity
- **`append_claim(&mut self, claim: &Claim) -> io::Result<()>`** — store a full signed claim envelope as a first-class object (not just its triples)
- **`get_claims_by_signer(&self, pubkey: &str) -> io::Result<Vec<Claim>>`** — find all claims from a specific signer

### Rationale

The current trait has no concept of a claim as a first-class object — claim metadata (signature, signer, prev_hash) doesn't flow through the storage trait at all. Librarybase's entity model requires looking up specific properties, and trust/provenance queries require signer-based lookups.

## Phase 3: Web UI / API Layer

Add pagination and counting for web-facing interfaces.

### New methods

- **`get_triples_by_subject_paged(&self, subject: &str, offset: u64, limit: u64) -> io::Result<Vec<Triple>>`** — paginated entity lookup
- **`count_triples(&self) -> io::Result<u64>`** — total triple count
- **`count_claims(&self) -> io::Result<u64>`** — total claim count

### Rationale

Any admin dashboard, API endpoint, or web UI needs pagination to avoid returning unbounded result sets, and basic statistics for monitoring.

## Phase 4: Trust Model

Add claim chain traversal for verifying provenance and trust.

### New methods

- **`get_claim_chain(&self, claim_hash: &str) -> io::Result<Vec<Claim>>`** — walk the `prev_hash` chain for a signer's history
- **`retract(&mut self, triple: &Triple) -> io::Result<()>`** — append a retraction claim (claims are immutable, but a new claim can assert a previous triple is retracted)
- Query methods that respect retractions (return only "current" triples)

### Rationale

Scatterbase's signed claim chains are a core differentiator. Without chain-walking methods, trust verification must be done outside the storage abstraction. Retractions are essential for any production system where corrections occur.

## Async Trait

The current trait is synchronous (`io::Result`). PostgreSQL access via `sqlx` is inherently async. The trait expansion is the natural time to introduce an `AsyncStorage` trait rather than bolting async onto a larger synchronous interface.

Recommended approach:

1. Phase 1: implement sync `Storage` with `block_on()` for the PostgreSQL adapter
2. Phase 2+: introduce `AsyncStorage` using the `async-trait` crate, migrate the codebase

```rust
#[async_trait]
pub trait AsyncStorage {
    async fn init(&mut self, domain: &str) -> io::Result<()>;
    async fn append(&mut self, triple: &Triple) -> io::Result<()>;
    async fn append_batch(&mut self, triples: &[Triple]) -> io::Result<()>;
    async fn get_triples_by_subject(&self, subject: &str) -> io::Result<Vec<Triple>>;
    // ... etc
}
```

## Design Principles

- **Additive only** — new methods are added; existing method signatures are not changed (until the async migration)
- **Feature-driven** — methods are added when a concrete feature requires them, not speculatively
- **Backend-agnostic** — all methods must be implementable by NtFileStorage, MockStorage, SQLite, and PostgreSQL (even if some backends are inefficient for certain operations)
- **Default implementations** — where possible, new trait methods should have default implementations to avoid breaking existing backends immediately
