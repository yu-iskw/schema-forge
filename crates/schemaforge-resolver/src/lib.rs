//! URI resolution and `$ref` handling for Schemaforge.
//!
//! The default [`OfflineResolver`] resolves schemas from an in-memory registry
//! only. A [`FileResolver`] additionally loads schemas from the filesystem.
//! [`NetworkResolver`] always denies network access (policy: network=deny).
//! Both implement the [`Resolver`] trait used by the compiler.
//!
//! A [`LockFile`] (serialised to `schemaforge.lock.toml`) records every
//! resolved external URI so that builds remain reproducible.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

// ── Error type ────────────────────────────────────────────────────────────────

/// Error returned when a URI cannot be resolved.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// The URI was not found in the resolver's registry.
    #[error("schema not found for URI: {0}")]
    NotFound(String),
    /// Network access was denied by policy.
    #[error(
        "network access denied (policy: network=deny) for URI `{uri}`; \
         add the schema to an offline registry or unlock network access"
    )]
    NetworkDenied {
        /// The URI that triggered the denial.
        uri: String,
    },
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
    /// The schema document exceeds the configured size limit.
    #[error("schema at `{uri}` exceeds maximum size ({size} > {limit} bytes)")]
    SizeExceeded {
        /// The URI of the oversized schema.
        uri: String,
        /// Actual byte size.
        size: usize,
        /// Configured limit.
        limit: usize,
    },
    /// The schema document exceeds the configured nesting depth limit.
    #[error("schema at `{uri}` exceeds maximum nesting depth ({depth} > {limit})")]
    DepthExceeded {
        /// The URI of the deep schema.
        uri: String,
        /// Observed depth.
        depth: usize,
        /// Configured limit.
        limit: usize,
    },
}

// ── Resolver trait ────────────────────────────────────────────────────────────

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

// ── OfflineResolver ───────────────────────────────────────────────────────────

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

// ── FileResolver ──────────────────────────────────────────────────────────────

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

// ── NetworkResolver ───────────────────────────────────────────────────────────

/// A resolver that **always** denies network requests (policy: network=deny).
///
/// Use this as a safe default in environments where external schema fetching
/// should be explicitly prohibited.  All `resolve` calls return
/// [`ResolveError::NetworkDenied`].
#[derive(Debug, Default, Clone, Copy)]
pub struct NetworkResolver;

impl NetworkResolver {
    /// Create a new `NetworkResolver`.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Resolver for NetworkResolver {
    fn resolve(&self, _base: &str, reference: &str) -> Result<Value, ResolveError> {
        Err(ResolveError::NetworkDenied {
            uri: reference.to_owned(),
        })
    }
}

// ── LimitingResolver ─────────────────────────────────────────────────────────

/// Enforces byte-size and nesting-depth limits on resolved schemas.
///
/// Any schema that exceeds either limit is rejected with a descriptive error
/// rather than silently accepted.
#[derive(Debug)]
pub struct LimitingResolver<R> {
    inner: R,
    /// Maximum allowed byte size of a serialised schema (inclusive).
    max_bytes: usize,
    /// Maximum allowed JSON nesting depth (inclusive).
    max_depth: usize,
}

impl<R: Resolver> LimitingResolver<R> {
    /// Wrap `inner` with the given limits.
    #[must_use]
    pub const fn new(inner: R, max_bytes: usize, max_depth: usize) -> Self {
        Self {
            inner,
            max_bytes,
            max_depth,
        }
    }
}

impl<R: Resolver + Send + Sync> Resolver for LimitingResolver<R> {
    fn resolve(&self, base: &str, reference: &str) -> Result<Value, ResolveError> {
        let value = self.inner.resolve(base, reference)?;
        let serialised = serde_json::to_string(&value).unwrap_or_default();
        let size = serialised.len();
        if size > self.max_bytes {
            return Err(ResolveError::SizeExceeded {
                uri: reference.to_owned(),
                size,
                limit: self.max_bytes,
            });
        }
        let depth = json_depth(&value);
        if depth > self.max_depth {
            return Err(ResolveError::DepthExceeded {
                uri: reference.to_owned(),
                depth,
                limit: self.max_depth,
            });
        }
        Ok(value)
    }
}

