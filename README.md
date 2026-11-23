# Scatterbase

Scatterbase is a Rust-based CLI tool for managing RDF triples, signed claims, and cryptographic identities. It provides a decentralized way to store and verify information using linked data and digital signatures.

## Overview

Scatterbase allows users to create, sign, and store "claims" about RDF triples. These claims are attributed to specific identities (represented by URIs) and signed using ED25519 cryptography. The tool uses N-Triples as its primary storage format and supports both N-Triples and Turtle for input/output of claims.

## Key Features

- **RDF Triple Management**: Create and store subject-predicate-object triples.
- **Signed Claims**: Group triples into signed "envelopes" that provide attribution, timestamps, and cryptographic proof of origin.
- **Identity & Keys**: Generate and manage ED25519 key pairs for signing claims.
- **URI Prefix Support**: Use short prefixes (e.g., `schema:name`) instead of full URIs for better readability.
- **Base32z Encoding**: Uses the human-friendly `z-base-32` encoding for keys and hashes.
- **Claim Log**: Accepted claims are records of an append-only, checkpointed log; an N-Triples file is derived from it.
- **Term Lookup**: Query the storage for the latest claim associated with a specific term.

## Installation

To build Scatterbase from source, you need to have Rust and Cargo installed:

```bash
cargo build --release
```

The binary will be available at `target/release/scatterbase`.

## Repository Layout

The repository is a Cargo workspace:

- `crates/scatter-claims` — what a claim is: claim IDs and their preimages, prior-use acknowledgments, client and server signatures, and the envelope's RDF form. Pure, with no I/O.
- `crates/scatterbase-kernel` — the library: reading triples, preparing and accepting claims, the claim log (on `scatter-log` and `scatter-integrity`), the storage file derived from it, and keys.
- `crates/scatterbase-cli` — the `scatterbase` binary: argument dispatch, prompts, help text and the command handlers.

## Usage

### Getting Started

Initialize a new Scatterbase instance:

```bash
scatterbase init
```
This command will prompt you for configuration details (domain, storage file path, etc.) and generate a server key pair. Use the `-d` or `--default` flag to use default values.

### Core Commands

#### Domain Management
Initialize a domain with a default triple:
```bash
scatterbase domain init
```

#### Triple Management
Write a single triple directly to the store:
```bash
scatterbase triple write <subject> <predicate> <object>
```

#### Claim Management
Claims are the primary unit of signed data in Scatterbase.

- **Prepare a claim**: Create a signed envelope from a set of triples.
  ```bash
  scatterbase claim prepare --file triples.nt > claim_envelope.ttl
  ```
- **Submit a claim**: Write a prepared claim envelope to the data store.
  ```bash
  scatterbase claim submit --file claim_envelope.ttl
  ```
- **Write a claim (direct)**: Combine prepare and submit in one step.
  ```bash
  scatterbase claim write --file triples.nt
  ```

#### Key Management
Register a key for an actor, so that claims attributed to it are accepted:
```bash
scatterbase key add --attribution https://example.org/alice
```
This generates a new Ed25519 key for the actor, keeps it beside the server's key, and registers its public key in a claim by the server. `claim prepare` and `claim write` then sign that actor's claims with it. Use `--file <keyfile>` to register a key you already hold, and `--private-key <keyfile>` when preparing a claim with it. `claim submit` verifies every claim's signature against the actor's registered key.

#### Claim Log
Every accepted claim is a record of an append-only log under `data/log`, with a checkpoint signed by the server after each one. The storage file is derived from it.

- **Verify the log**: check the signed checkpoint, every record and every signature.
  ```bash
  scatterbase log verify
  ```
- **Rebuild the storage file** from the log.
  ```bash
  scatterbase log rebuild
  ```

#### Prefix Management
Manage URI prefixes for shorter notation:
```bash
scatterbase prefix add schema http://schema.org/
```

#### Term Lookup
Find the latest claim hash associated with a specific term (URI):
```bash
scatterbase term latest <uri>
```

#### Purge Data
Delete all stored data (Warning: destructive!):
```bash
scatterbase purge
```

## Implementation Status

Scatterbase is currently in active development. Below is the status of core components:

- [x] **Core RDF Model**: IRIs, literals and triples are the `oxrdf` types, parsed with `oxttl`, as in the shared `scatter-*` crates.
- [x] **N-Triples Support**: Full support for reading and writing N-Triples.
- [x] **Turtle Support**: Support for parsing Turtle files for claim preparation.
- [x] **ED25519 Cryptography**: Key generation, signing, and verification.
- [x] **Base32z Encoding**: Custom encoding for all identifiers.
- [x] **File-based Storage**: Persistent storage in `.nt` files.
- [x] **CLI Interface**: Robust command-line interface with help for all commands.
- [x] **Claim Envelopes**: Implementation of signed claim structures with sequence IDs and timestamps.
- [x] **Term Lookup**: Ability to scan storage for the most recent activity related to a term.
- [ ] **Advanced Querying**: Support for SPARQL or complex graph queries (planned).
- [ ] **Network Synchronization**: P2P or client-server synchronization of claims (planned).

## Data Formats

- **Storage**: The claim log is a `scatter-log` file store of CBOR records. The derived storage file is standard N-Triples.
- **Claims**: Claims can be exported/imported as Turtle (`.ttl`) or N-Triples (`.nt`).
- **Config**: Configuration is stored in a `.env` file in the project root or a custom location.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
