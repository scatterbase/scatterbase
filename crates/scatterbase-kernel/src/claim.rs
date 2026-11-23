//! Reading triples, and preparing and accepting claims.
//!
//! What a claim is, how it is identified and what its signatures cover is
//! [`scatter_claims`]'s. This module does what needs the store and the clock:
//!
//! - [`parse_turtle`], [`parse_ntriples`] and the two `read_triples_from_*` functions read
//!   triples as they are written, with prefixes expanded;
//! - [`prepare`] makes a signed envelope of them: it finds the actor's previous claim and
//!   the prior use of each term in the store;
//! - [`submit`] accepts an envelope: it checks the actor's signature against the actor's
//!   registered key, refuses a claim the store already holds or one prepared against an
//!   older state of the store ([`check_current`]), signs as the server, appends the claim
//!   to the claim log ([`crate::log`]) and brings the store up to the log.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

use chrono::{SecondsFormat, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use scatter_actors::time::Timestamp;
use oxrdf::{NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::{NTriplesParser, TurtleParser};
use scatter_claims::{Acknowledgments, Claim, ClaimError, ClaimId, Envelope, SignedClaim};

use crate::instance::Instance;
use crate::log::{ClaimLog, ClaimPart, Reason};
use crate::lookup::Snapshot;
use crate::prefix::expand_prefix;
use crate::projection;
use crate::uri::parse_iri;

/// Reads triples from a file: Turtle if its name ends in `.ttl`, N-Triples otherwise.
pub fn read_triples_from_file(file_path: &str, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    let reader = BufReader::new(File::open(file_path)?);

    if file_path.ends_with(".ttl") {
        parse_turtle(reader, prefixes)
    } else {
        parse_ntriples(reader, prefixes)
    }
}

/// Reads triples from stdin, as Turtle, which N-Triples is a subset of.
pub fn read_triples_from_stdin(prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    parse_turtle(BufReader::new(io::stdin()), prefixes)
}

/// Parses N-Triples.
pub fn parse_ntriples<R: BufRead>(reader: R, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    expand_parsed(NTriplesParser::new().for_reader(reader), prefixes)
}

/// Parses Turtle.
pub fn parse_turtle<R: BufRead>(reader: R, prefixes: &HashMap<String, String>) -> io::Result<Vec<Triple>> {
    expand_parsed(TurtleParser::new().for_reader(reader), prefixes)
}

/// Collects parsed triples, expanding a prefix from the prefix file where an IRI in subject
/// or object position is written with one, as in `<schema:name>`.
///
/// Blank nodes and quoted triples are left as the parser gives them:
/// [`Claim::from_parsed`] replaces them when the triples are claimed.
fn expand_parsed<E>(
    parsed: impl Iterator<Item = Result<Triple, E>>,
    prefixes: &HashMap<String, String>,
) -> io::Result<Vec<Triple>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    parsed
        .map(|triple| {
            let triple = triple.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            expand_triple(triple, prefixes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
        .collect()
}

fn expand_triple(triple: Triple, prefixes: &HashMap<String, String>) -> Result<Triple, String> {
    let expand = |iri: NamedNode| parse_iri(&expand_prefix(iri.as_str(), prefixes));

    let subject = match triple.subject {
        NamedOrBlankNode::NamedNode(iri) => expand(iri)?.into(),
        blank => blank,
    };
    let object = match triple.object {
        Term::NamedNode(iri) => expand(iri)?.into(),
        Term::Triple(quoted) => Term::Triple(Box::new(expand_triple(*quoted, prefixes)?)),
        other => other,
    };
    Ok(Triple::new(subject, triple.predicate, object))
}

/// Prepares a claim of parsed triples by `attribution`, signed with that actor's key.
///
/// The claim follows the actor's latest claim in the store, and acknowledges for each of
/// its terms the claim in the store that last used the term. Blank nodes are given skolem
/// IRIs under `domain`.
pub fn prepare(
    parsed: Vec<Triple>,
    attribution: NamedNode,
    domain: &str,
    store: &Snapshot,
    key: &SigningKey,
) -> Result<Envelope, ClaimError> {
    let previous = store.latest_claim_by(attribution.as_ref());
    let claim = Claim::from_parsed(parsed, attribution, previous, domain)?;
    let acknowledgments: Acknowledgments = claim.terms().into_iter()
        .map(|term| {
            let prior = store.latest_claim_using(term.as_ref());
            (term, prior)
        })
        .collect();
    Envelope::sign(claim, acknowledgments, key)
}

/// Accepts an envelope and returns the signed claim.
///
/// The envelope's signature must verify against a key of its actor's, and the envelope must
/// have been prepared against the store as it is now ([`check_current`]). The server's key
/// is the current key of the log's server partition. Any other actor's keys are those the
/// server registered for it and has not revoked ([`ClaimLog::register_key`]), and an actor
/// with none is refused.
///
/// An accepted claim is appended to the claim partition of the log, where its offset is its
/// sequence ID, and the store is brought up to the log.
pub fn submit(envelope: Envelope, instance: &Instance, server_key: &SigningKey) -> io::Result<SignedClaim> {
    let mut log = ClaimLog::open(instance)?;
    projection::catch_up(instance, &log)?;
    let store = Snapshot::load(instance)?;
    let claim = envelope.claim();
    let attribution = claim.attribution();
    let offset = log.size()?;

    // The server's key is the one its partition of the log registers; any other actor's
    // are the keys the server registered for it in the log
    let now = Utc::now();
    let key = if *attribution == instance.server {
        let key = log.server_key();
        envelope.verify(&key).map_err(|e| io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{e} against the server's key"),
        ))?;
        key
    } else {
        let at = Timestamp(u64::try_from(now.timestamp_micros()).unwrap_or(0));
        let keys = log.current_keys(attribution)?;
        if !keys.iter().any(|key| key.is_valid_at(at)) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "No public key found for attribution URI {attribution}. Register one first using: scatterbase key add --attribution {}",
                    attribution.as_str(),
                ),
            ));
        }
        let preimage = envelope.signature_preimage();
        keys.iter()
            .find(|key| key.verify(preimage.as_bytes(), envelope.signature().as_bytes(), at).is_ok())
            .and_then(|key| VerifyingKey::from_bytes(&key.public_key.0).ok())
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the client signature does not verify against a key registered for {attribution}"),
            ))?
    };

    let id = envelope.id();
    if store.contains_claim(id) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("A claim with the hash '{id}' already exists in the database."),
        ));
    }
    check_current(&envelope, &store).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let signed = SignedClaim::accept(
        envelope,
        offset.to_string(),
        now.to_rfc3339_opts(SecondsFormat::Millis, true),
        server_key,
    ).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    log.append(&signed, &key, server_key)?;
    projection::catch_up(instance, &log)?;
    Ok(signed)
}

