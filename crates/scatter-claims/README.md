# scatter-claims

Signed claims over RDF triples: what a claim is, how it is identified, what its signatures cover, and how it is written as RDF.

A **claim** is a set of triples asserted by one actor as the next link in that actor's chain of claims. This crate is the definition of it that [Scatterbase](https://github.com/scatterbase/scatterbase) is built on. It is pure: no I/O, no clock, no globals. Storage, key lookup and the policy for accepting a claim belong to the caller.

- **`Claim`** — the triples, the actor and the actor's previous claim. `Claim::id` is the one function that computes a claim ID. `Claim::from_parsed` turns triples as a parser gives them, with blank nodes and quoted triples, into the form a claim holds.
- **`Envelope`** — a claim, the prior use of each of its terms that the actor acknowledges, and the actor's Ed25519 signature over both. This is what a client prepares.
- **`SignedClaim`** — an envelope a server accepted, with the sequence ID and time the server assigned and the server's signature.
- **`Redacted`** — a signed claim of which parts have been erased, which still names the claim and carries a server's signature that verifies.
- **`rdf`** — the envelope as triples and quads, as N-Triples and Turtle, and read back from triples.

RDF terms are [`oxrdf`](https://crates.io/crates/oxrdf)'s. The format is specified in [docs/claim-envelopes.md](https://github.com/scatterbase/scatterbase/blob/main/docs/claim-envelopes.md).

## Example

```rust
use std::collections::BTreeMap;

use ed25519_dalek::SigningKey;
use oxrdf::{Literal, NamedNode, Triple};
use scatter_claims::{Claim, ClaimId, Envelope, SignedClaim};

let alice = NamedNode::new("https://example.org/alice")?;
let ada = NamedNode::new("https://example.org/ada")?;
let name = NamedNode::new("http://schema.org/name")?;

// The actor's first claim: one triple, following no claim
let claim = Claim::new(
    [Triple::new(ada.clone(), name, Literal::new_simple_literal("Ada Lovelace"))],
    alice,
    ClaimId::NONE,
)?;

// The actor acknowledges what it has seen of each term it uses, here nothing, and signs
let actor_key = SigningKey::from_bytes(&[1; 32]);
let acknowledgments = BTreeMap::from([(ada, ClaimId::NONE)]);
let envelope = Envelope::sign(claim, acknowledgments, &actor_key)?;
envelope.verify(&actor_key.verifying_key())?;

// A server accepts it at a place in its log, and signs that
let server_key = SigningKey::from_bytes(&[2; 32]);
let signed = SignedClaim::accept(envelope, "0", "2026-11-23T00:00:00.000Z", &server_key)?;
signed.verify(&server_key.verifying_key())?;

// As RDF, and back
let read = SignedClaim::from_triples(&signed.to_triples())?;
assert_eq!(read, signed);
println!("{}", signed.to_turtle());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Status

Not published, and the format is at `v0`: the preimages carry a version tag, `scatter-claim-v0`, and may change before a first release.

## License

GPL-3.0-or-later.
