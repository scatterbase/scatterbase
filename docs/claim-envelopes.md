# Claim Envelopes

A **claim envelope** is the fundamental unit of provenance in Scatterbase. When a user writes triples, they are not stored as bare data — they are wrapped in a claim envelope that records *who* made the assertion, *when* it was recorded, and *what* was known at the time. This document explains the structure of a claim envelope and all the URIs involved.

## Overview

A claim goes through two phases:

1. **ClaimEnvelope** (client-side) — the user prepares a set of triples, signs them, and submits the envelope.
2. **SignedClaim** (server-side) — the server adds a timestamp, a sequence ID, and its own signature, then stores the result.

The final stored claim contains the original triples, RDF reification metadata, prior-use acknowledgments, and two signatures (user + server).

## Anatomy of a Claim Envelope

A claim envelope contains the following groups of triples:

### 1. Original Triples

The user-submitted data triples, stored verbatim. For example:

```turtle
<http://example.org/resource/1> <http://schema.org/name> "Test Resource" .
<http://example.org/resource/1> rdf:type <http://schema.org/Thing> .
```

### 2. Reified Triple Metadata

Each original triple is hashed to produce a **triple hash URI**. The hash is computed from the triple's canonical N-Triples representation. For each triple, the envelope records:

- Its type as both `rdf:Statement` and `scatter:ClaimTriple`
- RDF reification triples (`rdf:subject`, `rdf:predicate`, `rdf:object`)
- A link back to the claim via `scatter:claimedIn`

```turtle
scatter_triple:{tripleHash} rdf:type rdf:Statement .
scatter_triple:{tripleHash} rdf:type scatter:ClaimTriple .
scatter_triple:{tripleHash} rdf:subject <http://example.org/resource/1> .
scatter_triple:{tripleHash} rdf:predicate <http://schema.org/name> .
scatter_triple:{tripleHash} rdf:object "Test Resource" .
scatter_triple:{tripleHash} scatter:claimedIn scatter_claim:{claimHash} .
```

### 3. Claim Attribution

The claim is attributed to the server identity that submitted it, using the PROV ontology:

```turtle
scatter_claim:{claimHash} prov:wasAttributedTo <{serverURI}> .
```

### 4. Claim Chain Link

Each claim links to the previous claim in the signer's chain, forming an append-only log:

```turtle
scatter_claim:{claimHash} scatter:previousClaim scatter_claim:{previousClaimHash} .
```

For the very first claim, the previous claim hash is a placeholder value (`yyyy...y`).

### 5. Prior-Use Acknowledgments

Every URI term (subject or object) used in the claim gets an **acknowledgment triple** that records which prior claim last used that term. This creates a cross-referencing web of provenance. Each acknowledgment has three parts:

```turtle
# The acknowledgment node records which term it refers to
<https://scatter.red/ack/{claimHash}/term/{termHash}> scatter:term <{termURI}> .

# It records the prior claim where this term was last seen
<https://scatter.red/ack/{claimHash}/term/{termHash}> scatter:priorClaim scatter_claim:{priorClaimHash} .

# The claim declares that it acknowledges this prior use
scatter_claim:{claimHash} scatter:acknowledgesPriorUse <https://scatter.red/ack/{claimHash}/term/{termHash}> .
```

The `{termHash}` is computed by hashing the canonical string representation of the term URI.

### 6. User Signature

The user signs the claim data with their Ed25519 private key:

```turtle
scatter_claim:{claimHash} scatter:userSignature "ed25519:{signatureInBase32z}" .
```

### 7. Server-Added Triples (SignedClaim)

When the server accepts and stores the claim, it adds:

```turtle
# A unique sequence ID
scatter_claim:{claimHash} scatter:seqID "{sequenceId}"^^xsd:string .

# The timestamp of acceptance
scatter_claim:{claimHash} prov:generatedAtTime "{ISO8601timestamp}"^^xsd:dateTime .

# The server's own signature over the user signature and sequence ID
scatter_claim:{claimHash} scatter:serverSignature "ed25519:{serverSignatureInBase32z}" .
```

## Claim Hash Computation

The claim hash is a deterministic hash computed from:

1. The sorted triple hashes (each triple's N-Triples form, hashed individually)
2. The attribution URI
3. The previous claim hash
4. The timestamp
5. The sequence ID

These are concatenated and hashed together to produce the claim's unique identifier.

## URI Reference

### Namespace Prefixes

| Prefix | Namespace URI |
|---|---|
| `rdf` | `http://www.w3.org/1999/02/22-rdf-syntax-ns#` |
| `prov` | `http://www.w3.org/ns/prov#` |
| `scatter` | `https://scatter.red/terms/v0/` |
| `scatter_claim` | `https://scatter.red/claim/` |
| `scatter_triple` | `https://scatter.red/triple/` |

### Scatterbase Identity URIs

| URI Pattern | Description |
|---|---|
| `https://scatter.red/claim/{hash}` | Identifies a specific claim by its hash |
| `https://scatter.red/triple/{hash}` | Identifies a specific reified triple by its hash |
| `https://scatter.red/ack/{claimHash}/term/{termHash}` | An acknowledgment node linking a term to its prior claim |
| `urn:ed25519:{publicKeyInBase32z}` | Identifies a public key |

### Scatterbase Vocabulary (`https://scatter.red/terms/v0/`)

| URI | Type | Description |
|---|---|---|
| `scatter:ClaimTriple` | Class | Type for reified triples within a claim |
| `scatter:SignedClaim` | Class | Type for a server-signed claim |
| `scatter:PublicKey` | Class | Type for a registered public key |
| `scatter:claimedIn` | Predicate | Links a reified triple to its claim |
| `scatter:previousClaim` | Predicate | Links a claim to the previous claim in the chain |
| `scatter:userSignature` | Predicate | The user's Ed25519 signature over the claim |
| `scatter:serverSignature` | Predicate | The server's Ed25519 signature over the claim |
| `scatter:seqID` | Predicate | The server-assigned sequence ID |
| `scatter:term` | Predicate | Links an acknowledgment node to the term it references |
| `scatter:priorClaim` | Predicate | Links an acknowledgment node to the prior claim for that term |
| `scatter:acknowledgesPriorUse` | Predicate | Links a claim to its term acknowledgment nodes |
| `scatter:hasPublicKey` | Predicate | Links a server identity to its public key |
| `scatter:keyType` | Predicate | Declares the algorithm of a public key |
| `scatter:ed25519` | Individual | The Ed25519 key type |
| `scatter:owner` | Predicate | Links a key to its owner |

### Standard Vocabulary Used

| URI | Description |
|---|---|
| `rdf:type` | RDF type predicate |
| `rdf:Statement` | RDF reification class |
| `rdf:subject` | RDF reification — the subject of a reified triple |
| `rdf:predicate` | RDF reification — the predicate of a reified triple |
| `rdf:object` | RDF reification — the object of a reified triple |
| `prov:wasAttributedTo` | PROV — links a claim to its author |
| `prov:generatedAtTime` | PROV — the timestamp when the claim was recorded |
