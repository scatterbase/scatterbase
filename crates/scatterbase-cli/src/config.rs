//! The configuration an initialized instance keeps in its `.env` file.

use std::env;
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use oxrdf::NamedNode;
use scatterbase_kernel::keys;
use scatterbase_kernel::uri::parse_iri;
use scatterbase_kernel::Instance;

use crate::error::{Context, Result};

/// The storage file a new instance is given unless another is named.
pub const DEFAULT_STORAGE_FILE: &str = "data/scatterbase.nq";
/// The prefix file of an instance.
pub const DEFAULT_PREFIX_FILE: &str = "data/prefixes.ttl";
/// The file a new instance's server key is kept in unless another is named.
pub const DEFAULT_PRIVATE_KEY_FILE: &str = "keys/scatterbase.ed25519.private";
/// The file the server's public key is written to.
pub const DEFAULT_PUBLIC_KEY_FILE: &str = "keys/scatterbase.ed25519.public";

/// What the commands need to know about the instance.
pub struct Config {
    /// The domain: the base of skolem IRIs and the subject of the first claim.
    pub domain: String,
    /// The N-Quads file the store keeps its quads in.
    pub storage_file: String,
    /// The IRI the server claims as.
    pub server: NamedNode,
    /// The file holding the server's signing key.
    pub secret_key_file: String,
    /// The file the server's public key is written to.
    pub public_key_file: String,
    /// The file of the prefixes IRIs may be written with.
    pub prefix_file: String,
    /// The directory of the log, if it is not `log` beside the storage file.
    pub log_dir: Option<String>,
}

impl Config {
    /// Loads the configuration from the `.env` file at `env_path`.
    pub fn load(env_path: &str) -> Result<Self> {
        dotenv::from_path(env_path).map_err(|e| format!(
            "Error loading .env file at {env_path}: {e}\nMake sure you've initialized Scatterbase with 'scatterbase init' first.",
        ))?;
        let var = |name: &str| env::var(name)
            .map_err(|_| format!("{name} not set in environment. Run 'scatterbase init' first."));

        let domain = var("DOMAIN")?;
        let server = env::var("SERVER_URI").unwrap_or_else(|_| format!("{domain}/Server"));
        Ok(Self {
            server: parse_iri(&server).context("Invalid SERVER_URI")?,
            storage_file: var("STORAGE_FILE")?,
            secret_key_file: var("ED25519_SECRET_KEY_FILE")?,
            public_key_file: env::var("ED25519_PUBLIC_KEY_FILE").unwrap_or_else(|_| DEFAULT_PUBLIC_KEY_FILE.to_string()),
            prefix_file: env::var("PREFIX_FILE").unwrap_or_else(|_| DEFAULT_PREFIX_FILE.to_string()),
            log_dir: env::var("LOG_DIR").ok(),
            domain,
        })
    }

    /// The `.env` file of an instance with this configuration.
    pub fn to_env(&self) -> String {
        format!(
            "DOMAIN={}\nSTORAGE_FILE={}\nSERVER_URI={}\nED25519_PUBLIC_KEY_FILE={}\nED25519_SECRET_KEY_FILE={}\nPREFIX_FILE={}\n",
            self.domain, self.storage_file, self.server.as_str(), self.public_key_file, self.secret_key_file, self.prefix_file,
        )
    }

    /// The instance as the kernel knows it. Its log is the directory named by `LOG_DIR`, or
    /// `log` beside the storage file.
    pub fn instance(&self) -> Instance {
        let mut instance = Instance::new(self.domain.clone(), self.server.clone(), self.storage_file.clone());
        if let Some(log_dir) = &self.log_dir {
            instance.log_dir = PathBuf::from(log_dir);
        }
        instance
    }

    /// The server's signing key.
    pub fn server_key(&self) -> Result<SigningKey> {
        Ok(keys::read_signing_key(&self.secret_key_file)?)
    }

    /// The directory keys are kept in: the one holding the server's key file.
    pub fn key_dir(&self) -> PathBuf {
        Path::new(&self.secret_key_file).parent().map(Path::to_path_buf).unwrap_or_default()
    }

    /// The key an actor signs with: the key in `key_file` if one is named, the server's key
    /// for the server, and otherwise the actor's own key file, which `key add` writes.
    pub fn signing_key_for(&self, actor: &NamedNode, key_file: Option<&str>) -> Result<SigningKey> {
        if let Some(key_file) = key_file {
            return Ok(keys::read_signing_key(key_file)?);
        }
        if *actor == self.server {
            return self.server_key();
        }
        let path = keys::actor_key_file(self.key_dir(), actor.as_ref());
        if !path.exists() {
            return Err(format!(
                "No signing key for {actor}. Register one first using:\n  scatterbase key add --attribution {}\nor name its key file with --private-key.",
                actor.as_str(),
            ).into());
        }
        Ok(keys::read_signing_key(path)?)
    }
}
