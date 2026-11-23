# Claim Envelopes

A **claim** is the unit of provenance in Scatterbase. Triples are not stored as bare data: they are claimed by an actor, and the claim records *who* asserts them, *what that actor had seen* when asserting them, and *when* a server accepted them. This document specifies the claim, its identifier, its signatures and its RDF form. The implementation is the `scatter-claims` crate; each preimage below is produced by exactly one function there.

## Overview

A claim goes through two phases:

1. **Envelope** (client side). The actor prepares a claim of a set of triples, acknowledges the prior use of each term, and signs. `scatterbase claim prepare` writes an envelope.
2. **Signed claim** (server side). The server verifies the actor's signature against the actor's registered key, assigns a sequence ID and a timestamp, signs, and appends the result to the claim partition of the log. `scatterbase claim submit` does this; `scatterbase claim write` does both phases in one step and stores exactly the same triples. Every claim names its actor: there is no default.

## Hashes

Every hash is SHA-256. In text and in IRIs a hash is written in **z-base-32**: 52 characters, no padding.

| Hash | Preimage | Used as |
|---|---|---|
| **Triple hash** | The triple's N-Triples line, `<s> <p> o .`, with one space between terms and before the dot and no line ending | `https://scatter.red/triple/{hash}` |
| **Term hash** | The term's N-Triples form, `<iri>`, with the angle brackets | The last segment of an acknowledgment IRI |
| **Claim ID** | The claim preimage, below | `https://scatter.red/claim/{hash}` |

The claim ID of "no claim" is 32 zero bytes, `yyyy…y` (52 `y`). It stands for the claim before an actor's first claim and for the prior use of a term nobody has used.

## The claim

A claim is:

- its **triples**: a set, holding IRIs and literals only;
- its **attribution**: the IRI of the actor making the claim;
- its **previous claim**: the ID of the claim that actor made before this one, which makes each actor's claims a chain.

### Claim ID

The claim ID is the SHA-256 of this text, each line ending with a line feed:

```text
scatter-claim-v0
{attribution IRI}
{previous claim ID}
{triple hash}          one line per triple, in ascending order of the z-base-32 text
```

Nothing the server assigns is part of the claim ID, so the ID is known before the claim is submitted.

### Blank nodes and quoted triples

A claim holds neither, so when triples are read from a file:

- **A blank node** becomes the skolem IRI `{domain}/.well-known/genid/{claim}/{label}`. `{claim}` is the claim's *provisional ID*: the ID the claim has with `current` in place of `{claim}`. The claim's own ID is computed over the final IRIs.
- **A blank node written without a label** (`[ … ]`, a collection) is first labelled `anon1`, `anon2`, … in order of first appearance, skipping any label the input itself uses.
- **A quoted triple** becomes its triple hash IRI, the IRI the envelope gives that triple when it is claimed. `<< s p o >> q v` is claimed as `<https://scatter.red/triple/{hash}> q v`.

## Prior-use acknowledgments

For every IRI the claim uses in subject or object position, the envelope records which claim last used that term as far as the client knew, or "no claim". Every claim acknowledges every term it uses, so the claims that used a term are the claims with an acknowledgment of it.

A server accepts an envelope only if it is current: its previous claim is the actor's latest claim in the store, and each acknowledgment names the claim that last used its term. An envelope prepared before another claim was accepted is refused, and is prepared again against the store as it then is. So no claim is accepted without its actor having seen every earlier claim about the same terms.

## Signatures

Both signatures are Ed25519, written `ed25519:` followed by the 64 bytes in z-base-32.

**The actor** signs this text, each line ending with a line feed:

```text
scatter-claim-signature-v0
{claim ID}
<{term IRI}> {prior claim ID}      one line per acknowledged term, in ascending order of the IRI
```

The claim ID commits to the triples, the attribution and the previous claim, so the signature covers those and the acknowledgments. It does not depend on how the envelope is written as RDF.

**The server** signs this text when it accepts the envelope:

