//! The claims a new instance starts with.

use std::io;

use ed25519_dalek::SigningKey;
use oxrdf::{NamedNode, Triple};

use crate::claim::{prepare, submit};
use crate::instance::Instance;
use crate::keys;
use crate::lookup::Snapshot;
use scatter_vocab::{rdf, schema, scatter};

/// The triple that declares the domain a dataset: `{domain} rdf:type schema:Dataset`.
pub fn create_initial_triple(domain: &str) -> Triple {
    Triple::new(NamedNode::new(domain).unwrap(), rdf::TYPE, schema::DATASET)
}

/// Whether a domain is one a store is given when none is configured,
/// `http://{name}.sandbox` ([`crate::storage::generate_default_domain`]).
fn is_sandbox(domain: &NamedNode) -> bool {
    let host = domain.as_str().split_once("://").map_or(domain.as_str(), |(_, rest)| rest);
    host.split(['/', ':', '?', '#']).next().is_some_and(|host| host.ends_with(".sandbox"))
}

/// Writes the first two claims of an instance into its empty log, both by the server:
///
/// 1. the registration of the server's own key, which is checked against the key it
///    registers ([`submit`]);
/// 2. the declaration of the domain as a `schema:Dataset` and, if it is a sandbox domain,
///    a `scatter:SandboxDomain`.
pub fn create_initial_claims(instance: &Instance, server_key: &SigningKey) -> io::Result<()> {
    let invalid = |e| io::Error::new(io::ErrorKind::InvalidData, e);
    let (domain, server) = (instance.domain.as_str(), &instance.server);
    let store = || Snapshot::load(instance);

    let registration = keys::registration(server, &server_key.verifying_key());
    let envelope = prepare(registration, server.clone(), domain, &store()?, server_key).map_err(invalid)?;
    submit(envelope, instance, server_key)?;

    let domain_iri = NamedNode::new(domain).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut declaration = vec![create_initial_triple(domain)];
    if is_sandbox(&domain_iri) {
        declaration.push(Triple::new(domain_iri, rdf::TYPE, scatter::SANDBOX_DOMAIN));
    }
    let envelope = prepare(declaration, server.clone(), domain, &store()?, server_key).map_err(invalid)?;
    submit(envelope, instance, server_key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scatter_claims::ClaimId;
    use crate::instance::{GRAPH_CLAIM, GRAPH_SERVER};

    #[test]
    fn an_instance_starts_with_the_key_and_the_domain() {
        let dir = tempfile::tempdir().unwrap();
        let server = NamedNode::new("http://example.org/Server").unwrap();
        let instance = Instance::new("http://example.org", server.clone(), dir.path().join("store.nt").to_str().unwrap());
        let server_key = keys::generate();

        create_initial_claims(&instance, &server_key).unwrap();

        let store = Snapshot::load(&instance).unwrap();
        assert_eq!(store.public_key(server.as_ref()), Some(server_key.verifying_key()));
        assert_eq!(store.triples_in(&instance.graph(GRAPH_SERVER)).len(), 5);
        assert!(store.triples_in(&instance.graph(GRAPH_SERVER)).contains(&create_initial_triple("http://example.org")));
        assert!(store.triples_in(&instance.graph(GRAPH_CLAIM)).is_empty());
        let domain = NamedNode::new("http://example.org").unwrap();
        // Only a sandbox domain is typed as one
        assert!(!store.triples_in(&instance.graph(GRAPH_SERVER)).iter().any(|t| t.object == oxrdf::Term::from(scatter::SANDBOX_DOMAIN)));
        assert!(is_sandbox(&NamedNode::new("http://abcdefghijklmnop.sandbox").unwrap()));
        assert!(!is_sandbox(&NamedNode::new("http://sandbox.example.org/x.sandbox").unwrap()));
        assert_ne!(store.latest_claim_using(domain.as_ref()), ClaimId::NONE);
        assert_eq!(store.latest_claim_using(domain.as_ref()), store.latest_claim_by(server.as_ref()));
    }
}
