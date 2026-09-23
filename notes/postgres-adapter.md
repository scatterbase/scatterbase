# PostgreSQL Storage Adapter

This document describes the design for a PostgreSQL implementation of the Scatterbase `Storage` trait, intended for MediaWiki-like deployments where multiple users read and write concurrently through a web-facing service.

## Motivation

Scatterbase currently ships two `Storage` implementations:

- **`NtFileStorage`** — appends triples to a flat `.nt` file; `claim_exists` requires a full file scan
- **`MockStorage`** — in-memory store for testing

Neither supports concurrent access. A PostgreSQL backend provides:

- **Concurrent writers** via MVCC (no single-writer lock)
- **Indexed lookups** for `claim_exists` and future query operations
- **Connection pooling** for web application servers (Actix, Axum, etc.)
- **Scalability** to millions of triples (important for Librarybase's Wikidata and OpenAlex data)

## Storage Trait

The adapter must implement the existing trait defined in `src/storage.rs`. The trait is planned to expand in phases (see [storage-evolution.md](storage-evolution.md) for the full plan). The initial PostgreSQL adapter implements the current trait plus the Phase 1 additions:

```rust
// Current trait
pub trait Storage {
    fn init(&mut self, domain: &str) -> io::Result<()>;
    fn append(&mut self, triple: &Triple) -> io::Result<()>;
    fn claim_exists(&self, claim_hash: &str) -> io::Result<bool>;
}

// Phase 1 additions (immediate, required for the adapter to be useful)
//   get_triples_by_subject(&self, subject: &str) -> io::Result<Vec<Triple>>
//   get_claim(&self, claim_hash: &str) -> io::Result<Option<Claim>>
//   append_batch(&mut self, triples: &[Triple]) -> io::Result<()>
```

## Database Schema

Four tables capture the full Scatterbase data model: claims (signed envelopes), triples (user-submitted data assertions), acknowledgments (prior-use provenance records), and domains (initialization metadata).

```sql
-- Signed claim envelopes
CREATE TABLE claims (
    id              BIGSERIAL PRIMARY KEY,
    claim_hash      TEXT UNIQUE NOT NULL,
    prev_hash       TEXT,
    signer_pubkey   TEXT NOT NULL,
    attribution_uri TEXT NOT NULL,       -- prov:wasAttributedTo URI (user identity)
    signature       TEXT NOT NULL,
    domain          TEXT NOT NULL,       -- the domain/server this claim belongs to
    generated_at    TIMESTAMPTZ NOT NULL, -- prov:generatedAtTime (signed canonical timestamp)
    seq_id          TEXT NOT NULL,       -- scatter:seqID for ordering
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()  -- DB insertion time
);

CREATE INDEX idx_claims_signer ON claims(signer_pubkey);
CREATE INDEX idx_claims_prev ON claims(prev_hash);
CREATE INDEX idx_claims_attribution ON claims(attribution_uri);
CREATE INDEX idx_claims_domain ON claims(domain);
CREATE INDEX idx_claims_seq ON claims(seq_id);

-- User-submitted triples (the actual data assertions)
CREATE TABLE triples (
    id          BIGSERIAL PRIMARY KEY,
    claim_id    BIGINT REFERENCES claims(id) NOT NULL,
    triple_hash TEXT UNIQUE NOT NULL,    -- content-addressable hash (scatter:triple/<hash>)
    subject     TEXT NOT NULL,
    predicate   TEXT NOT NULL,
    object      TEXT NOT NULL
);

CREATE INDEX idx_triples_hash ON triples(triple_hash);
CREATE INDEX idx_triples_subject ON triples(subject);
CREATE INDEX idx_triples_predicate ON triples(predicate);
CREATE INDEX idx_triples_object ON triples(object);
CREATE INDEX idx_triples_spo ON triples(subject, predicate, object);
CREATE INDEX idx_triples_claim ON triples(claim_id);

-- Prior-use acknowledgments (one per subject/object URI per claim)
CREATE TABLE acknowledgments (
    id               BIGSERIAL PRIMARY KEY,
    claim_id         BIGINT REFERENCES claims(id) NOT NULL,
    ack_hash         TEXT UNIQUE NOT NULL,  -- scatter:ack/<claimHash>/term/<termHash>
    term_uri         TEXT NOT NULL,         -- the subject or object URI being acknowledged
    prior_claim_hash TEXT NOT NULL          -- the prior claim hash for this term
);

CREATE INDEX idx_ack_claim ON acknowledgments(claim_id);
CREATE INDEX idx_ack_term ON acknowledgments(term_uri);
CREATE INDEX idx_ack_prior ON acknowledgments(prior_claim_hash);

-- Domain metadata (stores the domain passed to init)
CREATE TABLE domains (
    id          BIGSERIAL PRIMARY KEY,
    domain      TEXT UNIQUE NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Schema rationale

The three core tables (`claims`, `triples`, `acknowledgments`) map directly to the three kinds of data a `ClaimEnvelope` produces:

1. **`claims`** — the signed envelope metadata (hash, signature, attribution, prev_hash chain, timestamps)
2. **`triples`** — the user-submitted data assertions, each with a content-addressable `triple_hash`
3. **`acknowledgments`** — prior-use records that form the provenance chain for Scatterbase's trust model

The `ClaimEnvelope.to_triples()` method also generates RDF reification triples (`rdf:subject`, `rdf:predicate`, `rdf:object`), type assertions (`rdf:type scatter:Triple`, `rdf:type rdf:Statement`, `rdf:type scatter:SignedClaim`), and relationship triples (`scatter:claimedIn`, `scatter:includes`). These are **derivable** from the structured columns and do not need their own tables — they can be reconstructed on demand for RDF export or SPARQL-compatible querying.

## Struct Design

```rust
use sqlx::PgPool;

pub struct PostgresStorage {
    pool: PgPool,
    domain: Option<String>,
}

impl PostgresStorage {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPool::connect(database_url).await?;
        Ok(PostgresStorage {
            pool,
            domain: None,
        })
    }
}
```

## Trait Method Implementations

### `init`

- Store the domain in `self.domain`
- Run migrations (create tables and indexes if they do not exist) using `sqlx::migrate!()` or raw `CREATE TABLE IF NOT EXISTS` statements
- This replaces the filesystem directory/file creation in `NtFileStorage`

### `append`

- Insert the triple into the `triples` table with its `triple_hash`
- Deduplicate: use `INSERT ... ON CONFLICT (triple_hash) DO NOTHING`, mirroring `NtFileStorage`'s behavior of silently dropping duplicate triples
- If claim metadata is available, insert into `claims` first, then reference the `claim_id` on the triple row
- Insert corresponding acknowledgment rows into the `acknowledgments` table

### `claim_exists`

- Query: `SELECT 1 FROM claims WHERE claim_hash = $1 LIMIT 1`
- The `UNIQUE` index on `claim_hash` makes this O(log n) instead of the full file scan in `NtFileStorage`

### `get_triples_by_subject` (Phase 1)

- Query: `SELECT subject, predicate, object FROM triples WHERE subject = $1`
- Uses the `idx_triples_subject` index
- Returns results as `Vec<Triple>`

### `get_claim` (Phase 1)

- Query: `SELECT * FROM claims WHERE claim_hash = $1`
- Also fetch associated triples: `SELECT * FROM triples WHERE claim_id = $1`
- Also fetch associated acknowledgments: `SELECT * FROM acknowledgments WHERE claim_id = $1`
- Returns the full claim envelope including signature, signer, attribution, prev_hash, domain, timestamps, and all associated triples and acknowledgments

### `append_batch` (Phase 1)

- Use a transaction wrapping multi-row `INSERT` or PostgreSQL `COPY` for bulk loading
- Insert claims, triples, and acknowledgments together within the transaction
- Critical for Wikidata/OpenAlex imports (millions of triples)
- Deduplicate within the batch and against existing data using `ON CONFLICT DO NOTHING`

## Async Considerations

The current `Storage` trait uses synchronous `io::Result` return types. PostgreSQL access via `sqlx` is async. There are two approaches:

1. **Block on async** — use `tokio::runtime::Handle::current().block_on()` inside the sync trait methods. This works but is not ideal for a web server context.
2. **Introduce an async trait** — define an `AsyncStorage` trait (or use the `async-trait` crate) and migrate the codebase. This is the cleaner long-term path but requires broader changes.

For an initial implementation, approach (1) is pragmatic. The trait can be evolved to async later.

## Crate Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres"] }
tokio = { version = "1", features = ["full"] }
```

