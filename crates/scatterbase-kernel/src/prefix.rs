//! The prefix file: the prefixes a person may write IRIs with.
//!
//! The file is Turtle prefix declarations, one to a line. An IRI in a file of triples that
//! is written with one of its prefixes, as `<schema:name>` is, is read as the IRI the
//! prefix stands for ([`expand_prefix`]).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use scatter_vocab::{foaf, owl, rdf, rdfs, xsd};

/// Reads the prefixes of a prefix file. A file that does not exist has none.
pub fn read_prefixes(prefix_file: impl AsRef<Path>) -> HashMap<String, String> {
    let mut prefixes = HashMap::new();
    let Ok(file) = File::open(prefix_file) else {
        return prefixes;
    };

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if line.starts_with("@prefix") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let prefix = parts[1].trim_end_matches(':');
                let uri = parts[2].trim_start_matches('<').trim_end_matches('>');
                prefixes.insert(prefix.to_string(), uri.to_string());
            }
        }
    }
    prefixes
}

/// Expands a prefixed URI using the prefix map
pub fn expand_prefix(uri_str: &str, prefixes: &HashMap<String, String>) -> String {
    // Check if the URI is already a full URI
    if uri_str.starts_with("http://") || uri_str.starts_with("https://") {
        return uri_str.to_string();
    }

    // Check if it's a prefixed URI
    if let Some(colon_pos) = uri_str.find(':') {
        let prefix = &uri_str[0..colon_pos];
        let local_part = &uri_str[colon_pos + 1..];

        if let Some(prefix_uri) = prefixes.get(prefix) {
            return format!("{}{}", prefix_uri, local_part);
        }
    }

    // Return as is if no prefix found
    uri_str.to_string()
}

fn create_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => fs::create_dir_all(parent),
        _ => Ok(()),
    }
}

/// Creates the prefix file with the standard prefixes, unless it exists. Returns whether it
/// was created.
pub fn init_prefix_file(prefix_file: impl AsRef<Path>) -> io::Result<bool> {
    let path = prefix_file.as_ref();
    if path.exists() {
        return Ok(false);
    }
    create_parent(path)?;
    let mut file = File::create(path)?;
    writeln!(file, "@prefix rdf: <{}> .", rdf::NS)?;
    writeln!(file, "@prefix rdfs: <{}> .", rdfs::NS)?;
    writeln!(file, "@prefix xsd: <{}> .", xsd::NS)?;
    writeln!(file, "@prefix owl: <{}> .", owl::NS)?;
    writeln!(file, "@prefix foaf: <{}> .", foaf::NS)?;
    writeln!(file, "@prefix dc: <http://purl.org/dc/elements/1.1/> .")?;
    Ok(true)
}

/// Adds a prefix to the prefix file, creating the file with the `rdf:` prefix if it is not
/// there.
pub fn add_prefix(prefix_file: impl AsRef<Path>, prefix: &str, uri: &str) -> io::Result<()> {
    let path = prefix_file.as_ref();
    create_parent(path)?;
    let is_new = !path.exists();
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    if is_new {
        writeln!(file, "@prefix rdf: <{}> .", rdf::NS)?;
    }
    writeln!(file, "@prefix {}: <{}> .", prefix, uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_are_written_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data").join("prefixes.ttl");
        assert!(read_prefixes(&file).is_empty());

        assert!(init_prefix_file(&file).unwrap());
        assert!(!init_prefix_file(&file).unwrap());
        add_prefix(&file, "schema", "http://schema.org/").unwrap();

        let prefixes = read_prefixes(&file);
        assert_eq!(prefixes.len(), 7);
        assert_eq!(expand_prefix("schema:name", &prefixes), "http://schema.org/name");
        assert_eq!(expand_prefix("unknown:name", &prefixes), "unknown:name");
        assert_eq!(expand_prefix("http://example.org/a", &prefixes), "http://example.org/a");
    }
}
