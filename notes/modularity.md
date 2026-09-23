# Modularity Strategy

Scatterbase is designed as a **kernel** — a minimal core that other features (entity namespaces, authentication, views of the underlying data, etc.) plug into as expansions or modules.

## Cargo Workspace

Restructure the repository as a Cargo workspace where the kernel is one crate and each module/expansion is a separate crate:

```toml
[workspace]
members = [
    "scatterbase-core",       # kernel: Triple, Term, Uri, Claim, Storage trait, crypto
    "scatterbase-storage-nt", # N-Triples file backend
    "scatterbase-storage-pg", # Postgres backend
    "scatterbase-auth",       # Authentication expansion
    "scatterbase-namespaces", # Entity namespace expansion
    "scatterbase-views",      # Data view expansion
    "scatterbase-cli",        # CLI binary
]
```

`scatterbase-core` contains core types (`Uri`, `Term`, `Triple`, `Literal`, `Claim`), trait interfaces (`Storage`, etc.), claim envelope logic, crypto, constants, and prefix definitions. Each expansion crate depends on `scatterbase-core` and implements or extends its traits.

## Trait-Based Extension Points

The existing `Storage` trait is the model. Apply the same pattern to other concerns:

- **`Storage`** — storage backend (already exists)
- **`AuthProvider`** — identity verification and claim signing
- **`NamespaceResolver`** — prefix registration and resolution
- **`QueryEngine`** — triple pattern queries and views

Modules implement these traits; the kernel only defines them.

## Runtime Composition via a Kernel Struct

A central `ScatterbaseKernel` struct holds trait objects, allowing modules to be plugged in at startup:

```rust
pub struct ScatterbaseKernel {
    pub storage: Box<dyn Storage>,
    pub auth: Option<Box<dyn AuthProvider>>,
    pub namespaces: Option<Box<dyn NamespaceResolver>>,
    pub query: Option<Box<dyn QueryEngine>>,
}
```

The CLI or server binary wires these together. Optional modules are `Option`-wrapped so the kernel runs with only what's installed.

## Cargo Features for Compile-Time Selection

For tightly coupled or performance-sensitive modules, use Cargo features so users compile only what they need:

```toml
[features]
default = ["storage-nt"]
storage-nt = []
storage-postgres = ["tokio", "sqlx"]
auth-ed25519 = []
```

## Event Hooks for Cross-Module Communication

Define lifecycle events so modules can react without tight coupling:

```rust
pub enum KernelEvent<'a> {
    ClaimPrepared(&'a ClaimEnvelope),
    ClaimSigned(&'a SignedClaim),
    TripleAppended(&'a Triple),
    StorageInitialized(&'a str),
}

pub trait EventListener {
    fn on_event(&self, event: &KernelEvent) -> Result<()>;
}
```

Modules register listeners; the kernel fires events at key points. This is how a "views" module could maintain indexes without the kernel knowing about it.

## Architecture Summary

| Layer | Contents | Crate |
|---|---|---|
| **Core types** | `Uri`, `Term`, `Triple`, `Literal`, `Claim` | `scatterbase-core` |
| **Trait interfaces** | `Storage`, `AuthProvider`, `NamespaceResolver`, `QueryEngine`, `EventListener` | `scatterbase-core` |
| **Storage backends** | `NtFileStorage`, Postgres | `scatterbase-storage-*` |
| **Auth modules** | Ed25519 signing/verification | `scatterbase-auth` |
| **Namespace modules** | Entity namespace management | `scatterbase-namespaces` |
| **View modules** | Query engines, indexes | `scatterbase-views` |
| **Binary** | CLI, wiring, config | `scatterbase-cli` |