## Configuration

The adapter needs a PostgreSQL connection string. This could be provided via:

- A `--database-url` CLI flag
- An environment variable (`SCATTERBASE_DATABASE_URL` or `DATABASE_URL`)
- A configuration file entry

The `create_storage` factory function in `src/storage.rs` would gain a branch for PostgreSQL:

```rust
fn create_storage(config: &Config) -> Box<dyn Storage> {
    match config.storage_backend {
        StorageBackend::NtFile(path) => Box::new(NtFileStorage::new(&path)),
        StorageBackend::Postgres(url) => Box::new(PostgresStorage::new_blocking(&url)),
    }
}
```

## Triple Generation

The structured tables store claim data in decomposed form. The full RDF graph (as seen in `.nt` files) can be reconstructed by:

1. **User triples** — read directly from the `triples` table
2. **Claim metadata triples** — generated from `claims` rows: `rdf:type scatter:SignedClaim`, `prov:wasAttributedTo`, `scatter:previousClaim`, `scatter:userSignature`, `prov:generatedAtTime`, `scatter:seqID`
3. **Triple-to-claim links** — generated from `triples` rows: `<tripleHash> scatter:claimedIn <claimHash>`, `<claimHash> scatter:includes <tripleHash>`, plus reification triples (`rdf:subject`, `rdf:predicate`, `rdf:object`) and type assertions (`rdf:type scatter:Triple`, `rdf:type rdf:Statement`)
4. **Acknowledgment triples** — generated from `acknowledgments` rows: `<ackHash> scatter:term <termUri>`, `<ackHash> scatter:priorClaim <priorClaimHash>`, `<claimHash> scatter:acknowledgesPriorUse <ackHash>`

