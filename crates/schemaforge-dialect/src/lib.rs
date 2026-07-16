//! JSON Schema dialect detection and vocabulary management.
//!
//! Supports JSON Schema drafts 4, 6, 7, 2019-09, and 2020-12. The primary
//! target is **Draft 2020-12**, which is tried first during detection.

pub mod schema_children;

use serde_json::Value;

/// Well-known JSON Schema dialect URIs.
pub mod uris {
    pub const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";
    pub const DRAFT_2019_09: &str = "https://json-schema.org/draft/2019-09/schema";
    pub const DRAFT_7: &str = "http://json-schema.org/draft-07/schema#";
    pub const DRAFT_6: &str = "http://json-schema.org/draft-06/schema#";
    pub const DRAFT_4: &str = "http://json-schema.org/draft-04/schema#";
}

/// A JSON Schema dialect (draft version).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dialect {
    /// JSON Schema Draft 2020-12 (primary target).
    #[default]
    Draft202012,
    /// JSON Schema Draft 2019-09.
    Draft201909,
    /// JSON Schema Draft 7.
    Draft7,
    /// JSON Schema Draft 6.
    Draft6,
    /// JSON Schema Draft 4.
    Draft4,
    /// Unknown or custom dialect URI.
    Unknown,
}

impl Dialect {
    /// Returns `true` if this dialect supports the `unevaluatedProperties`
    /// and `unevaluatedItems` keywords.
    #[must_use]
    pub const fn has_unevaluated(self) -> bool {
        matches!(self, Self::Draft202012 | Self::Draft201909)
    }

    /// Returns `true` if this dialect supports `$dynamicRef` / `$dynamicAnchor`.
    #[must_use]
    pub const fn has_dynamic_ref(self) -> bool {
        matches!(self, Self::Draft202012)
    }

    /// Returns `true` if `items` takes a schema (Draft 2020-12 changed it to
    /// only apply to items beyond `prefixItems`).
    #[must_use]
    pub const fn items_is_array_in_draft(self) -> bool {
        matches!(self, Self::Draft4 | Self::Draft6 | Self::Draft7)
    }

    /// The canonical `$schema` URI for this dialect.
    #[must_use]
    pub const fn uri(self) -> &'static str {
        match self {
            Self::Draft202012 => uris::DRAFT_2020_12,
            Self::Draft201909 => uris::DRAFT_2019_09,
            Self::Draft7 => uris::DRAFT_7,
            Self::Draft6 => uris::DRAFT_6,
            Self::Draft4 => uris::DRAFT_4,
            Self::Unknown => "",
        }
    }
}

impl std::fmt::Display for Dialect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.uri())
    }
}

/// Detect the dialect from the `$schema` keyword in a JSON value.
///
/// Returns [`Dialect::Draft202012`] when no `$schema` key is present (the
/// specification-recommended behaviour for context-free schemas).
#[must_use]
pub fn detect(schema: &Value) -> Dialect {
    let Some(Value::String(uri)) = schema.get("$schema") else {
        return Dialect::Draft202012;
    };
    classify_uri(uri)
}

fn classify_uri(uri: &str) -> Dialect {
    if uri.contains("draft/2020-12") {
        Dialect::Draft202012
    } else if uri.contains("draft/2019-09") {
        Dialect::Draft201909
    } else if uri.contains("draft-07") {
        Dialect::Draft7
    } else if uri.contains("draft-06") {
        Dialect::Draft6
    } else if uri.contains("draft-04") {
        Dialect::Draft4
    } else {
        Dialect::Unknown
    }
}

/// A JSON Schema vocabulary identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vocabulary {
    /// The URI identifying the vocabulary.
    pub uri: String,
    /// Whether the vocabulary is required.
    pub required: bool,
}

impl Vocabulary {
    /// Create a required vocabulary entry.
    #[must_use]
    pub fn required(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            required: true,
        }
    }

    /// Create an optional vocabulary entry.
    #[must_use]
    pub fn optional(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            required: false,
        }
    }
}

/// Return the standard vocabularies for Draft 2020-12.
#[must_use]
pub fn draft_2020_12_vocabularies() -> Vec<Vocabulary> {
    vec![
        Vocabulary::required("https://json-schema.org/draft/2020-12/vocab/core"),
        Vocabulary::required("https://json-schema.org/draft/2020-12/vocab/applicator"),
        Vocabulary::required("https://json-schema.org/draft/2020-12/vocab/unevaluated"),
        Vocabulary::required("https://json-schema.org/draft/2020-12/vocab/validation"),
        Vocabulary::optional("https://json-schema.org/draft/2020-12/vocab/meta-data"),
        Vocabulary::optional("https://json-schema.org/draft/2020-12/vocab/format-annotation"),
        Vocabulary::optional("https://json-schema.org/draft/2020-12/vocab/content"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detect_draft_2020_12() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "string"
        });
        assert_eq!(detect(&schema), Dialect::Draft202012);
    }

    #[test]
    fn detect_draft_7() {
        let schema = json!({"$schema": "http://json-schema.org/draft-07/schema#"});
        assert_eq!(detect(&schema), Dialect::Draft7);
    }

    #[test]
    fn detect_no_schema_defaults_to_2020_12() {
        let schema = json!({"type": "string"});
        assert_eq!(detect(&schema), Dialect::Draft202012);
    }

    #[test]
    fn detect_unknown_uri() {
        let schema = json!({"$schema": "https://custom.example.com/schema"});
        assert_eq!(detect(&schema), Dialect::Unknown);
    }

    #[test]
    fn dialect_features() {
        assert!(Dialect::Draft202012.has_unevaluated());
        assert!(Dialect::Draft202012.has_dynamic_ref());
        assert!(!Dialect::Draft7.has_dynamic_ref());
        assert!(Dialect::Draft4.items_is_array_in_draft());
        assert!(!Dialect::Draft202012.items_is_array_in_draft());
    }

    #[test]
    fn draft_2020_12_vocabs_non_empty() {
        let vocabs = draft_2020_12_vocabularies();
        assert!(!vocabs.is_empty());
        assert!(vocabs.iter().any(|v| v.required));
    }
}
