//! Parsing an IRI that a person wrote. The vocabulary is `scatter-vocab`'s.

use oxrdf::NamedNode;

/// Maximum length for a URI in bytes
/// RFC 7230 suggests a practical limit of 8000 bytes
pub const MAX_URI_LENGTH: usize = 8000;

/// Parses an IRI given on the command line or read from a file.
///
/// Angle brackets around the IRI are delimiters and are dropped. The IRI must be absolute
/// and no longer than [`MAX_URI_LENGTH`] bytes.
pub fn parse_iri(iri: &str) -> Result<NamedNode, String> {
    let iri = iri.strip_prefix('<').and_then(|rest| rest.strip_suffix('>')).unwrap_or(iri);
    if iri.len() > MAX_URI_LENGTH {
        return Err(format!("URI exceeds maximum length of {} bytes", MAX_URI_LENGTH));
    }
    NamedNode::new(iri).map_err(|e| format!("Invalid URI '{}': {}", iri, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_iri_strips_angle_brackets() {
        assert_eq!(parse_iri("<https://example.com/a>").unwrap().as_str(), "https://example.com/a");
        assert_eq!(parse_iri("https://example.com/a").unwrap().as_str(), "https://example.com/a");
    }

    #[test]
    fn test_parse_iri_rejects_relative_and_malformed() {
        assert!(parse_iri("").is_err());
        assert!(parse_iri("relative/path").is_err());
        assert!(parse_iri("https://example.com/<a>").is_err());
    }

    #[test]
    fn test_parse_iri_max_length() {
        let long = format!("https://example.com/{}", "a".repeat(MAX_URI_LENGTH));
        assert!(parse_iri(&long).is_err());
    }
}