The recommended approach is to read rows into Rust structs and call `ClaimEnvelope::to_triples()` (or an equivalent reconstruction method) rather than assembling N-Triples in SQL.

## Testing

- Use a dedicated test database or `sqlx`'s test support (`#[sqlx::test]`)
- The existing `MockStorage` tests establish the expected behavior; the PostgreSQL adapter should pass equivalent assertions
- Integration tests should verify concurrent writes do not produce duplicates or errors

## Future Extensions

As the `Storage` trait evolves (see [storage-evolution.md](storage-evolution.md)), the PostgreSQL adapter will gain additional capabilities in later phases:

- **Phase 2** — `get_triples_by_predicate`, `append_claim`, `get_claims_by_signer` for Librarybase entity and provenance workflows
- **Phase 3** — paginated queries and count operations for the web UI / API layer
- **Phase 4** — claim chain traversal (`get_claim_chain`) and retraction support for the trust model
- **AsyncStorage trait** — migrate from sync `block_on()` wrapper to a native `AsyncStorage` trait (planned for Phase 2+)

PostgreSQL-specific extensions beyond the trait:

- **Full-text search** on triple objects using PostgreSQL's `tsvector`/`tsquery`
- **JSONB column** on `claims` to store the full signed claim envelope for raw preservation
- **Read replicas** for scaling query-heavy workloads
- **SPARQL translation** layer that compiles SPARQL queries to SQL against the triples table
