//! Filesystem-backed schema resolver with path-jail enforcement.
//!
//! [`FileResolver`] is ready for future compiler wiring and resolves schemas
//! from the local filesystem, falling back to an in-memory offline registry.
//! All filesystem paths are confined to a configurable base-directory jail.

use std::io::Read;
use std::path::Path;

use serde_json::Value;

use crate::{ResolveError, Resolver, offline::OfflineResolver, uri};

/// Resolves schemas from the filesystem, falling back to an offline registry.
#[derive(Debug)]
pub struct FileResolver {
    offline: OfflineResolver,
    base_dir: Option<std::path::PathBuf>,
    /// Maximum allowed byte size of a schema file (checked via metadata before read).
    max_bytes: u64,
}

impl FileResolver {
    /// Default file size limit: 50 MiB.
    pub const DEFAULT_MAX_BYTES: u64 = 52_428_800;

    /// Create a file resolver rooted at `base_dir`.
    #[must_use]
    pub fn with_base_dir(base_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            offline: OfflineResolver::new(),
            base_dir: Some(base_dir.into()),
            max_bytes: Self::DEFAULT_MAX_BYTES,
        }
    }

    /// Create a file resolver rooted at `base_dir` with a custom size limit.
    #[must_use]
    pub fn with_base_dir_and_limit(
        base_dir: impl Into<std::path::PathBuf>,
        max_bytes: u64,
    ) -> Self {
        Self {
            offline: OfflineResolver::new(),
            base_dir: Some(base_dir.into()),
            max_bytes,
        }
    }

    /// Register a pre-loaded schema into the offline registry.
    pub fn register(&mut self, uri: impl Into<String>, schema: Value) {
        self.offline.register(uri, schema);
    }
}

impl Default for FileResolver {
    fn default() -> Self {
        Self {
            offline: OfflineResolver::new(),
            base_dir: None,
            max_bytes: Self::DEFAULT_MAX_BYTES,
        }
    }
}

