# Claim Envelopes

A **claim** is the unit of provenance in Scatterbase. Triples are not stored as bare data: they are claimed by an actor, and the claim records *who* asserts them, *what that actor had seen* when asserting them, and *when* a server accepted them. This document specifies the claim, its identifier, its signatures and its RDF form. The implementation is the `scatter-claims` crate; each preimage below is produced by exactly one function there.

## Overview

A claim goes through two phases:

1. **Envelope** (client side). The actor prepares a claim of a set of triples, acknowledges the prior use of each term, and signs. `scatterbase claim prepare` writes an envelope.
2. **Signed claim** (server side). The server verifies the actor's signature against the actor's registered key, assigns a sequence ID and a timestamp, signs, and appends the result to the claim log. `scatterbase claim submit` does this; `scatterbase claim write` does both phases in one step and stores exactly the same triples.

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
{actor's signature, as ed25519:…}
{sequence ID}
{timestamp}
```

The sequence ID is the claim's offset in the claim log, written in decimal: the first claim an instance accepts has sequence ID `0`, and there are no gaps. The timestamp is the acceptance time in RFC 3339 with milliseconds, in UTC.

## Keys

A public key is named by `urn:ed25519:{key}`, the 32 bytes in z-base-32. An actor's key is registered by a claim holding:

```turtle
<{actor}> scatter:hasPublicKey <urn:ed25519:{key}> .
<urn:ed25519:{key}> rdf:type scatter:PublicKey .
<urn:ed25519:{key}> scatter:keyType scatter:ed25519 .
<urn:ed25519:{key}> scatter:owner <{actor}> .
```

The actor's key is the one registered by the latest such claim. The server refuses a claim whose signature does not verify against its actor's registered key, and a claim by an actor with no registered key. The one exception is the first claim of an empty log, which may register its own actor's key and is checked against that key: this is how a server's key is registered by `scatterbase init`. `scatterbase key add` registers every other actor's key in a claim by the server.

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

## The claim log

Accepted claims are kept in an append-only log, which is the instance's source of truth. The log is a `scatter-log` file store with one partition, `claim`, that keeps every record. After each append the server signs a checkpoint of the partition's Merkle tree head under the origin `{host}/log/claim`, so a copy of the log can be shown to be complete and in order. `scatterbase log verify` checks the checkpoint, every record and every signature.

A claim is one record of payload type `scatter:v0/claim`. Its header key is the claim ID in z-base-32, its `appended_at` is the acceptance time, and its offset is the claim's sequence ID. Its body has five parts, each salted and erasable on its own:

| # | Part | CBOR |
|---|---|---|
| 0 | The claim | `{"previous": bytes(32), "triples": [text, …]}`, each triple its N-Triples line, in the claim's order |
| 1 | The attribution | text, the actor's IRI |
| 2 | The client signature | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64)}`, `key` being the public key the signature verified against |
| 3 | The server signature | `{"alg": "ed25519", "key": bytes(32), "sig": bytes(64), "sequence": text, "timestamp": text}` |
| 4 | The acknowledgments | `[[text, bytes(32)], …]`: term IRI and prior claim ID, in order of the IRI |

## The store and its graphs

The storage file is derived from the log: it is a `scatter-quadstore` N-Quads file holding the RDF form of every claim, below, and `scatterbase log rebuild` writes it again from the log. A claim is written into two of the instance's named graphs, whose IRIs are `{domain}/graph/{name}`:

| Graph | Holds |
|---|---|
| `claim` | The triples of every claim by an actor other than the server |
| `server` | The triples of every claim by the server: registered keys and the domain |
| `meta` | Everything recorded about each claim: its reification, attribution, chain, acknowledgments, signatures, sequence ID, time and type |
| the default graph | Triples written outside any claim, by `scatterbase triple write` and `scatterbase domain init` |

Only accepted claims are written to the three named graphs, and the server's lookups read `meta` and `server` alone:

- the latest claim using a term, an actor's latest claim and a claim's sequence ID are read from `meta`;
- an actor's key is the one registered in `server` by the latest of the *server's* claims holding the registration.

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