/// Compute the maximum nesting depth of a JSON value.
fn json_depth(v: &Value) -> usize {
    match v {
        Value::Array(arr) => arr.iter().map(json_depth).max().unwrap_or(0) + 1,
        Value::Object(obj) => obj.values().map(json_depth).max().unwrap_or(0) + 1,
        _ => 1,
    }
}

// ── Lockfile ──────────────────────────────────────────────────────────────────

/// A single entry in the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockEntry {
    /// The resolved absolute URI.
    pub uri: String,
    /// Hex-encoded SHA-256 digest of the serialised schema bytes.
    pub digest: String,
    /// Byte length of the serialised schema.
    pub size: usize,
}

/// The contents of a `schemaforge.lock.toml` file.
///
/// The lockfile records every externally resolved schema URI so that builds
/// remain reproducible.  It is human-readable TOML.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockFile {
    /// Ordered list of locked schema entries.
    #[serde(default)]
    pub entries: Vec<LockEntry>,
}

impl LockFile {
    /// Create an empty lock file.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a lock entry.
    ///
    /// If an entry with the same URI already exists it is replaced.
    pub fn upsert(&mut self, entry: LockEntry) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.uri == entry.uri) {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Serialise the lock file to TOML.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] when serialisation fails.
    pub fn to_toml(&self) -> Result<String, std::io::Error> {
        toml::to_string_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Deserialise a lock file from TOML text.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] when the content is not valid TOML.
    pub fn from_toml(s: &str) -> Result<Self, std::io::Error> {
        toml::from_str(s).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Write the lock file to `path`.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] on IO or serialisation failure.
    pub fn write_to_path(&self, path: &Path) -> Result<(), std::io::Error> {
        let content = self.to_toml()?;
        std::fs::write(path, content)
    }

    /// Read a lock file from `path`.
    ///
    /// # Errors
    ///
    /// Returns an [`std::io::Error`] on IO or deserialisation failure.
    pub fn read_from_path(path: &Path) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

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