/// Erases parts of an accepted claim, for good, and writes the store again without them.
/// Returns the parts this call erased ([`ClaimLog::erase_claim`]).
pub fn erase_claim(
    instance: &Instance,
    id: ClaimId,
    parts: &[ClaimPart],
    reason: Reason<'_>,
    server_key: &SigningKey,
) -> io::Result<Vec<ClaimPart>> {
    let mut log = ClaimLog::open(instance)?;
    let erased = log.erase_claim(id, parts, reason, server_key)?;
    if !erased.is_empty() {
        // What the erased parts put in the store may be shared with other claims, so the
        // store is written again from what the log still holds
        projection::rebuild(instance, &log)?;
    }
    Ok(erased)
}

/// Erases an actor from the log's mutable partition, for good, and takes its keys out of
/// the store. Returns the actor's surrogate, or `None` if it had none
/// ([`ClaimLog::erase_actor`]).
pub fn erase_actor(
    instance: &Instance,
    actor: &NamedNode,
    reason: Reason<'_>,
    server_key: &SigningKey,
) -> io::Result<Option<u64>> {
    let mut log = ClaimLog::open(instance)?;
    let surrogate = log.erase_actor(actor, reason, server_key)?;
    projection::catch_up(instance, &log)?;
    Ok(surrogate)
}

