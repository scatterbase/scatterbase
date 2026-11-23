//! The configuration an initialized instance keeps in its `.env` file.

use std::env;
use std::io;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use oxrdf::NamedNode;
use scatterbase_kernel::keys;
use scatterbase_kernel::uri::parse_iri;

/// What the commands need to know about the instance.
pub struct Config {
    /// The domain: the base of skolem IRIs and the subject of the first claim.
    pub domain: String,
    /// The N-Triples file the store keeps its triples in.
    pub storage_file: String,
    /// The IRI the server claims as.
    pub server: NamedNode,
    /// The file holding the server's signing key.
    pub secret_key_file: String,
}

impl Config {
    /// Loads the configuration from the `.env` file at `env_path`.
    pub fn load(env_path: &str) -> Result<Self, String> {
        dotenv::from_path(env_path).map_err(|e| format!(
            "Error loading .env file at {}: {}\nMake sure you've initialized Scatterbase with 'scatterbase init' first.",
            env_path, e,
        ))?;
        let var = |name: &str| env::var(name)
            .map_err(|_| format!("{} not set in environment. Run 'scatterbase init' first.", name));

        let domain = var("DOMAIN")?;
        let server = match env::var("SERVER_URI") {
            Ok(server) => server,
            Err(_) => format!("{}/Server", domain),
        };
        Ok(Self {
            server: parse_iri(&server).map_err(|e| format!("Invalid SERVER_URI: {}", e))?,
            storage_file: var("STORAGE_FILE")?,
            secret_key_file: var("ED25519_SECRET_KEY_FILE")?,
            domain,
        })
    }

    /// The server's signing key.
    pub fn server_key(&self) -> io::Result<SigningKey> {
        keys::read_signing_key(&self.secret_key_file)
    }

    /// The directory keys are kept in: the one holding the server's key file.
    pub fn key_dir(&self) -> PathBuf {
        Path::new(&self.secret_key_file).parent().map(Path::to_path_buf).unwrap_or_default()
    }

    /// The key an actor signs with: the key in `key_file` if one is named, the server's key
    /// for the server, and otherwise the actor's own key file, which `key add` writes.
    pub fn signing_key_for(&self, actor: &NamedNode, key_file: Option<&str>) -> io::Result<SigningKey> {
        if let Some(key_file) = key_file {
            return keys::read_signing_key(key_file);
        }
        if *actor == self.server {
            return self.server_key();
        }
        let path = keys::actor_key_file(self.key_dir(), actor.as_ref());
        if !path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "No signing key for {actor}. Register one first using:\n  scatterbase key add --attribution {}\nor name its key file with --private-key.",
                    actor.as_str(),
                ),
            ));
        }
        keys::read_signing_key(path)
    }
}
