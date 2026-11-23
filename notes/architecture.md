# Architecture

How Scatterbase is put together now, and what is planned next. This replaces the earlier modularity and storage-evolution notes, whose plans were carried out differently. The specification of claims, the log and the store is [docs/claim-envelopes.md](../docs/claim-envelopes.md).

## Crates

The repository is a Cargo workspace. Dependencies point downward in this list.

| Crate | Holds | Does I/O |
|---|---|---|
| `scatterbase-cli` | The `scatterbase` binary: commands, arguments, prompts, the `.env` configuration | yes |
| `scatterbase-kernel` | Preparing and accepting claims, the log, the store derived from it, keys | yes |
| `scatter-quadstore` | The quad-store contract (`lookup` by pattern, `apply` for deltas) and its in-memory and N-Quads file backends | file backend only |
| `scatter-claims` | What a claim is: its ID and preimage, acknowledgments, signatures, its RDF form | no |

The kernel is built on crates shared with Triplespace, taken from the triplespace repository at a commit pinned in `Cargo.toml`:

| Shared crate | Used for |
|---|---|
| `scatter-vocab` | The `scatter:` vocabulary and the vocabularies it reuses |
| `scatter-log` | The append-only log: records, partitions, the Merkle tree, the file store, the graph registry |
| `scatter-integrity` | Signed checkpoints, the key chain, attestations, verification, export bundles |
| `scatter-actors` | Actor records, surrogates and key records |

RDF terms are `oxrdf`'s and parsing is `oxttl`'s, as in the shared crates. `scatter-claims` and `scatter-quadstore` live here and are written to be shared in turn.

## How data flows

1. **A claim is prepared.** The actor's triples, the claim before it in the actor's chain and the last use of each term are signed by the actor into an envelope.
2. **The server accepts it.** With the log locked, it checks the signature against the actor's keys, checks that the envelope is current, assigns the next offset as the sequence ID, signs, and appends a record to the claim partition, followed by a signed checkpoint.
3. **The store is derived.** The claim's RDF form is written into the quad store's named graphs. The store can be thrown away and rebuilt from the log.

The log is the source of truth. It has three partitions: `server` (the server's key chain and the graph registry), `mutable` (actors and their keys) and `claim` (every accepted claim, and the record of every erasure).

## Rules the code keeps

- **The kernel takes what it needs as arguments.** It reads no environment variables and prints nothing. Configuration and all output belong to the CLI.
- **Each hash has one function that computes it**, in the crate that owns the type.
- **Nothing a claim says is taken for what the server recorded.** Lookups read the `meta` graph and the log, never a claim's own triples.
- **A record header never names a person.** Actors are known in headers by surrogates.
- **One process has the log open at a time.**

## What is planned

In rough order. Each note in this directory that predates the crate refactor says at its top what in it still holds.

1. **A server and HTTP API** ([rest-api.md](rest-api.md)): clients prepare envelopes locally and submit them over HTTP. With it, a store that is not read whole on every command, and `scatter-projection` in place of the kernel's own projection.
2. **Postgres** ([postgres-adapter.md](postgres-adapter.md)): `scatter-log-postgres` for the log and a Postgres driver for the quad store.
3. **Wikibase and foreign data** ([wikibase-extension.md](wikibase-extension.md), [librarybase.md](librarybase.md)): the shared `scatter-wikibase-*` and ingest crates, a `foreign` partition for mirrored data, and SPARQL through Oxigraph behind `scatter-quadstore`.

Known gaps that have no place in that order yet: a restore command for backup bundles, a guard against a stolen server key rotating itself (Triplespace has recovery keys), moving the server's own claims into the server partition, and publishing `scatter-claims` and `scatter-quadstore`.
