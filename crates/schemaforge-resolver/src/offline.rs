//! In-memory (offline) schema resolver.

use std::collections::HashMap;

use serde_json::Value;

use crate::{ResolveError, Resolver, uri};

/// Resolves schemas from an in-memory registry (no network, no filesystem).
#[derive(Debug, Default)]
pub struct OfflineResolver {
    pub(crate) schemas: HashMap<String, Value>,
}

impl OfflineResolver {
    /// Create an empty offline resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pre-loaded schema value under `uri`.
    pub fn register(&mut self, uri: impl Into<String>, schema: Value) {
        self.schemas.insert(uri::normalize_uri(uri.into()), schema);
    }

    /// Register a schema from its JSON source text.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError::ParseError`] when the JSON is invalid.
    pub fn register_json(
        &mut self,
        uri: impl Into<String>,
        json: &str,
    ) -> Result<(), ResolveError> {
        let uri = uri.into();
        let value = serde_json::from_str(json).map_err(|e| ResolveError::ParseError {
            uri: uri.clone(),
            reason: e.to_string(),
        })?;
        self.register(uri, value);
        Ok(())
    }
}

impl Resolver for OfflineResolver {
    fn resolve(&self, base: &str, reference: &str) -> Result<Value, ResolveError> {
        let resolved = uri::resolve_uri(base, reference);
        let (key, fragment) = uri::split_uri_fragment(&resolved);
        let doc = self
            .schemas
            .get(&uri::normalize_uri(key.to_owned()))
            .cloned()
            .ok_or_else(|| ResolveError::NotFound(resolved.clone()))?;
        crate::fragment::apply(doc, fragment, &resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn offline_resolver_found() {
        let mut r = OfflineResolver::new();
        r.register("https://example.com/schema.json", json!({"type": "string"}));
        let v = r.resolve(
            "https://example.com/other.json",
            "https://example.com/schema.json",
        );
        assert!(v.is_ok());
    }

    #[test]
    fn offline_resolver_not_found() {
        let r = OfflineResolver::new();
        let v = r.resolve("https://example.com/a.json", "https://example.com/b.json");
        assert!(matches!(v, Err(ResolveError::NotFound(_))));
    }

    #[test]
    fn register_json_parses_correctly() {
        let mut r = OfflineResolver::new();
        r.register_json("https://example.com/s.json", r#"{"type":"number"}"#)
            .unwrap();
        let v = r
            .resolve("https://example.com/x.json", "https://example.com/s.json")
            .unwrap();
        assert_eq!(v["type"], json!("number"));
    }

    #[test]
    fn offline_resolver_applies_pointer_fragment() {
        let mut r = OfflineResolver::new();
        r.register(
            "https://example.com/schema.json",
            json!({ "$defs": { "X": { "type": "string" } } }),
        );
        let v = r
            .resolve(
                "https://example.com/other.json",
                "https://example.com/schema.json#/$defs/X",
            )
            .unwrap();
        assert_eq!(v, json!({ "type": "string" }));
    }

    #[test]
    fn offline_resolver_applies_anchor_fragment() {
        let mut r = OfflineResolver::new();
        r.register(
            "urn:ext",
            json!({ "$defs": { "S": { "$anchor": "myStr", "type": "string" } } }),
        );
        let v = r.resolve("", "urn:ext#myStr").unwrap();
        assert_eq!(v["type"], json!("string"));
        assert_eq!(v["$anchor"], json!("myStr"));
    }
}