```text
scatter-claim-acceptance-v0
{claim ID}
{SHA-256 of the actor's signature, in z-base-32}
{sequence ID}
{timestamp}
```

The acceptance names the claim and the actor's signature by hashes alone, so it can be kept, and the server's signature checked, after the claim's triples and the actor's signature have been erased.

The sequence ID is the claim's offset in the claim partition of the log, written in decimal. Sequence IDs only increase, and there is a gap wherever the partition holds the record of an erasure. The timestamp is the acceptance time in RFC 3339 with milliseconds, in UTC.

## Keys

A public key is named by `urn:ed25519:{key}`, the 32 bytes in z-base-32. Its **key ID** is the SHA-256 of the public key in z-base-32.

**The server's key** is the last key of the chain in the log's server partition, which begins with the founding record, and a claim by the server is checked against it. `scatterbase key rotate` replaces it: the log records the new key in a `key:` record signed by the old one, and from then on only the new key attests records, signs checkpoints, accepts claims and claims as the server. What earlier keys signed still verifies, since every key of the chain is known to have been the server's. The old private key is overwritten.

**An actor's keys** are registered by the server in the log's mutable partition, as `scatter-actors` key records (below). An actor may have several keys at once. A claim by an actor is accepted if its signature verifies against any key of the actor's that is registered, not revoked and valid at the time; a claim by an actor with no such key is refused. `scatterbase key add` registers a key, which with `--expires` is valid only until a given time. `scatterbase key revoke` revokes an actor's keys, or with `--key-id` one of them, and `scatterbase key list` shows what has become of each. A revoked key signs no more, and the claims it signed stay in the log and stay verifiable, since each claim record keeps the public key it was checked against.

Nothing anyone claims registers a key. The keys actors have now are also written to the store, in the `mutable` graph, as:

```turtle
<{actor}> scatter:hasPublicKey <urn:ed25519:{key}> .
<urn:ed25519:{key}> rdf:type scatter:PublicKey .
<urn:ed25519:{key}> scatter:keyType scatter:ed25519 .
<urn:ed25519:{key}> scatter:owner <{actor}> .
```

That graph is derived from the key records, and a key that is revoked or has expired leaves it. The same triples claimed by an actor, or by the server, are in the `claim` or `server` graph and are only what was claimed.

Each actor has its own key. The server's private key is in the file the configuration names; `key add` writes an actor's new key to `actors/{hash}.ed25519.private` beside it, where `{hash}` is the SHA-256 of the actor's IRI in z-base-32.

## RDF form

An envelope with claim ID `C` is these triples. `T` is a triple hash and `H` a term hash.

```turtle
# The claim's triples, as claimed
<http://example.org/resource/1> <http://schema.org/name> "Test Resource" .

# For each triple: its reification and its claim
scatter_triple:{T} rdf:type rdf:Statement .
scatter_triple:{T} rdf:type scatter:ClaimTriple .
scatter_triple:{T} rdf:subject <http://example.org/resource/1> .
scatter_triple:{T} rdf:predicate <http://schema.org/name> .
scatter_triple:{T} rdf:object "Test Resource" .
scatter_triple:{T} scatter:claimedIn scatter_claim:{C} .

# Attribution and chain
scatter_claim:{C} prov:wasAttributedTo <{actor}> .
scatter_claim:{C} scatter:previousClaim scatter_claim:{previous} .

# For each term: its acknowledgment
<https://scatter.red/ack/{C}/term/{H}> scatter:term <{term}> .
<https://scatter.red/ack/{C}/term/{H}> scatter:priorClaim scatter_claim:{prior} .
scatter_claim:{C} scatter:acknowledgesPriorUse <https://scatter.red/ack/{C}/term/{H}> .

# The actor's signature
scatter_claim:{C} scatter:userSignature "ed25519:{signature}" .
```

A signed claim adds:

```turtle
scatter_claim:{C} rdf:type scatter:SignedClaim .
scatter_claim:{C} prov:generatedAtTime "{timestamp}"^^xsd:dateTime .
scatter_claim:{C} scatter:seqID "{sequence ID}" .
scatter_claim:{C} scatter:serverSignature "ed25519:{signature}" .
```