/// Checks that an envelope was prepared against the store as it is now: that its claim
/// follows the actor's latest claim, and that it acknowledges, for each term, the claim
/// that last used the term.
///
/// An envelope that fails was prepared before another claim was accepted. Nothing is wrong
/// with its content: preparing it again gives one that passes.
pub fn check_current(envelope: &Envelope, store: &Snapshot) -> Result<(), String> {
    let claim = envelope.claim();
    let latest = store.latest_claim_by(claim.attribution().as_ref());
    if claim.previous() != latest {
        return Err(format!(
            "the claim follows {}, but the latest claim by {} is {}. Prepare the claim again.",
            claim.previous(), claim.attribution(), latest,
        ));
    }
    for (term, acknowledged) in envelope.acknowledgments() {
        let latest = store.latest_claim_using(term.as_ref());
        if *acknowledged != latest {
            return Err(format!(
                "the claim acknowledges {acknowledged} as the last use of {term}, but it was last used in {latest}. Prepare the claim again.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::Literal;
    use std::io::Cursor;

    use crate::instance::{GRAPH_CLAIM, GRAPH_META, GRAPH_MUTABLE, GRAPH_SERVER};
    use crate::keys;

    fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    const DOMAIN: &str = "http://example.org";

    /// An instance in a temporary directory with its log created and no claims.
    struct Fixture {
        _dir: tempfile::TempDir,
        instance: Instance,
        server: NamedNode,
        server_key: SigningKey,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let server = iri("http://example.org/Server");
            let instance = Instance::new(DOMAIN, server.clone(), dir.path().join("store.nq").to_str().unwrap());
            let server_key = keys::generate();
            ClaimLog::create(&instance, &server_key).unwrap();
            Self { _dir: dir, instance, server, server_key }
        }

        fn store(&self) -> Snapshot {
            Snapshot::load(&self.instance).unwrap()
        }

        fn log(&self) -> ClaimLog {
            ClaimLog::open(&self.instance).unwrap()
        }

        fn prepare(&self, triples: Vec<Triple>, actor: &NamedNode, key: &SigningKey) -> Envelope {
            prepare(triples, actor.clone(), DOMAIN, &self.store(), key).unwrap()
        }

        fn claim(&self, triples: Vec<Triple>, actor: &NamedNode, key: &SigningKey) -> io::Result<SignedClaim> {
            submit(self.prepare(triples, actor, key), &self.instance, &self.server_key)
        }

        /// Registers a new key for an actor, as `key add` does, and returns it.
        fn register(&self, actor: &NamedNode) -> SigningKey {
            let key = keys::generate();
            self.log().register_key(actor, &key.verifying_key(), &self.server_key).unwrap();
            key
        }
    }

    fn name(subject: &str, value: &str) -> Triple {
        Triple::new(iri(subject), iri("http://schema.org/name"), Literal::new_simple_literal(value))
    }

    #[test]
    fn parsing_expands_prefixes_and_keeps_blank_nodes() {
        let prefixes: HashMap<String, String> = [("schema".to_string(), "http://schema.org/".to_string())].into();
        let triples = parse_ntriples(
            Cursor::new("<http://example.org/a> <http://schema.org/knows> <schema:Person> .\n_:x <http://schema.org/name> \"X\" .\n"),
            &prefixes,
        ).unwrap();
        assert_eq!(triples[0].object, Term::from(iri("http://schema.org/Person")));
        assert!(triples[1].subject.is_blank_node());

        assert!(parse_ntriples(Cursor::new("not n-triples\n"), &prefixes).is_err());
        assert_eq!(parse_turtle(Cursor::new("@prefix ex: <http://example.org/> . ex:a ex:p ex:b ."), &prefixes).unwrap().len(), 1);
    }

    #[test]
    fn the_first_claim_follows_none_and_reads_back() {
        let fixture = Fixture::new();
        let accepted = fixture.claim(vec![name("http://example.org/a", "A")], &fixture.server, &fixture.server_key).unwrap();
        assert_eq!(accepted.sequence_id(), "0");

        let store = fixture.store();
        let signed = SignedClaim::from_triples(&store.meta()).unwrap();
        assert_eq!(signed, accepted);
        assert_eq!(signed.envelope().claim().previous(), ClaimId::NONE);
        assert_eq!(signed.envelope().verify(&fixture.server_key.verifying_key()), Ok(()));
        assert_eq!(signed.verify(&fixture.server_key.verifying_key()), Ok(()));
        assert_eq!(store.latest_claim_by(fixture.server.as_ref()), signed.id());
    }

    #[test]
    fn a_claim_follows_the_actors_last_and_acknowledges_prior_use() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let bob = iri("http://example.org/bob");
        let alice_key = fixture.register(&alice);
        let bob_key = fixture.register(&bob);

        let first = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();
        assert_eq!(first.envelope().claim().previous(), ClaimId::NONE);
        assert_eq!(first.envelope().acknowledgments()[&iri("http://example.org/a")], ClaimId::NONE);

        let by_bob = fixture.claim(vec![name("http://example.org/a", "Ay")], &bob, &bob_key).unwrap();
        assert_eq!(by_bob.envelope().acknowledgments()[&iri("http://example.org/a")], first.id());

        let second = fixture.claim(vec![name("http://example.org/a", "A again")], &alice, &alice_key).unwrap();
        assert_eq!(second.envelope().claim().previous(), first.id());
        assert_eq!(second.envelope().acknowledgments()[&iri("http://example.org/a")], by_bob.id());
        assert_eq!([first.sequence_id(), by_bob.sequence_id(), second.sequence_id()], ["0", "1", "2"]);

        let store = fixture.store();
        assert_eq!(store.latest_claim_by(alice.as_ref()), second.id());
        assert_eq!(store.latest_claim_using(iri("http://example.org/a").as_ref()), second.id());
        assert_eq!(store.latest_claim_using(iri("http://example.org/never").as_ref()), ClaimId::NONE);
    }

    #[test]
    fn a_claim_signed_with_the_wrong_key_is_refused() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        fixture.register(&alice);

        let error = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &keys::generate()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("signature does not verify"), "{error}");
        assert_eq!(fixture.store().latest_claim_by(alice.as_ref()), ClaimId::NONE);
    }

    #[test]
    fn an_actor_with_no_registered_key_is_refused() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let mallory = iri("http://example.org/mallory");
        let alice_key = fixture.register(&alice);
        let key = keys::generate();

        let error = fixture.claim(vec![name("http://example.org/a", "A")], &mallory, &key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        // A key is registered by a record of the server's, never by what anyone claims
        fixture.claim(keys::registration(&mallory, &key.verifying_key()), &alice, &alice_key).unwrap();
        fixture.claim(keys::registration(&mallory, &key.verifying_key()), &fixture.server, &fixture.server_key).unwrap();
        assert!(fixture.log().current_keys(&mallory).unwrap().is_empty());
        let error = fixture.claim(vec![name("http://example.org/a", "A")], &mallory, &key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn the_servers_key_is_the_one_its_partition_registers() {
        let fixture = Fixture::new();
        let impostor = keys::generate();

        // A claim as the server signed with another key is refused
        let error = fixture.claim(vec![name("http://example.org/a", "A")], &fixture.server, &impostor).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        // A key registered for the server's IRI as for any actor does not sign for it
        fixture.log().register_key(&fixture.server, &impostor.verifying_key(), &fixture.server_key).unwrap();
        let error = fixture.claim(vec![name("http://example.org/a", "A")], &fixture.server, &impostor).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        // And a server holding another key can neither accept into this log nor register in it
        let envelope = fixture.prepare(vec![name("http://example.org/a", "A")], &fixture.server, &fixture.server_key);
        let error = submit(envelope, &fixture.instance, &impostor).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let error = fixture.log().register_key(&iri("http://example.org/alice"), &impostor.verifying_key(), &impostor).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        let log = fixture.log();
        assert_eq!(log.server_key(), fixture.server_key.verifying_key());
        let graphs = log.graphs().unwrap();
        assert_eq!(graphs.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["server", "claim", "mutable", "meta"]);
        assert_eq!(graphs[0].1, Some(crate::log::SERVER_PARTITION));
        assert!(graphs[1].1.is_some() && graphs[2].1.is_some() && graphs[1].1 != graphs[2].1);
        assert_eq!(graphs[3].1, None);
        assert!(ClaimLog::create(&fixture.instance, &fixture.server_key).is_err());
    }

    #[test]
    fn an_actor_can_have_several_keys_and_a_revoked_key_stops_signing() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let first = fixture.register(&alice);
        let second = fixture.register(&alice);

        // Either key signs
        fixture.claim(vec![name("http://example.org/a", "A")], &alice, &first).unwrap();
        fixture.claim(vec![name("http://example.org/b", "B")], &alice, &second).unwrap();
        assert_eq!(fixture.log().current_keys(&alice).unwrap().len(), 2);

        // Revoked, neither does, and a key registered afterwards does
        assert_eq!(fixture.log().revoke_keys(&alice, &fixture.server_key).unwrap().len(), 2);
        assert!(fixture.log().current_keys(&alice).unwrap().is_empty());
        let error = fixture.claim(vec![name("http://example.org/c", "C")], &alice, &first).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let third = fixture.register(&alice);
        assert!(fixture.claim(vec![name("http://example.org/c", "C")], &alice, &second).is_err());
        fixture.claim(vec![name("http://example.org/c", "C")], &alice, &third).unwrap();

        // The claims the revoked keys signed are still in the log and still verify
        assert_eq!(fixture.log().verify(&fixture.server_key.verifying_key()).unwrap(), 3);
        assert_eq!(fixture.log().key_records(&alice).unwrap().len(), 5);
    }

    #[test]
    fn claims_and_keys_are_stored_in_four_graphs() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        fixture.claim(vec![name("http://example.org/s", "S")], &fixture.server, &fixture.server_key).unwrap();
        let signed = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();
        let store = fixture.store();
        let graph = |name: &str| store.triples_in(&fixture.instance.graph(name));

        assert_eq!(graph(GRAPH_SERVER), vec![name("http://example.org/s", "S")]);
        assert_eq!(graph(GRAPH_CLAIM), vec![name("http://example.org/a", "A")]);
        assert_eq!(graph(GRAPH_MUTABLE), keys::registration(&alice, &alice_key.verifying_key()));
        // Everything about both claims, and nothing outside the four graphs
        assert_eq!(store.meta().len() + 1 + 1 + 4, store.quads().len());
        assert_eq!(fixture.instance.graph(GRAPH_META).as_str(), "http://example.org/graph/meta");
        assert!(store.meta().contains(&Triple::new(signed.id().iri(), scatter_vocab::prov::WAS_ATTRIBUTED_TO, alice.clone())));

        // A revoked key leaves the graph
        fixture.log().revoke_keys(&alice, &fixture.server_key).unwrap();
        projection::catch_up(&fixture.instance, &fixture.log()).unwrap();
        assert!(fixture.store().triples_in(&fixture.instance.graph(GRAPH_MUTABLE)).is_empty());
    }

    #[test]
    fn what_an_actor_claims_is_not_taken_for_a_key_or_for_bookkeeping() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let mallory = iri("http://example.org/mallory");
        let alice_key = fixture.register(&alice);
        let mallory_key = fixture.register(&mallory);
        let by_alice = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();

        // Mallory claims that her key is Alice's, that Alice's claim came later than it
        // did, and that a claim of Alice's exists that does not
        let forged = scatter_claims::ClaimId::from_bytes([9; 32]);
        let mut triples = keys::registration(&alice, &mallory_key.verifying_key());
        triples.push(Triple::new(by_alice.id().iri(), scatter_vocab::scatter::SEQ_ID, Literal::new_simple_literal("999999")));
        triples.push(Triple::new(forged.iri(), scatter_vocab::prov::WAS_ATTRIBUTED_TO, alice.clone()));
        triples.push(Triple::new(forged.iri(), scatter_vocab::scatter::SEQ_ID, Literal::new_simple_literal("999998")));
        let by_mallory = fixture.claim(triples, &mallory, &mallory_key).unwrap();

        let store = fixture.store();
        assert_eq!(store.triples_in(&fixture.instance.graph(GRAPH_MUTABLE)).len(), 8);
        assert_eq!(store.sequence_id(by_alice.id()), Some(0));
        assert_eq!(store.latest_claim_by(alice.as_ref()), by_alice.id());
        assert!(!store.contains_claim(forged));
        assert_eq!(store.latest_claim_using(alice.as_ref()), by_mallory.id());

        // Alice's key still signs for Alice, and Mallory's does not
        assert!(fixture.claim(vec![name("http://example.org/b", "B")], &alice, &mallory_key).is_err());
        fixture.claim(vec![name("http://example.org/b", "B")], &alice, &alice_key).unwrap();
    }

    #[test]
    fn the_same_envelope_is_accepted_once() {
        let fixture = Fixture::new();
        let envelope = fixture.prepare(vec![name("http://example.org/a", "A")], &fixture.server, &fixture.server_key);
        submit(envelope.clone(), &fixture.instance, &fixture.server_key).unwrap();
        let error = submit(envelope, &fixture.instance, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn an_envelope_behind_the_actors_chain_is_refused() {
        let fixture = Fixture::new();
        let first = fixture.prepare(vec![name("http://example.org/a", "A")], &fixture.server, &fixture.server_key);
        let second = fixture.prepare(vec![name("http://example.org/b", "B")], &fixture.server, &fixture.server_key);
        submit(first, &fixture.instance, &fixture.server_key).unwrap();

        let error = submit(second, &fixture.instance, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("the latest claim by <http://example.org/Server>"), "{error}");

        // Prepared again, the same triples are accepted
        fixture.claim(vec![name("http://example.org/b", "B")], &fixture.server, &fixture.server_key).unwrap();
    }

    #[test]
    fn an_envelope_with_a_stale_acknowledgment_is_refused() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let bob = iri("http://example.org/bob");
        let alice_key = fixture.register(&alice);
        let bob_key = fixture.register(&bob);

        // Alice prepares a claim about a term, and Bob's claim about it is accepted first
        let by_alice = fixture.prepare(vec![name("http://example.org/a", "A")], &alice, &alice_key);
        fixture.claim(vec![name("http://example.org/a", "Ay")], &bob, &bob_key).unwrap();

        let error = submit(by_alice, &fixture.instance, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(error.to_string().contains("as the last use of <http://example.org/a>"), "{error}");
        assert_eq!(fixture.store().latest_claim_by(alice.as_ref()), ClaimId::NONE);

        // A claim about another term is not affected by Bob's
        fixture.claim(vec![name("http://example.org/c", "C")], &alice, &alice_key).unwrap();
    }

    #[test]
    fn blank_nodes_are_skolemized_under_the_domain() {
        let fixture = Fixture::new();
        let parsed = parse_turtle(Cursor::new("_:x <http://schema.org/name> \"blank\" ."), &HashMap::new()).unwrap();
        let signed = fixture.claim(parsed, &fixture.server, &fixture.server_key).unwrap();
        let subject = signed.envelope().claim().triples()[0].subject.to_string();
        assert!(subject.starts_with("<http://example.org/.well-known/genid/") && subject.ends_with("/x>"), "{subject}");
        assert!(!subject.contains("/current/"));
    }

    #[test]
    fn accepted_claims_are_log_records_under_a_signed_checkpoint() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        fixture.claim(vec![name("http://example.org/s", "S")], &fixture.server, &fixture.server_key).unwrap();
        let signed = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();

        let log = fixture.log();
        assert_eq!(log.size().unwrap(), 2);
        assert_eq!(log.origin(), "example.org/log/claim");
        let claims = log.claims(0).unwrap();
        assert_eq!(claims[1].offset, 1);
        assert_eq!(claims[1].signed(), Some(signed));
        assert_eq!(claims[1].actor_key, Some(alice_key.verifying_key()));
        assert_eq!(claims[1].server_key, fixture.server_key.verifying_key());

        assert_eq!(log.verify(&fixture.server_key.verifying_key()).unwrap(), 2);
        assert!(log.verify(&keys::generate().verifying_key()).is_err());
    }

    const PRIVACY: Reason<'static> = Reason { class: "privacy", authority: "ticket:1", comment: Some("on request") };

    #[test]
    fn an_erased_claim_keeps_its_place_and_loses_the_parts_erased() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        let first = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();
        let graph = |name: &str| fixture.store().triples_in(&fixture.instance.graph(name));
        let about = |claim: &SignedClaim, predicate: oxrdf::NamedNodeRef<'_>| {
            fixture.store().meta().iter().any(|t| t.subject == claim.id().iri().into() && t.predicate.as_ref() == predicate)
        };

        // What was claimed goes; who claimed it, and when, stays
        let erased = erase_claim(&fixture.instance, first.id(), &[ClaimPart::Content], PRIVACY, &fixture.server_key).unwrap();
        assert_eq!(erased, [ClaimPart::Content]);
        assert!(graph(GRAPH_CLAIM).is_empty());
        assert!(!fixture.store().meta().iter().any(|t| t.predicate.as_ref() == oxrdf::vocab::rdf::SUBJECT));
        assert!(about(&first, scatter_vocab::prov::WAS_ATTRIBUTED_TO) && about(&first, scatter_vocab::scatter::SEQ_ID));
        assert!(!about(&first, scatter_vocab::scatter::PREVIOUS_CLAIM));

        // The claim is still the actor's latest, so her chain goes on from it
        let second = fixture.claim(vec![name("http://example.org/b", "B")], &alice, &alice_key).unwrap();
        assert_eq!(second.envelope().claim().previous(), first.id());
        assert_eq!(second.sequence_id(), "2", "the erase record took offset 1");

        // Erasing it again erases nothing; the rest goes on request
        assert!(erase_claim(&fixture.instance, first.id(), &[ClaimPart::Content], PRIVACY, &fixture.server_key).unwrap().is_empty());
        let erased = erase_claim(&fixture.instance, first.id(), &ClaimPart::ALL, PRIVACY, &fixture.server_key).unwrap();
        assert_eq!(erased, [ClaimPart::Attestation, ClaimPart::Acknowledgments]);

        // What is left: that a claim was accepted, its ID, its place and the server's signature
        let stored = fixture.log().claim(first.id()).unwrap().unwrap();
        assert_eq!(stored.erased(), ClaimPart::ALL);
        assert_eq!((stored.offset, stored.signed()), (0, None));
        assert_eq!(stored.claim.acceptance, first.acceptance());
        assert_eq!(stored.claim.acceptance.verify(&fixture.server_key.verifying_key()), Ok(()));
        let left: Vec<Triple> = fixture.store().meta().into_iter().filter(|t| t.subject == first.id().iri().into()).collect();
        assert_eq!(left.len(), 4);
        assert!(fixture.store().contains_claim(first.id()));
        assert_eq!(fixture.store().latest_claim_by(alice.as_ref()), second.id());

        // The log verifies, every erasure accounted for, and counts both claims
        assert_eq!(fixture.log().verify(&fixture.server_key.verifying_key()).unwrap(), 2);
        // The erased claim cannot be brought back by submitting it again
        let error = submit(first.envelope().clone(), &fixture.instance, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn a_claim_with_its_attestation_erased_is_nobodys() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        let by_server = fixture.claim(vec![name("http://example.org/s", "S")], &fixture.server, &fixture.server_key).unwrap();
        let by_alice = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();

        for claim in [&by_server, &by_alice] {
            erase_claim(&fixture.instance, claim.id(), &[ClaimPart::Attestation], PRIVACY, &fixture.server_key).unwrap();
        }
        let store = fixture.store();
        // What was claimed is still there, the server's now in the claim graph with the rest
        assert!(store.triples_in(&fixture.instance.graph(GRAPH_SERVER)).is_empty());
        assert_eq!(store.triples_in(&fixture.instance.graph(GRAPH_CLAIM)).len(), 2);
        assert_eq!(store.latest_claim_using(iri("http://example.org/a").as_ref()), by_alice.id());
        // Nobody is said to have claimed it, and no signature or key of theirs is kept
        assert!(!store.meta().iter().any(|t| t.predicate.as_ref() == scatter_vocab::prov::WAS_ATTRIBUTED_TO));
        assert!(!store.meta().iter().any(|t| t.predicate.as_ref() == scatter_vocab::scatter::USER_SIGNATURE));
        assert_eq!(store.latest_claim_by(alice.as_ref()), ClaimId::NONE);
        assert_eq!(fixture.log().claim(by_alice.id()).unwrap().unwrap().actor_key, None);
        assert_eq!(fixture.log().verify(&fixture.server_key.verifying_key()).unwrap(), 2);
    }

    #[test]
    fn an_erasure_needs_a_claim_a_reason_and_the_servers_key() {
        let fixture = Fixture::new();
        let claim = fixture.claim(vec![name("http://example.org/s", "S")], &fixture.server, &fixture.server_key).unwrap();

        let error = erase_claim(&fixture.instance, ClaimId::from_bytes([7; 32]), &ClaimPart::ALL, PRIVACY, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        let whim = Reason { class: "whim", ..PRIVACY };
        let error = erase_claim(&fixture.instance, claim.id(), &ClaimPart::ALL, whim, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        let error = erase_claim(&fixture.instance, claim.id(), &ClaimPart::ALL, PRIVACY, &keys::generate()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        // Nothing was erased by any of them
        assert_eq!(fixture.log().claim(claim.id()).unwrap().unwrap().signed(), Some(claim));
        assert_eq!(fixture.log().size().unwrap(), 1);
        assert_eq!(ClaimPart::named("attestation"), Some(ClaimPart::Attestation));
        assert_eq!(ClaimPart::named("acceptance"), None);
    }

    #[test]
    fn an_erased_actor_has_no_keys_and_its_claims_remain() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        let claim = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();

        assert_eq!(erase_actor(&fixture.instance, &alice, PRIVACY, &fixture.server_key).unwrap(), Some(1));
        assert_eq!(erase_actor(&fixture.instance, &alice, PRIVACY, &fixture.server_key).unwrap(), None);

        // The surrogate stands for nobody, and the keys are gone from the log and the store
        assert_eq!(fixture.log().surrogates().unwrap(), [(1, None)]);
        assert!(fixture.log().current_keys(&alice).unwrap().is_empty());
        assert!(fixture.store().triples_in(&fixture.instance.graph(GRAPH_MUTABLE)).is_empty());
        let error = fixture.claim(vec![name("http://example.org/b", "B")], &alice, &alice_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        // Her claim is untouched and the log verifies; a key registered now is under a new number
        assert_eq!(fixture.log().claim(claim.id()).unwrap().unwrap().signed(), Some(claim));
        assert_eq!(fixture.log().verify(&fixture.server_key.verifying_key()).unwrap(), 1);
        fixture.register(&alice);
        assert_eq!(fixture.log().surrogate_of(&alice).unwrap(), Some(2));
        assert_eq!(fixture.log().verify(&fixture.server_key.verifying_key()).unwrap(), 1);
    }

    #[test]
    fn the_store_is_rebuilt_from_the_log() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();
        // The store's quads, whatever order they were written in
        let quads = || {
            let text = std::fs::read_to_string(&fixture.instance.storage_file).unwrap();
            let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
            lines.sort();
            lines
        };
        let before = quads();
        assert!(before.iter().any(|line| line.ends_with("/graph/mutable> .")));

        std::fs::write(&fixture.instance.storage_file, "").unwrap();
        assert_eq!(projection::rebuild(&fixture.instance, &fixture.log()).unwrap(), 1);
        assert_eq!(quads(), before);

        // A store that fell behind is brought up to the log before the next claim is checked
        std::fs::remove_file(&fixture.instance.storage_file).unwrap();
        std::fs::remove_file(format!("{}.offset", fixture.instance.storage_file)).unwrap();
        let stale = fixture.prepare(vec![name("http://example.org/b", "B")], &alice, &alice_key);
        let error = submit(stale, &fixture.instance, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(quads(), before);
    }
}
