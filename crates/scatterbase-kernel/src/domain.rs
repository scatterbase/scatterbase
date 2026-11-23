//! The claims a new instance starts with.

use std::io;

use ed25519_dalek::SigningKey;
use oxrdf::{NamedNode, Triple};

use crate::claim::{prepare, submit};
use crate::keys;
use crate::lookup::Snapshot;
use crate::uri::{rdf, scatter, vocab};

/// The triple that declares the domain a dataset: `{domain} rdf:type schema:Dataset`.
pub fn create_initial_triple(domain: &str) -> Triple {
    let subject = NamedNode::new(domain).unwrap();
    let property = rdf::TYPE_URI.clone();
    let object = NamedNode::new(&format!("{}Dataset", vocab::SCHEMA)).unwrap();

    Triple::new(subject, property, object)
}

/// Writes the first two claims of an instance into its empty store, both by the server:
///
/// 1. the registration of the server's own key, which is checked against the key it
///    registers ([`submit`]);
/// 2. the declaration of the domain as a `schema:Dataset` and a `scatter:Domain`.
pub fn create_initial_claims(
    domain: &str,
    server: &NamedNode,
    storage_file: &str,
    server_key: &SigningKey,
) -> io::Result<()> {
    let invalid = |e| io::Error::new(io::ErrorKind::InvalidData, e);

    let registration = keys::registration(server, &server_key.verifying_key());
    let envelope = prepare(registration, server.clone(), domain, &Snapshot::load(storage_file)?, server_key)
        .map_err(invalid)?;
    submit(envelope, domain, storage_file, server_key)?;

    let domain_iri = NamedNode::new(domain).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let declaration = vec![
        create_initial_triple(domain),
        Triple::new(
            domain_iri,
            rdf::TYPE_URI.clone(),
            NamedNode::new_unchecked(format!("{}Domain", scatter::TERMS_BASE)),
        ),
    ];
    let envelope = prepare(declaration, server.clone(), domain, &Snapshot::load(storage_file)?, server_key)
        .map_err(invalid)?;
    submit(envelope, domain, storage_file, server_key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scatter_claims::ClaimId;

    #[test]
    fn an_instance_starts_with_the_key_and_the_domain() {
        let dir = tempfile::tempdir().unwrap();
        let storage_file = dir.path().join("store.nt");
        let storage_file = storage_file.to_str().unwrap();
        let server = NamedNode::new("http://example.org/Server").unwrap();
        let server_key = keys::generate();

        create_initial_claims("http://example.org", &server, storage_file, &server_key).unwrap();

        let store = Snapshot::load(storage_file).unwrap();
        assert_eq!(store.public_key(server.as_ref()), Some(server_key.verifying_key()));
        assert!(store.triples().contains(&create_initial_triple("http://example.org")));
        let domain = NamedNode::new("http://example.org").unwrap();
        assert_ne!(store.latest_claim_using(domain.as_ref()), ClaimId::NONE);
        assert_eq!(store.latest_claim_using(domain.as_ref()), store.latest_claim_by(server.as_ref()));
    }
}