In Turtle the envelope is written one triple per line: the claim's triples in order, then the triples about them in order, then the actor's signature, then the server's triples with the server's signature last.

**Reading an envelope back.** Only the triples about the claim are needed, which in the store are the `meta` graph. The claim's triples are taken from the reification of every `scatter_triple:{T}` claimed in `C`, each checked against its triple hash; the claim ID is then recomputed and must equal `C`. A set of triples that reads back is therefore the envelope that was written, and the signatures can be checked against it.

## The log

Everything an instance accepts is kept in an append-only log, which is its source of truth. The log is a `scatter-log` file store with three partitions. Each keeps every record header, and after each append the server signs a checkpoint of the partition's Merkle tree head, under the origin `{host}/log/{name}`, so a copy of the log can be shown to be complete and in order. `scatterbase log verify` checks the key chain, the checkpoints, every record, every signature and that every erased part is accounted for.

**One writer at a time.** A process holds a lock on the log's directory for as long as it has the log open, and everything that reads the log to decide what to append, appends, signs the checkpoint and brings the store up to the log does so with the log open. So two commands never interleave: the second waits for the first, for up to half a minute. `scatterbase claim write` prepares and accepts its claim under one hold of the lock, so claims written at once are all accepted, one after another, each following the one before. An envelope prepared separately with `scatterbase claim prepare` is checked when it is submitted, and refused if another claim got in first.

### The server partition

Partition 0 founds and describes the instance. It holds `scatter:v0/config` records, as a `config` partition does in Triplespace, and `scatterbase init` writes the first of them:

| Offset | Key | Content |
|---|---|---|
| 0 | `key:{key ID}` | The server's public key. This is the instance's **founding record**: it is signed by the key it registers, and everything else in the log is checked against that key. |
| 1 | `graph:server` | The server partition itself: partition 0 and its policies |
| 2 | `graph:claim` | The claim partition: its ID, 64 random bits, and its policies |
| 3 | `graph:mutable` | The mutable partition: its ID and its policies |
| 4 | `graph:meta` | The `meta` graph, a projection |

The `graph:` records are the instance's **graph registry**. Each is attested by the server under the founding record's authority. The claim partition is the one the `graph:claim` record names. The `key:` records form a chain, in which a later key must be signed by the one before it, and the server's key is the last of the chain (see Keys).

### The mutable partition

The mutable partition holds what can be taken back: who the actors are and the keys they sign with. Its records are attested by the server under the founding record's authority, so only the server writes them.

**Actors and surrogates.** A record's header is never erased, so it must not say who an actor is. An actor is therefore known in this partition by a **surrogate**: a number the server mints in sequence, from 1, the first time the actor is given a record, written `actor:{n}`. The one place that says which actor a surrogate stands for is its actor record, of payload type `scatter:v0/actor` and keyed `actor:{n}`, whose content is a `scatter-actors` actor record of kind `federated` holding the actor's IRI:

```text
{"kind": "federated", "status": "active", "raw": "{actor IRI}"}
```

If that record is erased (see Erasure, below), only the number is left. The key records under the surrogate can no longer be tied to an IRI and count for nobody, and the IRI seen again is given a new surrogate.

**Keys.** A key is registered by a record of payload type `scatter:v0/key`, whose header key is the actor's surrogate and whose content is a `scatter-actors` key record:

```text
{"key_id": text, "alg": "ed25519", "public_key": bytes(32), "valid_from": microseconds since the epoch}
```

with `valid_until` if the key expires. A later record for the same key ID with `"revoked": true` revokes it.

### The claim partition

A claim is one record of payload type `scatter:v0/claim`. Its header key is the claim ID in z-base-32, its `appended_at` is the acceptance time, and its offset is the claim's sequence ID. Its body has five parts, each salted:

