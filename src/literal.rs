//! Literal module for Scatterbase
//! 
//! This module provides the Literal data structure for Scatterbase,
//! which represents a literal value that can be used as an object in a Triple.

use crate::uri::Uri;

/// Represents a Literal in Scatterbase
/// 
/// A Literal has a lexical form (string contents) and optionally either:
/// - A datatype (URI)
/// - A language tag (string)
/// 
/// A Literal cannot have both a datatype and a language tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// The lexical form (string contents) of the Literal
    lexical_form: String,
    /// The optional datatype of the Literal
    datatype: Option<Uri>,
    /// The optional language tag of the Literal
    language_tag: Option<String>,
}

impl std::fmt::Display for Literal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl Literal {
    /// Creates a new simple Literal with just a lexical form
    ///
    /// # Arguments
    ///
    /// * `lexical_form` - The string contents of the Literal
    ///
    /// # Returns
    ///
    /// * `Literal` - A new Literal instance
    pub fn new(lexical_form: &str) -> Self {
        Literal {
            lexical_form: lexical_form.to_string(),
            datatype: None,
            language_tag: None,
        }
    }

    /// Creates a new Literal with a lexical form and a datatype
    ///
    /// # Arguments
    ///
    /// * `lexical_form` - The string contents of the Literal
    /// * `datatype` - The datatype URI of the Literal
    ///
    /// # Returns
    ///
    /// * `Literal` - A new Literal instance
    pub fn with_datatype(lexical_form: &str, datatype: Uri) -> Self {
        Literal {
            lexical_form: lexical_form.to_string(),
            datatype: Some(datatype),
            language_tag: None,
        }
    }

    /// Creates a new Literal with a lexical form and a language tag
    ///
    /// # Arguments
    ///
    /// * `lexical_form` - The string contents of the Literal
    /// * `language_tag` - The language tag of the Literal
    ///
    /// # Returns
    ///
    /// * `Literal` - A new Literal instance
    pub fn with_language(lexical_form: &str, language_tag: &str) -> Self {
        Literal {
            lexical_form: lexical_form.to_string(),
            datatype: None,
            language_tag: Some(language_tag.to_string()),
        }
    }

    /// Returns the lexical form of the Literal
    pub fn lexical_form(&self) -> &str {
        &self.lexical_form
    }

    /// Returns the datatype of the Literal, if any
    pub fn datatype(&self) -> Option<&Uri> {
        self.datatype.as_ref()
    }

    /// Returns the language tag of the Literal, if any
    pub fn language_tag(&self) -> Option<&str> {
        self.language_tag.as_deref()
    }

    /// Returns the canonical string representation of the Literal
    pub fn to_string(&self) -> String {
        if let Some(datatype) = &self.datatype {
            format!("\"{}\"^^<{}>", self.lexical_form, datatype)
        } else if let Some(language_tag) = &self.language_tag {
            format!("\"{}\"@{}", self.lexical_form, language_tag)
        } else {
            format!("\"{}\"", self.lexical_form)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_literal() {
        let literal = Literal::new("Example");
        assert_eq!(literal.lexical_form(), "Example");
        assert_eq!(literal.datatype(), None);
        assert_eq!(literal.language_tag(), None);
        assert_eq!(literal.to_string(), "\"Example\"");
    }

    #[test]
    fn test_literal_with_datatype() {
        let datatype = Uri::new("https://example.com/Type").unwrap();
        let literal = Literal::with_datatype("Example", datatype.clone());
        assert_eq!(literal.lexical_form(), "Example");
        assert_eq!(literal.datatype(), Some(&datatype));
        assert_eq!(literal.language_tag(), None);
        assert_eq!(literal.to_string(), "\"Example\"^^<https://example.com/Type>");
    }

    #[test]
    fn test_literal_with_language() {
        let literal = Literal::with_language("Example", "en");
        assert_eq!(literal.lexical_form(), "Example");
        assert_eq!(literal.datatype(), None);
        assert_eq!(literal.language_tag(), Some("en"));
        assert_eq!(literal.to_string(), "\"Example\"@en");
    }
}