//! Signing keys: one for the server and one for each actor that claims.
//!
//! A key is an Ed25519 key. A private key is kept in a file as 32 bytes in z-base-32,
//! readable by its owner only. A public key is named by the IRI `urn:ed25519:{key}`, the
//! 32 bytes in z-base-32, and an actor's key is registered by a claim holding the triples
//! of [`registration`].
//!
//! The server's key file is named by the configuration. An actor's key file is
//! [`actor_key_file`], beside it. Nothing here derives one key from another.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ed25519_dalek::{SigningKey, VerifyingKey};
use oxrdf::{NamedNode, NamedNodeRef, Triple};
use rand::rngs::OsRng;
use rand::RngCore;
use scatter_claims::id::{zbase32, zbase32_decode};
use sha2::{Digest, Sha256};

use crate::uri::{rdf, scatter};

/// The IRI prefix of a public key.
pub const PUBLIC_KEY_NS: &str = "urn:ed25519:";

/// A new random signing key.
pub fn generate() -> SigningKey {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    SigningKey::from_bytes(&secret)
}

/// Reads a signing key from its file.
pub fn read_signing_key(path: impl AsRef<Path>) -> io::Result<SigningKey> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .map_err(|e| io::Error::new(e.kind(), format!("Error reading key file {}: {}", path.display(), e)))?;
    zbase32_decode(text.trim())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .map(|bytes| SigningKey::from_bytes(&bytes))
        .ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} does not hold a 32-byte key in z-base-32", path.display()),
        ))
}

/// Writes a signing key to a file only its owner can read, creating the directory.
pub fn write_signing_key(path: impl AsRef<Path>, key: &SigningKey) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(zbase32(&key.to_bytes()).as_bytes())
}

/// The file an actor's signing key is kept in: `actors/{hash}.ed25519.private` under the
/// key directory, where `{hash}` is the SHA-256 of the actor's IRI in z-base-32.
pub fn actor_key_file(key_dir: impl AsRef<Path>, actor: NamedNodeRef<'_>) -> PathBuf {
    let hash = zbase32(&Sha256::digest(actor.as_str().as_bytes()));
    key_dir.as_ref().join("actors").join(format!("{hash}.ed25519.private"))
}

/// The IRI of a public key, `urn:ed25519:{key}`.
pub fn public_key_iri(key: &VerifyingKey) -> NamedNode {
    NamedNode::new_unchecked(format!("{PUBLIC_KEY_NS}{}", zbase32(&key.to_bytes())))
}

/// The public key an IRI names, if it is a public key IRI.
pub fn public_key_from_iri(iri: NamedNodeRef<'_>) -> Option<VerifyingKey> {
    let bytes = zbase32_decode(iri.as_str().strip_prefix(PUBLIC_KEY_NS)?)?;
    VerifyingKey::from_bytes(&<[u8; 32]>::try_from(bytes).ok()?).ok()
}

/// The triples that register a public key for an actor:
///
/// ```text
/// {actor} scatter:hasPublicKey <urn:ed25519:{key}> .
/// <urn:ed25519:{key}> rdf:type scatter:PublicKey ; scatter:keyType scatter:ed25519 ; scatter:owner {actor} .
/// ```
pub fn registration(actor: &NamedNode, key: &VerifyingKey) -> Vec<Triple> {
    let key_iri = public_key_iri(key);
    vec![
        Triple::new(actor.clone(), scatter::HAS_PUBLIC_KEY_URI.clone(), key_iri.clone()),
        Triple::new(key_iri.clone(), rdf::TYPE_URI.clone(), scatter::PUBLIC_KEY_URI.clone()),
        Triple::new(key_iri.clone(), scatter::KEY_TYPE_URI.clone(), scatter::ED25519_URI.clone()),
        Triple::new(key_iri, scatter::OWNER_URI.clone(), actor.clone()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_files_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let actor = NamedNode::new("http://example.org/alice").unwrap();
        let path = actor_key_file(dir.path(), actor.as_ref());
        assert!(path.starts_with(dir.path().join("actors")));
        assert_ne!(path, actor_key_file(dir.path(), NamedNodeRef::new_unchecked("http://example.org/bob")));

        let key = generate();
        write_signing_key(&path, &key).unwrap();
        assert_eq!(read_signing_key(&path).unwrap().to_bytes(), key.to_bytes());

        fs::write(&path, "not a key").unwrap();
        assert!(read_signing_key(&path).is_err());
    }

    #[test]
    fn public_key_iris_round_trip() {
        let key = generate().verifying_key();
        let iri = public_key_iri(&key);
        assert!(iri.as_str().starts_with("urn:ed25519:"));
        assert_eq!(public_key_from_iri(iri.as_ref()), Some(key));
        assert_eq!(public_key_from_iri(NamedNodeRef::new_unchecked("http://example.org/key")), None);
    }
}
