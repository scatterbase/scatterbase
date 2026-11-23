use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use ed25519_dalek::{SigningKey, VerifyingKey, Signer};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Sha256, Digest};
use data_encoding::Specification;
use dotenv::dotenv;
use std::env;

use crate::utils::{ensure_directory_exists};
use crate::constants::DEFAULT_PUBLIC_KEY_FILE;

// Define Base32z encoding specification
lazy_static::lazy_static! {
    static ref BASE32Z_SPEC: Specification = {
        let mut spec = Specification::new();
        spec.symbols = "ybndrfg8ejkmcpqxot1uwisza345h769".to_string();
        spec.padding = None;
        spec.ignore = "".to_string();
        spec
    };
    pub static ref BASE32Z: data_encoding::Encoding = BASE32Z_SPEC.encoding().unwrap();
}

/// Generates an ED25519 key pair
pub fn generate_ed25519_keypair() -> (SigningKey, VerifyingKey) {
    let mut csprng = OsRng;
    let mut secret_key_bytes = [0u8; 32];
    csprng.fill_bytes(&mut secret_key_bytes);
    let signing_key = SigningKey::from_bytes(&secret_key_bytes);
    let verifying_key = signing_key.verifying_key();

    (signing_key, verifying_key)
}

/// Gets the server's signing key from the environment
pub fn get_server_signing_key() -> SigningKey {
    // Load environment variables
    dotenv().ok();

    // Get the path to the secret key file
    let secret_key_file = env::var("ED25519_SECRET_KEY_FILE")
        .expect("ED25519_SECRET_KEY_FILE not set in environment");

    // Read the secret key from the file
    let mut file = File::open(&secret_key_file)
        .expect(&format!("Failed to open secret key file: {}", secret_key_file));
    let mut secret_key_b32z = String::new();
    file.read_to_string(&mut secret_key_b32z)
        .expect("Failed to read secret key file");

    // Decode the Base32z-encoded secret key
    let secret_key_bytes = BASE32Z.decode(secret_key_b32z.trim().as_bytes())
        .expect("Failed to decode secret key");

    // Create the signing key from the bytes
    SigningKey::from_bytes(&secret_key_bytes.try_into().unwrap())
}

/// Gets the server's verifying key from the environment
pub fn get_server_verifying_key() -> Option<VerifyingKey> {
    // Load environment variables
    dotenv().ok();

    // Get the path to the public key file
    let public_key_file = env::var("ED25519_PUBLIC_KEY_FILE")
        .unwrap_or_else(|_| DEFAULT_PUBLIC_KEY_FILE.to_string());

    // Check if the file exists
    if !Path::new(&public_key_file).exists() {
        println!("Public key file not found: {}", public_key_file);
        return None;
    }

    // Read the public key from the file
    let mut file = match File::open(&public_key_file) {
        Ok(file) => file,
        Err(e) => {
            println!("Failed to open public key file: {}: {}", public_key_file, e);
            return None;
        }
    };

    let mut public_key_b32z = String::new();
    if let Err(e) = file.read_to_string(&mut public_key_b32z) {
        println!("Failed to read public key file: {}", e);
        return None;
    }

    // Decode the Base32z-encoded public key
    let public_key_bytes = match BASE32Z.decode(public_key_b32z.trim().as_bytes()) {
        Ok(bytes) => bytes,
        Err(e) => {
            println!("Failed to decode public key: {}", e);
            return None;
        }
    };

    // Create the verifying key from the bytes
    match VerifyingKey::from_bytes(&public_key_bytes.try_into().unwrap()) {
        Ok(key) => Some(key),
        Err(e) => {
            println!("Failed to create verifying key: {}", e);
            None
        }
    }
}

/// Generates a new signing key and saves it to a file
pub fn generate_new_signing_key(key_file_path: &str) -> SigningKey {
    // Generate a new ED25519 key pair
    let (signing_key, verifying_key) = generate_ed25519_keypair();

    // Convert the signing key to Base32z for storage
    let secret_key_b32z = BASE32Z.encode(&signing_key.to_bytes());

    // Ensure the directory exists
    ensure_directory_exists(key_file_path).expect("Failed to create directory for key file");

    // Write the secret key to the file
    let mut file = File::create(key_file_path).expect("Failed to create key file");
    file.write_all(secret_key_b32z.as_bytes()).expect("Failed to write key to file");

    // Set file permissions to 600 (read/write for owner only) if on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(key_file_path, permissions).expect("Failed to set file permissions");
    }

    signing_key
}

/// Derives a key from the server key and an attribution string
pub fn derive_key_from_server(server_key: &SigningKey, attribution: &str) -> SigningKey {
    // Create a hash of the attribution string
    let mut hasher = Sha256::new();
    hasher.update(attribution.as_bytes());
    let hash = hasher.finalize();

    // Combine the server key and the hash to create a new key
    let mut combined = [0u8; 32];
    for (i, byte) in server_key.to_bytes().iter().enumerate() {
        combined[i] = byte ^ hash[i % 32];
    }

    SigningKey::from_bytes(&combined)
}

/// Signs data with a key and returns the signature as a Base32z-encoded string
pub fn sign_with_key(key: &SigningKey, data: &str) -> String {
    let signature = key.sign(data.as_bytes());
    BASE32Z.encode(&signature.to_bytes())
}

/// Hashes a string using SHA-256 and returns the hash as a Base32z-encoded string
pub fn hash_string(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    let result = hasher.finalize();
    BASE32Z.encode(result.as_ref())
}