| # | Part | CBOR | Erasable |
|---|---|---|---|
| 0 | `content`: the claim | `{"previous": bytes(32), "triples": [text, …]}`, each triple its N-Triples line, in the claim's order | yes |
| 1 | `comment` | `null` | |
| 2 | `attestation`: who claims, and their signature | `{"actor": text, "alg": "ed25519", "key": bytes(32), "sig": bytes(64)}`, `key` being the public key the signature verified against | yes |
| 3 | `acknowledgments` | `[[text, bytes(32)], …]`: term IRI and prior claim ID, in order of the IRI | yes |
| 4 | `acceptance`: the server's | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64), "client": text, "sequence": text, "timestamp": text}`, `client` being the digest of the actor's signature | never |

## Erasure

A log that never forgets cannot hold what the law, or a person, may later require to be gone. A log that forgets freely proves nothing. So part of every claim is kept for good and part can be erased for good.

**Always kept:** the record's header, which holds the claim ID, its place in the log, its time and a commitment to the body; and the server's acceptance. Neither says what was claimed or by whom. An erased claim therefore still has its ID, its sequence ID and a server's signature that verifies, and the claims after it, whose chains and acknowledgments point at it, still point at something the log proves was there. The Merkle tree is over headers, so every checkpoint signed before an erasure still holds after it.

**Erasable, each on its own:**

| Part | Erasing it removes | What remains true |
|---|---|---|
| `content` | What was claimed: the triples, their reification, the previous claim | Who claimed something, when, and what terms it acknowledged |
| `attestation` | Who claimed it: the actor, the actor's signature and key | What was claimed and when; the claim is nobody's |
| `acknowledgments` | Which terms the claim used and what it had seen of them | The claim and its actor |

`scatterbase claim erase <claim> --parts <parts> --reason <class> --authority <text>` erases parts of a claim. The bytes are removed from the log's files and the store is written again without them. It cannot be undone, and submitting the same envelope again is refused, since the claim's ID is still in the log.

Erasing costs what it removes. A claim with no content can no longer have its ID or its actor's signature checked. A claim with no attestation leaves its actor's chain, so the actor's next claim follows an earlier one. A claim with no acknowledgments is no longer found as the last use of its terms.

**An actor** is erased as a whole with `scatterbase actor erase --attribution <actor> --reason <class> --authority <text>`: its actor record and every key record under its surrogate, leaving the number. The actor then has no keys. The claims it made are not touched: each still names the actor in its attestation until that is erased.

**Every erasure is recorded** before it is done, as a record of payload type `scatter:v0/erase` in the same partition, attested by the server:

```text
{"targets": {"offsets": [41]}, "parts": ["content"], "reason": "privacy", "authority": "ticket:4411"}
{"targets": {"key": "actor:7"}, "reason": "privacy", "authority": "ticket:4412"}
```

`reason` is one of `legal`, `privacy`, `upstream` and `operational`, and `authority` says what authorizes the erasure. The record's comment part holds a public reason, if one is given. `scatterbase log verify` accepts a missing part only where an erase record covers it.

## Bundles

A **bundle** is a copy of the log that verifies with nothing else: a directory holding partitions of the log with every record as the log has it, every signed checkpoint, and the chain of the server's keys. It is a `scatter-integrity` export bundle.

`scatterbase log export <directory>` writes one for publication. It holds the server and claim partitions, which is everything needed to check every claim, and it says nothing of who the actors are beyond what their claims say: the mutable partition, with the actor records and key records, is left out. `scatterbase log export <directory> --backup` writes one with all three partitions, which is not for publication.

`scatterbase log verify --bundle <directory> --key urn:ed25519:{key}` verifies a bundle anywhere, with no instance: the same checks as `scatterbase log verify`, against the server key the reader trusts. Without `--key` the bundle is checked against the key its own chain ends in, which shows that it is consistent and leaves it to the reader to compare that key with one they trust. In a bundle for publication, where no key records are to be had, a claim's signature is checked against the key its record says it was accepted under, and not against a registration.

Parts that were erased are erased in a bundle too, with the records of their erasure.

## The store and its graphs