// ── Tests ─────────────────────────────────────────────────────────────────────

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

    // ── NetworkResolver tests ─────────────────────────────────────────────────

    #[test]
    fn network_resolver_denies_all() {
        let r = NetworkResolver::new();
        let err = r
            .resolve(
                "https://base.example.com/",
                "https://remote.example.com/schema.json",
            )
            .unwrap_err();
        assert!(matches!(err, ResolveError::NetworkDenied { .. }));
    }

    #[test]
    fn network_resolver_error_message_contains_uri() {
        let r = NetworkResolver::new();
        let uri = "https://example.com/forbidden.json";
        let err = r.resolve("", uri).unwrap_err();
        assert!(err.to_string().contains(uri));
    }

    #[test]
    fn network_resolver_error_message_mentions_policy() {
        let r = NetworkResolver::new();
        let err = r.resolve("", "https://example.com/x.json").unwrap_err();
        assert!(err.to_string().contains("network=deny"));
    }

    // ── LimitingResolver tests ────────────────────────────────────────────────

    #[test]
    fn limiting_resolver_passes_small_schema() {
        let mut offline = OfflineResolver::new();
        offline.register("https://example.com/s.json", json!({"type": "string"}));
        let limiting = LimitingResolver::new(offline, 10_000, 20);
        let result = limiting.resolve("", "https://example.com/s.json");
        assert!(result.is_ok());
    }

    #[test]
    fn limiting_resolver_rejects_oversized_schema() {
        let mut offline = OfflineResolver::new();
        let big_desc = "x".repeat(10_000);
        let big = json!({"description": big_desc});
        offline.register("https://example.com/big.json", big);
        let limiting = LimitingResolver::new(offline, 100, 20);
        let err = limiting
            .resolve("", "https://example.com/big.json")
            .unwrap_err();
        assert!(matches!(err, ResolveError::SizeExceeded { .. }));
    }

    #[test]
    fn limiting_resolver_rejects_too_deep_schema() {
        let mut offline = OfflineResolver::new();
        let deep = build_deep_schema(25);
        offline.register("https://example.com/deep.json", deep);
        let limiting = LimitingResolver::new(offline, 100_000, 10);
        let err = limiting
            .resolve("", "https://example.com/deep.json")
            .unwrap_err();
        assert!(matches!(err, ResolveError::DepthExceeded { .. }));
    }

    #[test]
    fn limiting_resolver_depth_error_has_uri() {
        let mut offline = OfflineResolver::new();
        offline.register("https://example.com/d.json", build_deep_schema(15));
        let limiting = LimitingResolver::new(offline, 100_000, 5);
        let err = limiting
            .resolve("", "https://example.com/d.json")
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("d.json"));
    }

    /// Build a JSON value with `depth` levels of nesting.
    fn build_deep_schema(depth: usize) -> Value {
        let mut v = json!("leaf");
        for _ in 0..depth {
            v = json!({"nested": v});
        }
        v
    }

    // ── LockFile tests ────────────────────────────────────────────────────────

    #[test]
    fn lockfile_roundtrip_toml() {
        let mut lf = LockFile::new();
        lf.upsert(LockEntry {
            uri: "https://example.com/schema.json".to_owned(),
            digest: "abc123".to_owned(),
            size: 42,
        });
        let toml = lf.to_toml().unwrap();
        let restored = LockFile::from_toml(&toml).unwrap();
        assert_eq!(lf, restored);
    }

    #[test]
    fn lockfile_upsert_replaces_existing() {
        let mut lf = LockFile::new();
        lf.upsert(LockEntry {
            uri: "https://example.com/s.json".to_owned(),
            digest: "old".to_owned(),
            size: 1,
        });
        lf.upsert(LockEntry {
            uri: "https://example.com/s.json".to_owned(),
            digest: "new".to_owned(),
            size: 2,
        });
        assert_eq!(lf.entries.len(), 1);
        assert_eq!(lf.entries[0].digest, "new");
    }

    #[test]
    fn lockfile_write_and_read_path() {
        let dir = std::env::temp_dir();
        let path = dir.join("schemaforge_test.lock.toml");
        let mut lf = LockFile::new();
        lf.upsert(LockEntry {
            uri: "https://example.com/schema.json".to_owned(),
            digest: "deadbeef".to_owned(),
            size: 100,
        });
        lf.write_to_path(&path).unwrap();
        let restored = LockFile::read_from_path(&path).unwrap();
        assert_eq!(lf, restored);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn lockfile_from_toml_invalid_returns_error() {
        let result = LockFile::from_toml("this is not toml!!! [[[");
        assert!(result.is_err());
    }

    // ── json_depth helper tests ───────────────────────────────────────────────

    #[test]
    fn json_depth_scalar_is_one() {
        assert_eq!(json_depth(&json!("hello")), 1);
        assert_eq!(json_depth(&json!(42)), 1);
        assert_eq!(json_depth(&json!(null)), 1);
    }

    #[test]
    fn json_depth_flat_object() {
        assert_eq!(json_depth(&json!({"a": 1, "b": 2})), 2);
    }

    #[test]
    fn json_depth_nested() {
        assert_eq!(json_depth(&json!({"a": {"b": {"c": 1}}})), 4);
    }

    #[test]
    fn json_depth_array() {
        assert_eq!(json_depth(&json!([1, 2, 3])), 2);
        assert_eq!(json_depth(&json!([[1, 2], [3]])), 3);
    }
}
