# Scatterbase

Scatterbase stores RDF triples as **signed claims** in a **verifiable log**. Every triple it holds was claimed by a named actor, signed with that actor's key, and accepted by a server that signed its place in an append-only log. Anyone with a copy of the log and the server's public key can check all of it.

It is a command-line tool and a set of Rust crates. There is no server process yet: the commands act on an instance kept in local files.

## What it does

- **Claims, not bare triples.** A claim is a set of triples, the actor who asserts them, the actor's previous claim, and what the actor had seen of each term it uses. The actor signs it; the server checks the signature, assigns it the next place in the log and signs that.
- **A log that proves itself.** Accepted claims are records of an append-only log. After every record the server signs a checkpoint of the log's Merkle tree, so a copy can be shown to be complete and in order.
- **Keys with a lifecycle.** The server registers each actor's keys, and can revoke or expire them. The server's own key can be rotated, and what the old key signed still verifies.
- **Erasure that leaves the proof intact.** What was claimed, who claimed it and what it acknowledged can each be erased for good. The claim keeps its ID, its place in the log and the server's signature.
- **Copies anyone can verify.** A bundle is a copy of the log that verifies on its own, with no instance and no secrets.
- **An RDF store derived from the log.** Claims are projected into an N-Quads file, in named graphs that keep what actors assert apart from what the server recorded about each claim.

## Quick start

Build the binary. Rust 1.95 or later is needed.

```bash
cargo build --release
```

The binary is `target/release/scatterbase`. The examples below call it `scatterbase`.

Set up an instance in the current directory. `--default` asks nothing and gives the instance a throwaway `.sandbox` domain; without it you are asked for the domain, the file paths and the server's URI.

```bash
scatterbase init --default
```

Register a key for an actor. This generates a key pair, keeps the private key with the server's, and records the public key in the log.

```bash
scatterbase key add --attribution https://example.org/alice
```

Claim some triples as that actor. Input is Turtle if the file name ends in `.ttl`, and N-Triples otherwise.

```bash
cat > people.ttl <<'EOF'
@prefix ex: <https://example.org/> .
@prefix schema: <http://schema.org/> .
ex:ada a schema:Person ; schema:name "Ada Lovelace" ; schema:knows ex:charles .
EOF

scatterbase claim write --attribution https://example.org/alice --file people.ttl
```

```text
Claim committed to data storage successfully.
  Claim: m1ct1hpbru814scks5q5up98xsbbqgw5y5iyi1xbcitfnjntgkwo
  Sequence ID: 1
  Timestamp: 2026-11-23T20:33:01.481Z
  Server Signature: ed25519:h86n9uajby4pcxpj5a7ops3fmap48p75…
```

Check the whole log: the server's key chain, every signed checkpoint, every record and every signature.

```bash
scatterbase log verify
```

```text
Log verified: 2 claims under the checkpoint of example.org/log/claim
  graph server: partition 0000000000000000
  graph claim: partition c9e807e819b858de
  graph mutable: partition 95bec2eb483d9b80
  graph meta: projection
```

There are two claims because `init` made the first: the server's declaration of the instance's domain.

The triples are now in the store, `data/scatterbase.nq`, in the instance's `claim` graph:

```text
<https://example.org/ada> <http://schema.org/name> "Ada Lovelace" <https://example.org/graph/claim> .
```

## The two-step form

`claim write` prepares a claim and submits it in one step. The steps can be taken apart, which is how a client that holds its own key would work:

```bash
scatterbase claim prepare --attribution https://example.org/alice --file people.ttl > envelope.ttl
scatterbase claim submit --file envelope.ttl
```

`prepare` writes a signed envelope as Turtle. `submit` verifies it and appends it to the log. An envelope is refused if another claim about the same terms, or by the same actor, was accepted after it was prepared; preparing it again fixes that.

## Commands

Every command describes itself with `--help`. A command that fails says why on standard error and exits with a non-zero status. [docs/commands.md](docs/commands.md) is the full reference.

| Command | Does |
|---|---|
| `init` | Sets up a new instance |
| `claim prepare`, `submit`, `write` | Makes and accepts claims |
| `claim erase` | Erases parts of an accepted claim, for good |
| `triple write` | Claims a single triple |
| `domain init` | Claims, as the server, that a domain is a dataset |
| `key add`, `revoke`, `list` | Registers, revokes and lists actors' keys |
| `key rotate` | Replaces the server's key |
| `actor erase` | Erases an actor's record and keys from the log, for good |
| `log verify`, `rebuild`, `export` | Verifies the log, rebuilds the store from it, writes a bundle |
| `term latest` | Prints the claim that last used a term |
| `prefix add` | Adds a prefix that URIs may be written with |
| `purge` | Deletes everything the instance keeps |

## Documentation

- [docs/commands.md](docs/commands.md) — every command, its options and examples.
- [docs/operations.md](docs/operations.md) — running an instance: the files it keeps, keys, backups, erasure, and what to do when something is interrupted.
- [docs/claim-envelopes.md](docs/claim-envelopes.md) — the specification: claim IDs, signatures, the log's records, erasure, bundles, the store's graphs and the vocabulary.
- [notes/architecture.md](notes/architecture.md) — how the code is put together, the rules it keeps, and what is planned.

## Repository layout

The repository is a Cargo workspace of four crates.

| Crate | Holds |
|---|---|
| [`scatter-claims`](crates/scatter-claims) | What a claim is: its ID, acknowledgments, signatures and RDF form. No I/O. |
| [`scatter-quadstore`](crates/scatter-quadstore) | The quad-store contract, with an in-memory backend and an N-Quads file backend |
| `scatterbase-kernel` | Preparing and accepting claims, the log, the store derived from it, and keys |
| `scatterbase-cli` | The `scatterbase` binary |

The kernel is built on crates shared with [Triplespace](https://github.com/scatterbase/triplespace): `scatter-vocab`, `scatter-log`, `scatter-integrity` and `scatter-actors`. They are taken from that repository at a commit pinned in `Cargo.toml`.

## Development

```bash
cargo build --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs these three on every pull request and on `main`.

## Status

Scatterbase is in active development and has made no release. Formats have changed between commits and may change again; an instance written by one commit may not be readable by the next.

What is not built yet:

- **A server.** There is no network interface; clients and the server are the same local command.
- **Querying.** There is no SPARQL. The store is an N-Quads file that other tools can load.
- **Other storage.** The log and the store are local files, and the store is read whole into memory by each command.
- **Synchronization** between instances.

[notes/architecture.md](notes/architecture.md) lists what is planned and in what order.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
