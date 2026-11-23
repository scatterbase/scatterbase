# CLI to REST API Mapping

> **Status.** This note predates the crate refactor. The mapping of commands to endpoints still describes the intent. What has changed since: every claim must name its actor (`--attribution`), `triple write` and `domain init` are claims, there are commands for keys (`key add`, `revoke`, `list`, `rotate`), erasure (`claim erase`, `actor erase`) and the log (`log verify`, `rebuild`, `export`), and a prepared envelope is refused if another claim was accepted first. See [architecture.md](architecture.md).

This document maps the current Scatterbase CLI commands to a future HTTP REST API, and identifies functionality that needs to be added.

## Current CLI Commands → REST Endpoints

| CLI Command | HTTP Method | REST Endpoint | Notes |
|---|---|---|---|
| `scatterbase init` | — | — | Local-only: creates `.env`, generates keys. Not exposed via API. |
| `scatterbase domain init` | `POST` | `/domains` | Initialize a domain with a default triple |
| `scatterbase triple write <s> <p> <o>` | `POST` | `/triples` | Body: `{subject, property, object}` |
| `scatterbase claim prepare --file <f>` | `POST` | `/claims/prepare` | Body: triples payload; returns unsigned envelope |
| `scatterbase claim submit --file <f>` | `POST` | `/claims/submit` | Body: signed claim envelope |
| `scatterbase claim write --file <f>` | `POST` | `/claims` | Combined prepare+submit in one step |
| `scatterbase prefix add <prefix> <uri>` | `PUT` | `/prefixes/{prefix}` | Body: `{uri}` |
| `scatterbase key add` | `POST` | `/keys` | Register a public key |
| `scatterbase term latest <term>` | `GET` | `/terms/{term}/latest` | Read-only lookup |
| `scatterbase purge` | `DELETE` | `/data` | Destructive; requires authorization |

## Functionality to Add

### Read/List Endpoints

The CLI currently has no list or query commands beyond `term latest`. The REST API should add:

| HTTP Method | REST Endpoint | Description |
|---|---|---|
| `GET` | `/triples` | List/query triples, with filtering by subject, property, or object |
| `GET` | `/triples/{id}` | Get a specific triple |
| `GET` | `/claims` | List claims, with filtering by signer, timestamp range |
| `GET` | `/claims/{id}` | Get a specific claim envelope |
| `GET` | `/prefixes` | List all registered prefixes |
| `GET` | `/keys` | List registered public keys |
| `GET` | `/domains` | List initialized domains |

### Entity Namespace Registry

Support for compact entity identifiers (see `librarybase.md`):

| HTTP Method | REST Endpoint | Description |
|---|---|---|
| `POST` | `/entity-namespaces` | Register an entity namespace (e.g., `WDQ` → `http://www.wikidata.org/entity/Q{id}`) |
| `GET` | `/entity-namespaces` | List registered entity namespaces |
| `GET` | `/entity-namespaces/{prefix}` | Get a specific entity namespace |
| `DELETE` | `/entity-namespaces/{prefix}` | Remove an entity namespace |

Corresponding CLI: `scatterbase entity-ns add <prefix> <pattern>`

### Advanced Querying

| HTTP Method | REST Endpoint | Description |
|---|---|---|
| `POST` | `/query` | Execute a structured query across triples and claims |

Query support should handle filtering across native and foreign entities, timestamp ranges, and signer-based provenance.

### Health and Status

| HTTP Method | REST Endpoint | Description |
|---|---|---|
| `GET` | `/health` | Server health check |
| `GET` | `/status` | Instance status: domain, storage backend, key count, triple count |

## Design Notes

- **Authentication**: Write endpoints (`POST`, `PUT`, `DELETE`) should require authentication. `GET` endpoints can be public or scoped.
- **Content types**: Accept and return `application/json` by default. Support `application/n-triples` and `text/turtle` for triple/claim payloads.
- **`init` stays CLI-only**: It creates local files (`.env`, key pairs) and has no meaningful REST equivalent.
- **`purge` needs safeguards**: If exposed at all, `DELETE /data` requires strong authorization and confirmation.
- **`--env` flag**: A local config concern; the API server reads its own configuration at startup.
- **Append-only model**: No `PUT`/`PATCH` on triples or claims. New claims supersede old ones, so update semantics are handled by writing new data.
