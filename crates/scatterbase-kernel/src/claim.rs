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
use oxrdf::{NamedNode, NamedOrBlankNode, Term, Triple};
use oxttl::{NTriplesParser, TurtleParser};
use scatter_claims::{Acknowledgments, Claim, ClaimError, Envelope, SignedClaim};

use crate::instance::Instance;
use crate::keys::public_key_from_iri;
use crate::log::ClaimLog;
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
/// The envelope's signature must verify against the key its actor has registered, and the
/// envelope must have been prepared against the store as it is now ([`check_current`]). An
/// actor with no registered key is refused, with one exception: the first claim of an empty
/// log may register its own actor's key and is checked against that key, which is how a
/// server's key comes to be registered.
///
/// An accepted claim is appended to the claim log, where its offset is its sequence ID,
/// and the store is brought up to the log.
pub fn submit(envelope: Envelope, instance: &Instance, server_key: &SigningKey) -> io::Result<SignedClaim> {
    let mut log = ClaimLog::open(&instance.log_dir, &instance.domain)?;
    projection::catch_up(instance, &log)?;
    let store = Snapshot::load(&instance.storage_file)?;
    let claim = envelope.claim();
    let attribution = claim.attribution();
    let offset = log.size()?;

    let key = match store.public_key(attribution.as_ref()) {
        Some(key) => key,
        None => founding_key(offset, claim).ok_or_else(|| io::Error::new(
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
    check_current(&envelope, &store).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let signed = SignedClaim::accept(
        envelope,
        offset.to_string(),
        Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        server_key,
    ).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    log.append(&signed, &key, server_key)?;
    projection::catch_up(instance, &log)?;
    Ok(signed)
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

/// The key the first claim of an empty log registers for its own actor, if it does.
fn founding_key(offset: u64, claim: &Claim) -> Option<VerifyingKey> {
    if offset != 0 {
        return None;
    }
    let actor = NamedOrBlankNode::from(claim.attribution().clone());
    claim.triples().iter()
        .filter(|t| t.subject == actor && t.predicate.as_ref() == scatter_vocab::scatter::HAS_PUBLIC_KEY)
        .find_map(|t| match &t.object {
            Term::NamedNode(key_iri) => public_key_from_iri(key_iri.as_ref()),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxrdf::Literal;
    use scatter_claims::ClaimId;
    use std::io::Cursor;

    use crate::keys;

    fn iri(s: &str) -> NamedNode {
        NamedNode::new(s).unwrap()
    }

    const DOMAIN: &str = "http://example.org";

    /// An instance in a temporary directory whose server key is registered.
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
            let instance = Instance::new(DOMAIN, server.clone(), dir.path().join("store.nt").to_str().unwrap());
            let server_key = keys::generate();
            let fixture = Self { _dir: dir, instance, server, server_key };
            fixture.claim(keys::registration(&fixture.server, &fixture.server_key.verifying_key()), &fixture.server, &fixture.server_key).unwrap();
            fixture
        }

        fn store(&self) -> Snapshot {
            Snapshot::load(&self.instance.storage_file).unwrap()
        }

        fn log(&self) -> ClaimLog {
            ClaimLog::open(&self.instance.log_dir, DOMAIN).unwrap()
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
        assert_eq!([first.sequence_id(), by_bob.sequence_id(), second.sequence_id()], ["3", "4", "5"]);

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
    fn accepted_claims_are_log_records_under_a_signed_checkpoint() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        let signed = fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();

        let log = fixture.log();
        assert_eq!(log.size().unwrap(), 3);
        assert_eq!(log.origin(), "example.org/log/claim");
        let claims = log.claims(0).unwrap();
        assert_eq!(claims[2].0, 2);
        assert_eq!(claims[2].1.signed, signed);
        assert_eq!(claims[2].1.actor_key, alice_key.verifying_key());
        assert_eq!(claims[2].1.server_key, fixture.server_key.verifying_key());

        assert_eq!(log.verify(&fixture.server_key.verifying_key()).unwrap(), 3);
        assert!(log.verify(&keys::generate().verifying_key()).is_err());
    }

    #[test]
    fn the_store_is_rebuilt_from_the_log() {
        let fixture = Fixture::new();
        let alice = iri("http://example.org/alice");
        let alice_key = fixture.register(&alice);
        fixture.claim(vec![name("http://example.org/a", "A")], &alice, &alice_key).unwrap();
        let before = std::fs::read_to_string(&fixture.instance.storage_file).unwrap();

        std::fs::write(&fixture.instance.storage_file, "").unwrap();
        assert_eq!(projection::rebuild(&fixture.instance, &fixture.log()).unwrap(), 3);
        assert_eq!(std::fs::read_to_string(&fixture.instance.storage_file).unwrap(), before);

        // A store that fell behind is brought up to the log before the next claim is checked
        std::fs::remove_file(&fixture.instance.storage_file).unwrap();
        std::fs::remove_file(format!("{}.offset", fixture.instance.storage_file)).unwrap();
        let stale = fixture.prepare(vec![name("http://example.org/b", "B")], &alice, &alice_key);
        let error = submit(stale, &fixture.instance, &fixture.server_key).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(&fixture.instance.storage_file).unwrap(), before);
    }
}