impl Resolver for FileResolver {
    fn resolve(&self, base: &str, reference: &str) -> Result<Value, ResolveError> {
        // Offline registry applies fragments itself; prefer it when present.
        if let Ok(v) = self.offline.resolve(base, reference) {
            return Ok(v);
        }
        let resolved = uri::resolve_uri(base, reference);
        // Strip any fragment before opening the file so the OS sees a plain
        // path, then apply the fragment to the loaded document.
        let (path_uri, fragment) = uri::split_uri_fragment(&resolved);
        let doc = load_from_path(path_uri, self.base_dir.as_deref(), self.max_bytes)?;
        crate::fragment::apply(doc, fragment, &resolved)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn load_from_path(
    uri: &str,
    base_dir: Option<&Path>,
    max_bytes: u64,
) -> Result<Value, ResolveError> {
    let path = uri_to_jailed_path(uri, base_dir)?;

    // Open the file first to obtain a stable file descriptor before reading.
    // This narrows the TOCTOU window: a symlink created between the initial
    // jail check and open will be resolved by the OS at open time, and then
    // caught by the re-canonicalize check below before any bytes are read.
    let file = std::fs::File::open(&path).map_err(|e| ResolveError::IoError {
        uri: uri.to_owned(),
        reason: e.to_string(),
    })?;

    // Re-canonicalize via the open file descriptor so that any symlink swap
    // that occurred between uri_to_jailed_path and File::open is caught.
    // On Linux/Unix we use /proc/self/fd/{fd} which resolves the actual target
    // of the descriptor rather than re-walking the (now possibly stale) path.
    // Fail-closed: any canonicalization failure is treated as a jail escape.
    if let Some(base) = base_dir {
        let canonical = post_open_canonical(&file, &path)?;
        let canonical_base =
            std::fs::canonicalize(base).unwrap_or_else(|_| lexically_normalize(base));
        if !canonical.starts_with(&canonical_base) {
            return Err(ResolveError::PathEscaped {
                path: canonical.display().to_string(),
            });
        }
    }

    // Stat the open file descriptor. Fail-closed: any metadata failure is treated
    // as an IO error rather than silently continuing with unknown file properties.
    let metadata = file.metadata().map_err(|e| ResolveError::IoError {
        uri: uri.to_owned(),
        reason: format!("cannot stat file: {e}"),
    })?;

    // Reject non-regular files (FIFOs, sockets, devices, directories).
    // A FIFO would block forever on read; devices may produce unbounded data.
    if !metadata.file_type().is_file() {
        return Err(ResolveError::IoError {
            uri: uri.to_owned(),
            reason: "not a regular file (FIFO, device, socket, or directory)".to_owned(),
        });
    }

    // Guard against large files using the size from stat.
    let file_len = metadata.len();
    if file_len > max_bytes {
        return Err(ResolveError::SizeExceeded {
            uri: uri.to_owned(),
            size: file_len as usize,
            limit: max_bytes as usize,
        });
    }

    // Use take(max_bytes + 1) so that if the file grows between stat and read
    // we still cap the read at a safe bound and detect the excess.
    let mut text = String::new();
    file.take(max_bytes + 1)
        .read_to_string(&mut text)
        .map_err(|e| ResolveError::IoError {
            uri: uri.to_owned(),
            reason: e.to_string(),
        })?;

    // Belt-and-suspenders: if more than max_bytes were read (file grew after stat),
    // reject with SizeExceeded rather than silently accepting oversized content.
    if text.len() as u64 > max_bytes {
        return Err(ResolveError::SizeExceeded {
            uri: uri.to_owned(),
            size: text.len(),
            limit: max_bytes as usize,
        });
    }

    serde_json::from_str(&text).map_err(|e| ResolveError::ParseError {
        uri: uri.to_owned(),
        reason: e.to_string(),
    })
}

/// Canonicalize the real path of an already-open file.
///
/// On Linux (and Unix generally) reads `/proc/self/fd/{fd}` so that the OS
/// resolves the descriptor's actual target rather than re-walking a path that
/// could have been swapped since `File::open`.  On non-Unix platforms falls
/// back to `std::fs::canonicalize` on the pre-open path.  Either way,
/// failure is treated as a jail escape (fail-closed).
fn post_open_canonical(
    file: &std::fs::File,
    fallback_path: &Path,
) -> Result<std::path::PathBuf, ResolveError> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let proc_link = std::path::PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        std::fs::canonicalize(&proc_link).map_err(|_| ResolveError::PathEscaped {
            path: fallback_path.display().to_string(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        std::fs::canonicalize(fallback_path).map_err(|_| ResolveError::PathEscaped {
            path: fallback_path.display().to_string(),
        })
    }
}

/// Resolve a URI to a filesystem path, enforcing a base-directory jail.
///
/// Rules:
/// - `file://` URIs with an absolute path require `base_dir`; without one
///   they are rejected with [`ResolveError::NotFound`] to prevent unconfined
///   filesystem access.
/// - Relative URIs are joined with `base_dir`; if `base_dir` is `None` the
///   URI cannot be resolved and [`ResolveError::NotFound`] is returned.
/// - The resolved path is canonicalized via [`std::fs::canonicalize`] when the
///   file exists (which resolves symlinks and removes `..` components).  When
///   the file does not yet exist the path is lexically normalised instead.
/// - After normalisation the result must have the canonical (or lexical)
///   `base_dir` as a prefix.  Any path that escapes the jail — including via
///   symlinks — is rejected with [`ResolveError::PathEscaped`].
fn uri_to_jailed_path(
    uri: &str,
    base_dir: Option<&Path>,
) -> Result<std::path::PathBuf, ResolveError> {
    let raw_path: std::path::PathBuf = if let Some(file_path) = uri.strip_prefix("file://") {
        // Absolute file:// URIs require a jail; refuse unconfined access.
        if base_dir.is_none() {
            return Err(ResolveError::NotFound(uri.to_owned()));
        }
        std::path::PathBuf::from(file_path)
    } else {
        let base = base_dir.ok_or_else(|| ResolveError::NotFound(uri.to_owned()))?;
        let relative = uri.trim_start_matches('/');
        base.join(relative)
    };

    // Prefer canonical resolution (resolves symlinks) when the path exists;
    // fall back to lexical normalisation for paths that do not yet exist.
    let normalized = if raw_path.exists() {
        std::fs::canonicalize(&raw_path).unwrap_or_else(|_| lexically_normalize(&raw_path))
    } else {
        lexically_normalize(&raw_path)
    };

    // Enforce jail when a base directory is configured.
    if let Some(base) = base_dir {
        let normalized_base = if base.exists() {
            std::fs::canonicalize(base).unwrap_or_else(|_| lexically_normalize(base))
        } else {
            lexically_normalize(base)
        };
        if !normalized.starts_with(&normalized_base) {
            return Err(ResolveError::PathEscaped {
                path: normalized.display().to_string(),
            });
        }
    }

    Ok(normalized)
}

/// Lexically normalize a path by collapsing `.` and `..` components without
/// touching the filesystem (no symlink resolution).
pub(crate) fn lexically_normalize(path: &Path) -> std::path::PathBuf {
    let mut parts: Vec<std::path::Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // Only pop a normal component; a leading `..` above the root
                // or a prefix cannot be popped, so leave it in place (the
                // subsequent starts_with check will catch the escape).
                match parts.last() {
                    Some(std::path::Component::Normal(_)) => {
                        parts.pop();
                    }
                    _ => parts.push(component),
                }
            }
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_jail() -> PathBuf {
        let dir = std::env::temp_dir().join("schemaforge_jail_test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("schema.json"), r#"{"type":"string"}"#).unwrap();
        dir
    }

    #[test]
    fn file_resolver_allows_file_uri_inside_jail() {
        let jail = make_jail();
        let r = FileResolver::with_base_dir(&jail);
        let schema_uri = format!("file://{}/schema.json", jail.display());
        let result = r.resolve("", &schema_uri);
        assert!(
            !matches!(result, Err(ResolveError::PathEscaped { .. })),
            "expected Ok or non-jail error, got {result:?}"
        );
    }

    #[test]
    fn file_resolver_rejects_dotdot_in_file_uri() {
        let jail = make_jail();
        let r = FileResolver::with_base_dir(&jail);
        let escaped = format!("file://{}/../../../etc/passwd", jail.display());
        let err = r.resolve("", &escaped).unwrap_err();
        assert!(
            matches!(err, ResolveError::PathEscaped { .. }),
            "expected PathEscaped, got {err:?}"
        );
    }

    #[test]
    fn file_resolver_rejects_absolute_path_outside_jail() {
        let jail = make_jail();
        let r = FileResolver::with_base_dir(&jail);
        let err = r.resolve("", "file:///etc/passwd").unwrap_err();
        assert!(
            matches!(err, ResolveError::PathEscaped { .. }),
            "expected PathEscaped, got {err:?}"
        );
    }

    #[test]
    fn file_resolver_rejects_relative_dotdot_escape() {
        let jail = make_jail();
        let r = FileResolver::with_base_dir(&jail);
        let err = r.resolve("", "../../etc/passwd").unwrap_err();
        assert!(
            matches!(err, ResolveError::PathEscaped { .. }),
            "expected PathEscaped, got {err:?}"
        );
    }

    #[test]
    fn file_resolver_rejects_dotdot_in_resolved_ref() {
        let jail = make_jail();
        let r = FileResolver::with_base_dir(&jail);
        let base = format!("file://{}/schema.json", jail.display());
        let err = r.resolve(&base, "../../../etc/passwd").unwrap_err();
        assert!(
            matches!(err, ResolveError::PathEscaped { .. }),
            "expected PathEscaped, got {err:?}"
        );
    }

    #[test]
    fn file_resolver_no_base_dir_rejects_absolute_file_uri() {
        let r = FileResolver::default();
        let err = r.resolve("", "file:///etc/passwd").unwrap_err();
        assert!(
            matches!(err, ResolveError::NotFound(_)),
            "expected NotFound without base_dir, got {err:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_resolver_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let tmp = std::env::temp_dir().join("schemaforge_symlink_test");
        let jail = tmp.join("jail");
        let outside = tmp.join("outside_schema.json");
        std::fs::create_dir_all(&jail).unwrap();
        std::fs::write(&outside, r#"{"type":"string"}"#).unwrap();

        let link = jail.join("escape.json");
        let _ = std::fs::remove_file(&link);
        symlink(&outside, &link).unwrap();

        let r = FileResolver::with_base_dir(&jail);
        let uri = format!("file://{}", link.display());
        let result = r.resolve("", &uri);
        assert!(
            matches!(result, Err(ResolveError::PathEscaped { .. })),
            "expected PathEscaped for symlink escape, got {result:?}"
        );

        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir(&jail);
        let _ = std::fs::remove_dir(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn file_resolver_post_open_recheck_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let tmp = std::env::temp_dir().join("schemaforge_toctou_recheck_test");
        let jail = tmp.join("jail");
        let outside = tmp.join("outside.json");
        std::fs::create_dir_all(&jail).unwrap();
        std::fs::write(&outside, r#"{"type":"integer"}"#).unwrap();

        let link = jail.join("link.json");
        let _ = std::fs::remove_file(&link);
        symlink(&outside, &link).unwrap();

        let r = FileResolver::with_base_dir(&jail);
        let uri = format!("file://{}", link.display());
        let result = r.resolve("", &uri);
        assert!(
            matches!(result, Err(ResolveError::PathEscaped { .. })),
            "expected PathEscaped after post-open re-check, got {result:?}"
        );

        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir(&jail);
        let _ = std::fs::remove_dir(&tmp);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_self_fd_resolves_to_file_path() {
        use std::os::unix::io::AsRawFd;
        let jail = make_jail();
        let schema_path = jail.join("schema.json");
        let file = std::fs::File::open(&schema_path).unwrap();
        let proc_link = std::path::PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()));
        let via_proc = std::fs::canonicalize(&proc_link).unwrap();
        let via_path = std::fs::canonicalize(&schema_path).unwrap();
        assert_eq!(
            via_proc, via_path,
            "/proc/self/fd/{{fd}} must resolve to the same path as the opened file"
        );
    }

    #[test]
    fn file_resolver_rejects_oversized_file() {
        let jail = make_jail();
        let path = jail.join("large.json");
        std::fs::write(&path, r#"{"type":"string"}"#).unwrap();
        let r = FileResolver::with_base_dir_and_limit(&jail, 5);
        let uri = format!("file://{}", path.display());
        let err = r.resolve("", &uri).unwrap_err();
        assert!(
            matches!(err, ResolveError::SizeExceeded { .. }),
            "expected SizeExceeded for oversized file, got {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_resolver_accepts_file_within_size_limit() {
        let dir = std::env::temp_dir().join("schemaforge_size_ok_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("schema.json");
        std::fs::write(&path, r#"{"type":"string"}"#).unwrap();
        let r = FileResolver::with_base_dir_and_limit(&dir, 1_000_000);
        let uri = format!("file://{}", path.display());
        let result = r.resolve("", &uri);
        assert!(
            result.is_ok(),
            "expected Ok for file within size limit, got {result:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn file_resolver_rejects_directory_as_non_regular_file() {
        let base = std::env::temp_dir().join("schemaforge_dir_reject_test");
        let inner = base.join("subdir");
        std::fs::create_dir_all(&inner).unwrap();

        let r = FileResolver::with_base_dir(&base);
        let uri = format!("file://{}", inner.display());
        let result = r.resolve("", &uri);
        assert!(
            matches!(result, Err(ResolveError::IoError { .. })),
            "expected IoError for directory, got {result:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn file_resolver_rejects_fifo() {
        let dir = std::env::temp_dir().join("schemaforge_fifo_test");
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("pipe.json");
        let _ = std::fs::remove_file(&fifo);

        let status = std::process::Command::new("mkfifo")
            .arg(fifo.to_str().unwrap())
            .status()
            .expect("mkfifo command not found");
        assert!(status.success(), "mkfifo failed: {status}");

        let fifo_clone = fifo.clone();
        let writer = std::thread::spawn(move || {
            let _w = std::fs::OpenOptions::new()
                .write(true)
                .open(&fifo_clone)
                .ok();
        });

        let r = FileResolver::with_base_dir(&dir);
        let uri = format!("file://{}", fifo.display());
        let result = r.resolve("", &uri);

        let _ = writer.join();

        assert!(
            matches!(result, Err(ResolveError::IoError { .. })),
            "expected IoError for FIFO, got {result:?}"
        );

        let _ = std::fs::remove_file(&fifo);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn lexically_normalize_collapses_dotdot() {
        let p = PathBuf::from("/tmp/jail/sub/../../../etc/passwd");
        let normalized = lexically_normalize(&p);
        assert_eq!(normalized, PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn lexically_normalize_keeps_valid_path() {
        let p = PathBuf::from("/tmp/jail/sub/schema.json");
        let normalized = lexically_normalize(&p);
        assert_eq!(normalized, p);
    }

    // ── Fragment-aware resolution ─────────────────────────────────────────────

    fn make_schema_file(dir: &std::path::Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        format!("file://{}", path.display())
    }

    #[test]
    fn file_resolver_strips_empty_fragment_and_returns_whole_doc() {
        let dir = std::env::temp_dir().join("sf_frag_empty_test");
        std::fs::create_dir_all(&dir).unwrap();
        let uri = make_schema_file(&dir, "schema.json", r#"{"type":"string"}"#);
        let r = FileResolver::with_base_dir(&dir);
        let result = r.resolve("", &format!("{uri}#"));
        assert!(result.is_ok(), "empty fragment must succeed: {result:?}");
        assert_eq!(result.unwrap(), serde_json::json!({"type": "string"}));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_resolver_applies_pointer_fragment() {
        let dir = std::env::temp_dir().join("sf_frag_pointer_test");
        std::fs::create_dir_all(&dir).unwrap();
        let content = r#"{"$defs":{"Str":{"type":"string"}}}"#;
        let uri = make_schema_file(&dir, "schema.json", content);
        let r = FileResolver::with_base_dir(&dir);
        let result = r.resolve("", &format!("{uri}#/$defs/Str"));
        assert!(result.is_ok(), "pointer fragment must succeed: {result:?}");
        assert_eq!(result.unwrap(), serde_json::json!({"type": "string"}));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_resolver_applies_anchor_fragment() {
        let dir = std::env::temp_dir().join("sf_frag_anchor_test");
        std::fs::create_dir_all(&dir).unwrap();
        let content = r#"{"$defs":{"Str":{"$anchor":"myStr","type":"string"}}}"#;
        let uri = make_schema_file(&dir, "schema.json", content);
        let r = FileResolver::with_base_dir(&dir);
        let result = r.resolve("", &format!("{uri}#myStr"));
        assert!(result.is_ok(), "anchor fragment must succeed: {result:?}");
        let val = result.unwrap();
        assert_eq!(val["type"], serde_json::json!("string"));
        assert_eq!(val["$anchor"], serde_json::json!("myStr"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_resolver_anchor_fragment_not_found_returns_not_found() {
        let dir = std::env::temp_dir().join("sf_frag_anchor_missing_test");
        std::fs::create_dir_all(&dir).unwrap();
        let uri = make_schema_file(&dir, "schema.json", r#"{"type":"object"}"#);
        let r = FileResolver::with_base_dir(&dir);
        let result = r.resolve("", &format!("{uri}#nonexistent"));
        assert!(
            matches!(result, Err(ResolveError::NotFound(_))),
            "missing anchor must return NotFound, got {result:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_resolver_pointer_fragment_missing_key_returns_not_found() {
        let dir = std::env::temp_dir().join("sf_frag_ptr_missing_test");
        std::fs::create_dir_all(&dir).unwrap();
        let uri = make_schema_file(&dir, "schema.json", r#"{"type":"string"}"#);
        let r = FileResolver::with_base_dir(&dir);
        let result = r.resolve("", &format!("{uri}#/nonexistent/key"));
        assert!(
            matches!(result, Err(ResolveError::NotFound(_))),
            "missing pointer key must return NotFound, got {result:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_resolver_anchor_inside_allof_array() {
        let dir = std::env::temp_dir().join("sf_frag_allof_anchor_test");
        std::fs::create_dir_all(&dir).unwrap();
        let content = r#"{"allOf":[{"$anchor":"inAllOf","type":"string"}]}"#;
        let uri = make_schema_file(&dir, "schema.json", content);
        let r = FileResolver::with_base_dir(&dir);
        let result = r.resolve("", &format!("{uri}#inAllOf"));
        assert!(
            result.is_ok(),
            "anchor under allOf must resolve: {result:?}"
        );
        assert_eq!(result.unwrap()["type"], serde_json::json!("string"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_resolver_offline_hit_still_applies_fragment() {
        let mut r = FileResolver::default();
        r.register(
            "urn:offline",
            serde_json::json!({ "$defs": { "N": { "type": "number" } } }),
        );
        let result = r.resolve("", "urn:offline#/$defs/N");
        assert_eq!(result.unwrap(), serde_json::json!({ "type": "number" }));
    }
}
