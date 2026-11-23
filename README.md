# Scatterbase

Scatterbase is a Rust-based CLI tool for managing RDF triples, signed claims, and cryptographic identities. It provides a decentralized way to store and verify information using linked data and digital signatures.

## Overview

Scatterbase allows users to create, sign, and store "claims" about RDF triples. These claims are attributed to specific identities (represented by URIs) and signed using ED25519 cryptography. Accepted claims are kept in an append-only log, with an N-Quads store derived from it, and claims are read and written as N-Triples or Turtle.

## Key Features

- **RDF Triple Management**: Create and store subject-predicate-object triples.
- **Signed Claims**: Group triples into signed "envelopes" that provide attribution, timestamps, and cryptographic proof of origin.
- **Identity & Keys**: Generate and manage ED25519 key pairs for signing claims.
- **URI Prefix Support**: Use short prefixes (e.g., `schema:name`) instead of full URIs for better readability.
- **Base32z Encoding**: Uses the human-friendly `z-base-32` encoding for keys and hashes.
- **Claim Log**: Accepted claims are records of an append-only, checkpointed log; an N-Quads store is derived from it, with claims, server claims and claim metadata in separate named graphs.
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
- `crates/scatter-quadstore` — the quad-store contract: lookup by pattern with wildcards and deltas of deleted and inserted quads, with an in-memory backend and an N-Quads file backend.
- `crates/scatterbase-kernel` — the library: reading triples, preparing and accepting claims, the claim log (on `scatter-log` and `scatter-integrity`), the quad store derived from it (on `scatter-quadstore`), and keys.
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
Claim, as the server, that a domain is a dataset:
```bash
scatterbase domain init https://example.org
```

#### Triple Management
Claim a single triple, as the actor named by `--attribution`:
```bash
scatterbase triple write <subject> <predicate> <object> --attribution <actor>
```

#### Claim Management
Claims are the primary unit of signed data in Scatterbase. Every claim names who makes it with `--attribution`: an actor with a registered key, or the server's own URI.

- **Prepare a claim**: Create a signed envelope from a set of triples.
  ```bash
  scatterbase claim prepare --attribution <actor> --file triples.nt > claim_envelope.ttl
  ```
- **Submit a claim**: Verify a prepared claim envelope and append it to the log.
  ```bash
  scatterbase claim submit --file claim_envelope.ttl
  ```
- **Write a claim (direct)**: Combine prepare and submit in one step.
  ```bash
  scatterbase claim write --attribution <actor> --file triples.nt
  ```
- **Erase parts of a claim**, for good: what was claimed (`content`), who claimed it (`attestation`) or what it acknowledged (`acknowledgments`). The claim keeps its ID, its place in the log and the server's signature.
  ```bash
  scatterbase claim erase <claim> --parts content --reason privacy --authority ticket:4411
  ```

#### Key Management
Register a key for an actor, so that claims attributed to it are accepted:
```bash
scatterbase key add --attribution https://example.org/alice
```
This generates a new Ed25519 key for the actor, keeps it beside the server's key, and registers its public key in the log as a key record written by the server. `claim prepare` and `claim write` then sign that actor's claims with it. Use `--file <keyfile>` to register a key you already hold, and `--private-key <keyfile>` when preparing a claim with it. `claim submit` verifies every claim's signature against the actor's registered keys.

A key can be given an end date with `--expires 2027-12-31T00:00:00Z`.

Revoke an actor's keys, or one of them with `--key-id`, after which `key add` registers a new one:
```bash
scatterbase key revoke --attribution https://example.org/alice
```

List the keys of an actor, or of the server:
```bash
scatterbase key list --attribution https://example.org/alice
scatterbase key list
```

Replace the server's key. The log records the new key, signed by the old one; what the old key signed still verifies:
```bash
scatterbase key rotate
```

#### Actors
Erase an actor from the log, for good: its record and its keys. The claims it made are not touched.
```bash
scatterbase actor erase --attribution https://example.org/alice --reason privacy --authority ticket:4412
```

#### Claim Log
The log under `data/log` has three partitions. The server partition holds the server's key, as a self-signed founding record, and the registry of the instance's graphs. The mutable partition holds the keys registered for actors. The claim partition holds every accepted claim. The server signs a checkpoint of a partition after each record. The storage file is derived from the claims.

- **Verify the log**: check the server's key chain, the signed checkpoints, every record, every signature and that every erased part is accounted for by a record of its erasure.
  ```bash
  scatterbase log verify
  ```
- **Rebuild the storage file** from the log.
  ```bash
  scatterbase log rebuild
  ```
- **Export a bundle**: a copy of the log that anyone can verify with nothing but the server's public key. With `--backup` it also holds who the actors are, and is not for publication.
  ```bash
  scatterbase log export ./bundle
  scatterbase log verify --bundle ./bundle --key urn:ed25519:<the server's public key>
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
- [x] **File-based Storage**: A checkpointed claim log, and an N-Quads store derived from it.
- [x] **CLI Interface**: Robust command-line interface with help for all commands.
- [x] **Claim Envelopes**: Implementation of signed claim structures with sequence IDs and timestamps.
- [x] **Term Lookup**: Ability to scan storage for the most recent activity related to a term.
- [ ] **Advanced Querying**: Support for SPARQL or complex graph queries (planned).
- [ ] **Network Synchronization**: P2P or client-server synchronization of claims (planned).

## Data Formats

- **Storage**: The claim log is a `scatter-log` file store of CBOR records. The derived storage file is standard N-Quads, in the graphs `{domain}/graph/claim`, `server` and `meta`.
- **Claims**: Claims can be exported/imported as Turtle (`.ttl`) or N-Triples (`.nt`).
- **Config**: Configuration is stored in a `.env` file in the project root or a custom location.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
