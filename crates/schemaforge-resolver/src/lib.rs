//! URI resolution and `$ref` handling for Schemaforge.
//!
//! The default [`OfflineResolver`] resolves schemas from an in-memory registry
//! only. A [`FileResolver`] additionally loads schemas from the filesystem.
//! Both implement the [`Resolver`] trait used by the compiler.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;
use thiserror::Error;

/// Error returned when a URI cannot be resolved.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// The URI was not found in the resolver's registry.
    #[error("schema not found for URI: {0}")]
    NotFound(String),
    /// The referenced URI could not be parsed.
    #[error("invalid URI reference `{uri}`: {reason}")]
    InvalidUri {
        /// The URI that failed to parse.
        uri: String,
        /// Why the parse failed.
        reason: String,
    },
    /// The schema content could not be parsed as JSON.
    #[error("failed to parse schema at `{uri}`: {reason}")]
    ParseError {
        /// The URI of the schema.
        uri: String,
        /// JSON parse error message.
        reason: String,
    },
    /// IO error reading from the filesystem.
    #[error("IO error loading `{uri}`: {reason}")]
    IoError {
        /// The URI of the schema.
        uri: String,
        /// IO error message.
        reason: String,
    },
}

/// Resolves a `$ref` URI to a JSON [`Value`].
pub trait Resolver: Send + Sync {
    /// Resolve `reference` relative to `base` and return the schema value.
    ///
    /// The `base` is the `$id` or URI of the document currently being compiled.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] when the reference cannot be found or parsed.
    fn resolve(&self, base: &str, reference: &str) -> Result<Value, ResolveError>;
}

/// Resolves schemas from an in-memory registry (no network, no filesystem).
#[derive(Debug, Default)]
pub struct OfflineResolver {
    schemas: HashMap<String, Value>,
}

impl OfflineResolver {
    /// Create an empty offline resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pre-loaded schema value under `uri`.
    pub fn register(&mut self, uri: impl Into<String>, schema: Value) {
        self.schemas.insert(normalize_uri(uri.into()), schema);
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
        let resolved = resolve_uri(base, reference);
        let key = strip_fragment(&resolved);
        self.schemas
            .get(&normalize_uri(key.to_owned()))
            .cloned()
            .ok_or(ResolveError::NotFound(resolved))
    }
}

/// Resolves schemas from the filesystem, falling back to an offline registry.
#[derive(Debug, Default)]
pub struct FileResolver {
    offline: OfflineResolver,
    base_dir: Option<std::path::PathBuf>,
}

impl FileResolver {
    /// Create a file resolver rooted at `base_dir`.
    #[must_use]
    pub fn with_base_dir(base_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            offline: OfflineResolver::new(),
            base_dir: Some(base_dir.into()),
        }
    }

    /// Register a pre-loaded schema into the offline registry.
    pub fn register(&mut self, uri: impl Into<String>, schema: Value) {
        self.offline.register(uri, schema);
    }
}

impl Resolver for FileResolver {
    fn resolve(&self, base: &str, reference: &str) -> Result<Value, ResolveError> {
        if let Ok(v) = self.offline.resolve(base, reference) {
            return Ok(v);
        }
        let resolved = resolve_uri(base, reference);
        load_from_path(&resolved, self.base_dir.as_deref())
    }
}

/// Load a schema from the local filesystem based on a URI.
fn load_from_path(uri: &str, base_dir: Option<&Path>) -> Result<Value, ResolveError> {
    let path = uri_to_path(uri, base_dir).ok_or_else(|| ResolveError::NotFound(uri.to_owned()))?;
    let text = std::fs::read_to_string(&path).map_err(|e| ResolveError::IoError {
        uri: uri.to_owned(),
        reason: e.to_string(),
    })?;
    serde_json::from_str(&text).map_err(|e| ResolveError::ParseError {
        uri: uri.to_owned(),
        reason: e.to_string(),
    })
}

fn uri_to_path(uri: &str, base_dir: Option<&Path>) -> Option<std::path::PathBuf> {
    if let Some(file_path) = uri.strip_prefix("file://") {
        return Some(std::path::PathBuf::from(file_path));
    }
    let base = base_dir?;
    let relative = uri.trim_start_matches('/');
    Some(base.join(relative))
}

/// Normalize a URI for use as a registry key (strip trailing `#`).
fn normalize_uri(mut uri: String) -> String {
    if uri.ends_with('#') {
        uri.pop();
    }
    uri
}

/// Strip the fragment component (`#...`) from a URI.
fn strip_fragment(uri: &str) -> &str {
    uri.find('#').map_or(uri, |i| &uri[..i])
}

/// Resolve `reference` against `base` following RFC 3986 (simplified).
///
/// If `reference` is absolute (contains `://`), it is returned unchanged.
/// Fragment-only references are resolved against the base.
#[must_use]
pub fn resolve_uri(base: &str, reference: &str) -> String {
    if is_absolute_uri(reference) {
        return reference.to_owned();
    }
    if reference.starts_with('#') {
        let base_no_frag = strip_fragment(base);
        return format!("{base_no_frag}{reference}");
    }
    let base_dir = base_directory(base);
    if reference.starts_with('/') {
        let scheme_end = base.find("://").map_or(0, |i| i + 3);
        let authority_end = base[scheme_end..]
            .find('/')
            .map_or(base.len(), |i| scheme_end + i);
        format!("{}{reference}", &base[..authority_end])
    } else {
        format!("{base_dir}{reference}")
    }
}

fn is_absolute_uri(uri: &str) -> bool {
    uri.contains("://") || uri.starts_with("urn:")
}

fn base_directory(uri: &str) -> &str {
    uri.rfind('/').map_or("", |i| &uri[..=i])
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
    fn resolve_absolute_uri_passthrough() {
        let result = resolve_uri("https://example.com/a.json", "https://other.com/b.json");
        assert_eq!(result, "https://other.com/b.json");
    }

    #[test]
    fn resolve_fragment_uri() {
        let result = resolve_uri("https://example.com/schema.json", "#/defs/Foo");
        assert_eq!(result, "https://example.com/schema.json#/defs/Foo");
    }

    #[test]
    fn resolve_relative_uri() {
        let result = resolve_uri("https://example.com/schemas/a.json", "b.json");
        assert_eq!(result, "https://example.com/schemas/b.json");
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
}
