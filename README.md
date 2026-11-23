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
- **Local Storage**: Simple file-based storage using the N-Triples format.
- **Term Lookup**: Query the storage for the latest claim associated with a specific term.

## Installation

To build Scatterbase from source, you need to have Rust and Cargo installed:

```bash
cargo build --release
```

The binary will be available at `target/release/scatterbase`.

## Repository Layout

The repository is a Cargo workspace:

- `crates/scatterbase-kernel` — the library: RDF terms and triples, claim envelopes, hashing, keys and storage.
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
Add a new key to the system:
```bash
scatterbase key add --name "Alice"
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

- [x] **Core RDF Model**: Implementation of URIs, Literals, and Triples.
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

- **Storage**: `NtFileStorage` uses the standard N-Triples format.
- **Claims**: Claims can be exported/imported as Turtle (`.ttl`) or N-Triples (`.nt`).
- **Config**: Configuration is stored in a `.env` file in the project root or a custom location.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
