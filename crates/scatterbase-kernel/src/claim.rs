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
//!   registered key, refuses a claim the store already holds, assigns the sequence ID and
//!   the time, signs as the server and appends the triples.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

use chrono::{DateTime, SecondsFormat, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use oxrdf::{NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::{NTriplesParser, TurtleParser};
use scatter_claims::{Acknowledgments, Claim, ClaimError, Envelope, SignedClaim};

use crate::keys::public_key_from_iri;
use crate::lookup::Snapshot;
use crate::prefix::expand_prefix;
use crate::storage::create_storage;
use crate::uri::{parse_iri, scatter};

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

/// Accepts an envelope into the store and returns the signed claim.
///
/// The envelope's signature must verify against the key its actor has registered. An actor
/// with no registered key is refused, with one exception: the first claim of an empty
/// store may register its own actor's key and is checked against that key, which is how a
/// server's key comes to be registered.
pub fn submit(
    envelope: Envelope,
    domain: &str,
    storage_file: &str,
    server_key: &SigningKey,
) -> io::Result<SignedClaim> {
    let store = Snapshot::load(storage_file)?;
    let claim = envelope.claim();
    let attribution = claim.attribution();

    let key = match store.public_key(attribution.as_ref()) {
        Some(key) => key,
        None => founding_key(&store, claim).ok_or_else(|| io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "No public key found for attribution URI {attribution}. Register one first using: scatterbase key add --attribution {}",
                attribution.as_str(),
            ),
        ))?,
    };
    envelope.verify(&key).map_err(|e| io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{e} against the key registered for {attribution}"),
    ))?;

    let id = envelope.id();
    if store.contains_claim(id) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("A claim with the hash '{id}' already exists in the database."),
        ));
    }

    let now = Utc::now();
    let greatest = store.triples().iter()
        .filter(|t| t.predicate.as_str() == scatter::SEQ_ID)
        .filter_map(|t| match &t.object {
            Term::Literal(literal) => Some(literal.value()),
            _ => None,
        })
        .max();
    let signed = SignedClaim::accept(
        envelope,
        next_sequence_id(now, greatest),
        now.to_rfc3339_opts(SecondsFormat::Millis, true),
        server_key,
    ).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut storage = create_storage(storage_file);
    storage.init(domain)?;
    for triple in signed.to_triples() {
        storage.append(&triple)?;
    }
    Ok(signed)
}

/// The key the first claim of an empty store registers for its own actor, if it does.
fn founding_key(store: &Snapshot, claim: &Claim) -> Option<VerifyingKey> {
    if !store.is_empty() {
        return None;
    }
    let actor = NamedOrBlankNode::from(claim.attribution().clone());
    claim.triples().iter()
        .filter(|t| t.subject == actor && t.predicate.as_str() == scatter::HAS_PUBLIC_KEY)
        .find_map(|t| match &t.object {
            Term::NamedNode(key_iri) => public_key_from_iri(key_iri.as_ref()),
            _ => None,
        })
}

/// The sequence ID of a claim accepted at `now`: the time to the millisecond as
/// `YYYYMMDDHHMMSSmmm` followed by two digits, `00` unless that would not be greater than
/// the greatest sequence ID already in the store, in which case it is that ID plus one.
/// Sequence IDs therefore only ever increase.
pub fn next_sequence_id(now: DateTime<Utc>, greatest: Option<&str>) -> String {
    let from_clock = format!("{}00", now.format("%Y%m%d%H%M%S%3f"));
    match greatest {
        Some(greatest) if greatest >= from_clock.as_str() => match greatest.parse::<u128>() {
            Ok(n) => format!("{:019}", n + 1),
            Err(_) => from_clock,
        },
        _ => from_clock,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use oxrdf::Literal;
    use scatter_claims::ClaimId;
    use std::io::Cursor;

    use crate::keys;

    fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    const DOMAIN: &str = "http://example.org";

    /// A store in a temporary directory whose server key is registered.
    struct Fixture {
        _dir: tempfile::TempDir,
        storage_file: String,
        server: NamedNode,
        server_key: SigningKey,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let storage_file = dir.path().join("store.nt").to_str().unwrap().to_string();
            let server = iri("http://example.org/Server");
            let server_key = keys::generate();
            let fixture = Self { _dir: dir, storage_file, server, server_key };
            fixture.claim(keys::registration(&fixture.server, &fixture.server_key.verifying_key()), &fixture.server, &fixture.server_key).unwrap();
            fixture
        }

        fn store(&self) -> Snapshot {
            Snapshot::load(&self.storage_file).unwrap()
        }

        fn prepare(&self, triples: Vec<Triple>, actor: &NamedNode, key: &SigningKey) -> Envelope {
            prepare(triples, actor.clone(), DOMAIN, &self.store(), key).unwrap()
        }

        fn claim(&self, triples: Vec<Triple>, actor: &NamedNode, key: &SigningKey) -> io::Result<SignedClaim> {
            submit(self.prepare(triples, actor, key), DOMAIN, &self.storage_file, &self.server_key)
        }

        /// Registers a new key for an actor, as `key add` does, and returns it.
        fn register(&self, actor: &NamedNode) -> SigningKey {
            let key = keys::generate();
            self.claim(keys::registration(actor, &key.verifying_key()), &self.server, &self.server_key).unwrap();
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
    fn the_first_claim_registers_the_server_key_and_reads_back() {
        let fixture = Fixture::new();
        let store = fixture.store();
        assert_eq!(store.public_key(fixture.server.as_ref()), Some(fixture.server_key.verifying_key()));

        let signed = SignedClaim::from_triples(store.triples()).unwrap();
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
        assert!(second.sequence_id() > by_bob.sequence_id() && by_bob.sequence_id() > first.sequence_id());

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
        let mallory = iri("http://example.org/mallory");
        let key = keys::generate();

        let error = fixture.claim(vec![name("http://example.org/a", "A")], &mallory, &key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);

        // Registering one's own key works only for the first claim of an empty store
        let error = fixture.claim(keys::registration(&mallory, &key.verifying_key()), &mallory, &key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn the_same_envelope_is_accepted_once() {
        let fixture = Fixture::new();
        let envelope = fixture.prepare(vec![name("http://example.org/a", "A")], &fixture.server, &fixture.server_key);
        submit(envelope.clone(), DOMAIN, &fixture.storage_file, &fixture.server_key).unwrap();
        let error = submit(envelope, DOMAIN, &fixture.storage_file, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn a_registered_key_can_be_replaced() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let old = fixture.register(&alice);
        let new = fixture.register(&alice);
        assert_eq!(fixture.store().public_key(alice.as_ref()), Some(new.verifying_key()));
        assert!(fixture.claim(vec![name("http://example.org/a", "A")], &alice, &old).is_err());
        assert!(fixture.claim(vec![name("http://example.org/a", "A")], &alice, &new).is_ok());
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
    fn sequence_ids_follow_the_clock_and_never_go_back() {
        let now = Utc.with_ymd_and_hms(2026, 11, 23, 12, 0, 0).unwrap();
        assert_eq!(next_sequence_id(now, None), "2026112312000000000");
        assert_eq!(next_sequence_id(now, Some("2026112311595999900")), "2026112312000000000");
        assert_eq!(next_sequence_id(now, Some("2026112312000000000")), "2026112312000000001");
        assert_eq!(next_sequence_id(now, Some("2026112312000000099")), "2026112312000000100");
        assert_eq!(next_sequence_id(now, Some("2027010100000000000")), "2027010100000000001");
    }
}
