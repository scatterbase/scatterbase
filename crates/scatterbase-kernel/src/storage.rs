//! Storage module for Scatterbase
//! 
//! This module provides the storage abstraction and implementations for Scatterbase.

use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{self, Write, BufRead, BufReader};
use std::path::Path;

use oxrdf::Triple;
use crate::triple::TripleExt;

/// Trait defining the storage interface for Scatterbase
pub trait Storage {
    /// Initialize the storage with the given domain
    fn init(&mut self, domain: &str) -> io::Result<()>;

    /// Append a triple to the storage
    fn append(&mut self, triple: &Triple) -> io::Result<()>;

    /// Check if a claim with the given hash already exists in the storage
    fn claim_exists(&self, claim_hash: &str) -> io::Result<bool>;
}

/// NT file storage implementation
/// 
/// This storage implementation appends triples to a .NT file on the filesystem.
pub struct NtFileStorage {
    /// The path to the NT file
    file_path: String,
    /// The domain for the dataset
    domain: Option<String>,
}

impl NtFileStorage {
    /// Creates a new NtFileStorage instance
    pub fn new(file_path: &str) -> Self {
        NtFileStorage {
            file_path: file_path.to_string(),
            domain: None,
        }
    }
}

impl Storage for NtFileStorage {
    fn init(&mut self, domain: &str) -> io::Result<()> {
        self.domain = Some(domain.to_string());

        // Create the directory if it doesn't exist
        if let Some(parent) = Path::new(&self.file_path).parent() {
            if !parent.exists() {
                create_dir_all(parent)?;
                println!("Created directory: {}", parent.display());
            }
        }

        // Create the file if it doesn't exist
        if !Path::new(&self.file_path).exists() {
            File::create(&self.file_path)?;
        }

        Ok(())
    }

    fn append(&mut self, triple: &Triple) -> io::Result<()> {
        // Ensure the storage has been initialized
        if self.domain.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Storage has not been initialized with a domain",
            ));
        }

        // Format the triple in NT format
        let nt_line = triple.to_ntriples();

        // Check if the triple already exists in the file
        let file = File::open(&self.file_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line == nt_line {
                // Triple already exists, silently drop it
                return Ok(());
            }
        }

        // Open the file in append mode
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.file_path)?;

        // Write the triple to the file with a newline
        file.write_all(format!("{}\n", nt_line).as_bytes())?;

        Ok(())
    }

    fn claim_exists(&self, claim_hash: &str) -> io::Result<bool> {
        // Ensure the storage has been initialized
        if self.domain.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Storage has not been initialized with a domain",
            ));
        }

        // Check if the file exists
        if !Path::new(&self.file_path).exists() {
            return Ok(false);
        }

        // Create the claim hash URI pattern to search for
        let claim_uri_pattern = format!("<https://scatter.red/claim/{}>", claim_hash);

        // Open the file and search for the claim hash URI
        let file = File::open(&self.file_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            // Check if the line starts with the claim hash URI (as a subject)
            if line.starts_with(&claim_uri_pattern) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// Mock storage implementation for testing
/// 
/// This storage implementation stores triples in memory instead of writing to a file.
/// It's useful for testing without relying on the filesystem.
pub struct MockStorage {
    /// The domain for the dataset
    domain: Option<String>,
    /// The triples stored in memory
    triples: Vec<String>,
}

impl MockStorage {
    /// Creates a new MockStorage instance
    pub fn new() -> Self {
        MockStorage {
            domain: None,
            triples: Vec::new(),
        }
    }

    /// Clears all triples from the storage
    /// 
    /// This is useful for ensuring tests start with a clean slate.
    pub fn clear(&mut self) {
        self.triples.clear();
    }

    /// Returns all triples in the storage
    pub fn get_triples(&self) -> &Vec<String> {
        &self.triples
    }
}

impl Storage for MockStorage {
    fn init(&mut self, domain: &str) -> io::Result<()> {
        self.domain = Some(domain.to_string());
        Ok(())
    }

    fn append(&mut self, triple: &Triple) -> io::Result<()> {
        // Ensure the storage has been initialized
        if self.domain.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Storage has not been initialized with a domain",
            ));
        }

        // Format the triple in NT format
        let nt_line = triple.to_ntriples();
        println!("MockStorage.append: Adding triple: {}", nt_line);

        // Check if the triple already exists in the storage
        if self.triples.contains(&nt_line) {
            // Triple already exists, silently drop it
            println!("MockStorage.append: Triple already exists, not adding duplicate");
            return Ok(());
        }

        // Store the triple
        self.triples.push(nt_line);

        Ok(())
    }

    fn claim_exists(&self, claim_hash: &str) -> io::Result<bool> {
        // Ensure the storage has been initialized
        if self.domain.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Storage has not been initialized with a domain",
            ));
        }

        // Create the claim hash URI pattern to search for
        let claim_uri_pattern = format!("<https://scatter.red/claim/{}>", claim_hash);

        println!("MockStorage.claim_exists: Checking for claim hash: {}", claim_hash);
        println!("MockStorage.claim_exists: Looking for pattern: {}", claim_uri_pattern);
        println!("MockStorage.claim_exists: Current triples: {:?}", self.triples);

        // Print a stack trace to see where this method is being called from
        println!("MockStorage.claim_exists: Stack trace:");
        let backtrace = std::backtrace::Backtrace::capture();
        println!("{}", backtrace);

        // Search for the claim hash URI in the stored triples
        for triple in &self.triples {
            // Check if the triple starts with the claim hash URI (as a subject)
            if triple.starts_with(&claim_uri_pattern) {
                println!("MockStorage.claim_exists: Found matching triple: {}", triple);
                return Ok(true);
            }
        }

        println!("MockStorage.claim_exists: No matching triple found");
        Ok(false)
    }
}

/// Creates a storage instance based on the environment
/// 
/// If the MOCK_STORAGE environment variable is set to "true", returns a MockStorage instance.
/// Otherwise, returns an NtFileStorage instance with the given file path.
pub fn create_storage(file_path: &str) -> Box<dyn Storage> {
    if let Ok(mock_storage) = std::env::var("MOCK_STORAGE") {
        if mock_storage == "true" {
            // Create a new MockStorage instance
            let mut storage = MockStorage::new();

            // Check if MOCK_STORAGE_CLEAR is set to "true" to determine if we should clear the mock storage
            if let Ok(clear_storage) = std::env::var("MOCK_STORAGE_CLEAR") {
                if clear_storage == "true" {
                    // Clear the storage to ensure a clean slate
                    println!("Clearing MockStorage instance for a clean slate");
                    storage.clear();
                }
            }

            return Box::new(storage);
        }
    }
    Box::new(NtFileStorage::new(file_path))
}

/// Generates a default domain for Scatterbase
/// 
/// The default domain is 16 randomly generated letters followed by ".sandbox"
pub fn generate_default_domain() -> String {
    use rand::{Rng, thread_rng};
    use rand::distributions::Alphanumeric;

    let random_string: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .filter(|c| c.is_alphabetic())
        .take(16)
        .collect();

    format!("http://{}.sandbox", random_string.to_lowercase())
}