The storage file is derived from the log: it is a `scatter-quadstore` N-Quads file holding the RDF form of every claim, below, and `scatterbase log rebuild` writes it again from the log. The instance's named graphs have IRIs `{domain}/graph/{name}`, and a claim is written into two of them:

| Graph | Holds |
|---|---|
| `claim` | The triples of every claim by an actor other than the server |
| `server` | The triples of every claim by the server, such as the declaration of the domain. (The records of the server *partition* are configuration and are not projected into it.) |
| `meta` | Everything recorded about each claim: its reification, attribution, chain, acknowledgments, signatures, sequence ID, time and type |
| `mutable` | The registration of each key an actor has now, derived from the key records of the mutable partition |

Only what the log holds is written to the named graphs. The server's lookups read `meta` alone, for the latest claim using a term, an actor's latest claim and a claim's sequence ID, and keys are read from the log itself, never from the store.

So nothing an actor claims is taken for a registered key or for what the server recorded about a claim, whatever its triples say. A claim that says `<alice> scatter:hasPublicKey <urn:ed25519:…>` or `scatter_claim:{C} scatter:seqID "999999"` is stored, in `claim`, as what its actor asserted and nothing more. The `meta` graph alone is enough to read every claim back, since it holds each claimed triple's reification.

## IRI reference

### Namespace prefixes

| Prefix | Namespace IRI |
|---|---|
| `rdf` | `http://www.w3.org/1999/02/22-rdf-syntax-ns#` |
| `prov` | `http://www.w3.org/ns/prov#` |
| `scatter` | `https://scatter.red/terms/v0/` |
| `scatter_claim` | `https://scatter.red/claim/` |
| `scatter_triple` | `https://scatter.red/triple/` |

### Identity IRIs

| IRI pattern | Identifies |
|---|---|
| `https://scatter.red/claim/{hash}` | A claim, by its claim ID |
| `https://scatter.red/triple/{hash}` | A triple, by its triple hash |
| `https://scatter.red/ack/{claim ID}/term/{term hash}` | A claim's acknowledgment of one term |
| `urn:ed25519:{key}` | A public key |

### Scatterbase vocabulary (`https://scatter.red/terms/v0/`)

These terms are defined in the `scatter-vocab` crate, which is where the code takes them from.

| IRI | Type | Description |
|---|---|---|
| `scatter:ClaimTriple` | Class | A triple as claimed |
| `scatter:SignedClaim` | Class | A claim the server accepted: sequenced, timestamped and signed by it |
| `scatter:PublicKey` | Class | A registered public key |
| `scatter:SandboxDomain` | Class | The `.sandbox` domain an instance is given when none is configured |
| `scatter:claimedIn` | Predicate | From a claimed triple to its claim |
| `scatter:previousClaim` | Predicate | From a claim to the one before it in the actor's chain |
| `scatter:userSignature` | Predicate | The actor's signature |
| `scatter:serverSignature` | Predicate | The accepting server's signature |
| `scatter:seqID` | Predicate | The sequence ID the accepting server assigned |
| `scatter:term` | Predicate | From an acknowledgment to the term it is about |
| `scatter:priorClaim` | Predicate | From an acknowledgment to the claim that last used the term |
| `scatter:acknowledgesPriorUse` | Predicate | From a claim to each of its acknowledgments |
| `scatter:hasPublicKey` | Predicate | From an actor to its public key |
| `scatter:keyType` | Predicate | From a public key to its algorithm |
| `scatter:ed25519` | Individual | The Ed25519 key type |
| `scatter:owner` | Predicate | From a public key to its actor |

### Standard vocabulary used

| IRI | Description |
|---|---|
| `rdf:type`, `rdf:Statement`, `rdf:subject`, `rdf:predicate`, `rdf:object` | RDF reification of each claimed triple |
| `rdf:type` | Also the class of an accepted claim, a public key and a domain |
| `prov:wasAttributedTo` | From a claim to its actor |
| `prov:generatedAtTime` | When the server accepted the claim |
