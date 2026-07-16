//! URI helpers: normalization, resolution, fragment stripping.

/// Remove-dot-segments for `file://` URIs.
///
/// Handles `.` and `..` path segments and additionally collapses duplicate
/// slashes (`//`) and strips a trailing `/`, so that filesystem aliases such
/// as `file:///tmp//a.json` and `file:///tmp/a.json` map to the same key.
/// The leading empty segment of an absolute path is preserved so the result
/// remains rooted.
fn remove_dot_segments(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for (i, seg) in path.split('/').enumerate() {
        match seg {
            ".." => {
                // Never pop the leading empty segment (represents the root `/`).
                if stack.last().is_some_and(|s: &&str| !s.is_empty()) {
                    stack.pop();
                }
            }
            // Keep the very first empty segment so that absolute paths stay
            // rooted; skip every subsequent empty segment (produced by `//`
            // or a trailing `/`).  Treat `.` the same as a redundant empty.
            "" if i == 0 => stack.push(seg),
            "." | "" => {}
            other => stack.push(other),
        }
    }
    let joined = stack.join("/");
    // A pure-slash path (e.g. `/`) leaves only the leading `""` on the stack,
    // which joins to an empty string.  Restore the canonical root `/`.
    if joined.is_empty() && path.starts_with('/') {
        "/".to_owned()
    } else {
        joined
    }
}

/// RFC 3986 Section 5.2.4 remove-dot-segments for non-`file://` URIs.
///
/// Only eliminates `.` (current-dir) and `..` (parent-dir) segments.
/// Empty segments from `//` and the trailing empty segment from a trailing `/`
/// are **preserved**, so that `https://example.com/api/` and
/// `https://example.com/api` remain distinct registry keys.
fn remove_dot_segments_rfc3986(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            ".." => {
                if stack.last().is_some_and(|s: &&str| !s.is_empty()) {
                    stack.pop();
                }
            }
            "." => {}
            other => stack.push(other),
        }
    }
    let joined = stack.join("/");
    if joined.is_empty() && path.starts_with('/') {
        "/".to_owned()
    } else {
        joined
    }
}

/// Lowercase the path component of a `file://` URI.
///
/// The scheme (`file://`) and any authority are preserved; only the path
/// (everything from the first `/` after the authority) is lowercased.
///
/// Used by [`normalize_uri`] on `cfg(windows)` to produce case-insensitive
/// registry keys for the case-insensitive Windows filesystem.
///
/// Compiled on `cfg(any(windows, test))` so it can be unit-tested on all
/// platforms without triggering the dead-code lint on non-Windows production
/// builds.
#[cfg(any(windows, test))]
pub(crate) fn casefold_file_uri_path(uri: String) -> String {
    if !uri.starts_with("file://") {
        return uri;
    }
    let prefix_len = "file://".len();
    // The authority is everything between "file://" and the first '/'; the
    // path is everything from that '/' onward.  Use a temporary borrow so
    // `uri` is not kept borrowed past the `find` call.
    let Some(rel_slash) = uri[prefix_len..].find('/') else {
        return uri;
    };
    let authority_end = prefix_len + rel_slash;
    format!(
        "file://{}{}",
        &uri[prefix_len..authority_end],
        uri[authority_end..].to_lowercase()
    )
}

/// Normalize the path component of a `scheme://authority/path` URI.
///
/// - For `file://` URIs: applies [`remove_dot_segments`], which collapses
///   duplicate slashes and strips trailing slashes so filesystem aliases map
///   to the same key.
/// - For all other schemes (http, https, urn, …): applies
///   [`remove_dot_segments_rfc3986`], which only removes `.` and `..`
///   segments; empty segments and trailing slashes are preserved so that
///   `https://example.com/api/` and `https://example.com/api` remain distinct.
/// - URIs without `://` (e.g. URNs) are returned as-is.
///
/// The scheme, authority, query, and fragment are never modified.
pub(crate) fn normalize_path_in_uri(uri: &str) -> String {
    // Locate where the authority ends and the path begins.
    let path_start = uri.find("://").and_then(|dslash| {
        let after_authority = dslash + 3;
        uri[after_authority..]
            .find('/')
            .map(|i| after_authority + i)
    });

    let Some(path_start) = path_start else {
        return uri.to_string();
    };

    let prefix = &uri[..path_start];
    let rest = &uri[path_start..];

    // Isolate the fragment and query so only the path is normalized.
    let (path_and_query, fragment) = rest
        .find('#')
        .map_or((rest, ""), |i| (&rest[..i], &rest[i..]));
    let (path, query) = path_and_query.find('?').map_or((path_and_query, ""), |i| {
        (&path_and_query[..i], &path_and_query[i..])
    });

    let normalized = if uri.starts_with("file://") {
        remove_dot_segments(path)
    } else {
        remove_dot_segments_rfc3986(path)
    };

    format!("{prefix}{normalized}{query}{fragment}")
}

/// Normalize a URI for use as a registry key.
///
/// - Strips a trailing bare `#`.
/// - Applies RFC 3986 remove-dot-segments to the path component so that
///   `file:///a/./b.json` and `file:///a/b.json` map to the same key.
/// - On Windows, lowercases the path component of `file://` URIs so that
///   the case-insensitive filesystem is reflected in the registry key.
#[must_use]
pub fn normalize_uri(mut uri: String) -> String {
    if uri.ends_with('#') {
        uri.pop();
    }
    #[cfg(windows)]
    {
        uri = casefold_file_uri_path(uri);
    }
    normalize_path_in_uri(&uri)
}

/// Split a URI into `(base, fragment)` at the first `#`.
///
/// Returns `(uri, "")` when the URI contains no `#`.
#[must_use]
pub fn split_uri_fragment(uri: &str) -> (&str, &str) {
    uri.find('#')
        .map_or((uri, ""), |pos| (&uri[..pos], &uri[pos + 1..]))
}

/// Strip the fragment component (`#...`) from a URI.
pub(crate) fn strip_fragment(uri: &str) -> &str {
    split_uri_fragment(uri).0
}

/// Resolve `reference` against `base` following RFC 3986 (simplified).
///
/// If `reference` is absolute (contains `://`), it is returned with dot
/// segments normalized.  Fragment-only references are resolved against the
/// base.  All other references are merged with the base directory and then
/// normalized so that `./` and `../` segments collapse.
#[must_use]
pub fn resolve_uri(base: &str, reference: &str) -> String {
    if is_absolute_uri(reference) {
        return normalize_path_in_uri(reference);
    }
    if reference.starts_with('#') {
        let base_no_frag = strip_fragment(base);
        return format!("{base_no_frag}{reference}");
    }
    let base_dir = base_directory(base);
    let raw = if reference.starts_with('/') {
        let scheme_end = base.find("://").map_or(0, |i| i + 3);
        let authority_end = base[scheme_end..]
            .find('/')
            .map_or(base.len(), |i| scheme_end + i);
        format!("{}{reference}", &base[..authority_end])
    } else {
        format!("{base_dir}{reference}")
    };
    normalize_path_in_uri(&raw)
}

/// Return `true` when `uri` is self-contained and must not be resolved
/// relative to any base.  Covers `scheme://…` URIs (http, https, file, …)
/// and `urn:` URNs.
#[must_use]
pub fn is_absolute_uri(uri: &str) -> bool {
    uri.contains("://") || uri.starts_with("urn:")
}

pub(crate) fn base_directory(uri: &str) -> &str {
    uri.rfind('/').map_or("", |i| &uri[..=i])
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn split_uri_fragment_splits_at_hash() {
        assert_eq!(
            split_uri_fragment("https://ex.com/s.json#/$defs/X"),
            ("https://ex.com/s.json", "/$defs/X")
        );
        assert_eq!(split_uri_fragment("urn:x"), ("urn:x", ""));
        assert_eq!(split_uri_fragment("urn:x#"), ("urn:x", ""));
    }

    // ── RFC 3986 remove-dot-segments ─────────────────────────────────────────

    #[test]
    fn remove_dot_segments_single_dot() {
        assert_eq!(remove_dot_segments("/a/./b.json"), "/a/b.json");
    }

    #[test]
    fn remove_dot_segments_double_dot() {
        assert_eq!(remove_dot_segments("/a/b/../c.json"), "/a/c.json");
    }

    #[test]
    fn remove_dot_segments_chained_double_dot() {
        assert_eq!(remove_dot_segments("/a/b/c/../../d.json"), "/a/d.json");
    }

    #[test]
    fn remove_dot_segments_double_dot_at_root_stays_rooted() {
        assert_eq!(remove_dot_segments("/a/../../b.json"), "/b.json");
    }

    #[test]
    fn normalize_uri_collapses_single_dot() {
        assert_eq!(
            normalize_uri("file:///tmp/./schema.json".to_string()),
            "file:///tmp/schema.json"
        );
    }

    #[test]
    fn normalize_uri_collapses_double_dot() {
        assert_eq!(
            normalize_uri("file:///tmp/sub/../schema.json".to_string()),
            "file:///tmp/schema.json"
        );
    }

    #[test]
    fn normalize_uri_strips_trailing_hash_then_normalizes() {
        assert_eq!(
            normalize_uri("file:///tmp/./schema.json#".to_string()),
            "file:///tmp/schema.json"
        );
    }

    #[test]
    fn resolve_uri_collapses_dot_in_absolute_ref() {
        let result = resolve_uri("", "file:///tmp/./schema.json");
        assert_eq!(result, "file:///tmp/schema.json");
    }

    #[test]
    fn resolve_uri_collapses_dotdot_after_merge() {
        let result = resolve_uri("file:///tmp/sub/nested.json", "../schema.json");
        assert_eq!(result, "file:///tmp/schema.json");
    }

    #[test]
    fn resolve_uri_dot_in_absolute_ref_with_fragment() {
        let result = resolve_uri("", "file:///tmp/./schema.json#diskOnly");
        assert_eq!(result, "file:///tmp/schema.json#diskOnly");
    }

    #[test]
    fn resolve_uri_dotdot_with_fragment() {
        let result = resolve_uri("file:///tmp/sub/nested.json", "../schema.json#diskOnly");
        assert_eq!(result, "file:///tmp/schema.json#diskOnly");
    }

    // ── Double-slash and trailing-slash collapsing ────────────────────────────

    #[test]
    fn remove_dot_segments_double_slash_collapses() {
        assert_eq!(remove_dot_segments("/a//b.json"), "/a/b.json");
    }

    #[test]
    fn remove_dot_segments_dot_double_slash_collapses() {
        assert_eq!(remove_dot_segments("/a/.//b.json"), "/a/b.json");
    }

    #[test]
    fn remove_dot_segments_trailing_slash_stripped() {
        assert_eq!(remove_dot_segments("/a/b.json/"), "/a/b.json");
    }

    #[test]
    fn remove_dot_segments_root_path_preserved() {
        assert_eq!(remove_dot_segments("/"), "/");
    }

    #[test]
    fn normalize_uri_collapses_double_slash_in_path() {
        assert_eq!(
            normalize_uri("file:///tmp//schema.json".to_string()),
            "file:///tmp/schema.json"
        );
    }

    #[test]
    fn normalize_uri_strips_trailing_slash() {
        assert_eq!(
            normalize_uri("file:///tmp/schema.json/".to_string()),
            "file:///tmp/schema.json"
        );
    }

    #[test]
    fn normalize_path_in_uri_double_slash_with_fragment() {
        assert_eq!(
            normalize_path_in_uri("file:///tmp//schema.json#diskOnly"),
            "file:///tmp/schema.json#diskOnly"
        );
    }

    #[test]
    fn normalize_path_in_uri_trailing_slash_with_fragment() {
        assert_eq!(
            normalize_path_in_uri("file:///tmp/schema.json/#diskOnly"),
            "file:///tmp/schema.json#diskOnly"
        );
    }

    // ── Scheme-specific normalization (https vs file) ─────────────────────────

    #[test]
    fn normalize_uri_https_trailing_slash_is_preserved() {
        // https:// trailing slash must NOT be stripped — it is a distinct key.
        assert_eq!(
            normalize_uri("https://example.com/api/".to_string()),
            "https://example.com/api/"
        );
    }

    #[test]
    fn normalize_uri_https_no_trailing_slash_is_preserved() {
        assert_eq!(
            normalize_uri("https://example.com/api".to_string()),
            "https://example.com/api"
        );
    }

    #[test]
    fn normalize_uri_https_trailing_slash_and_no_trailing_slash_are_distinct() {
        let with_slash = normalize_uri("https://example.com/api/".to_string());
        let without_slash = normalize_uri("https://example.com/api".to_string());
        assert_ne!(
            with_slash, without_slash,
            "https trailing slash must not be stripped: '{with_slash}' vs '{without_slash}'"
        );
    }

    #[test]
    fn normalize_uri_https_double_slash_is_preserved() {
        // Double slash in an https path is not collapsed (only . and .. removed).
        assert_eq!(
            normalize_uri("https://example.com/api//v2".to_string()),
            "https://example.com/api//v2"
        );
    }

    #[test]
    fn normalize_uri_https_dot_segments_are_removed() {
        assert_eq!(
            normalize_uri("https://example.com/a/./b.json".to_string()),
            "https://example.com/a/b.json"
        );
        assert_eq!(
            normalize_uri("https://example.com/a/b/../c.json".to_string()),
            "https://example.com/a/c.json"
        );
    }

    #[test]
    fn normalize_uri_file_double_slash_collapses() {
        // file:// double slash is still collapsed.
        assert_eq!(
            normalize_uri("file:///tmp//a.json".to_string()),
            "file:///tmp/a.json"
        );
    }

    #[test]
    fn normalize_uri_file_trailing_slash_is_stripped() {
        // file:// trailing slash is still stripped.
        assert_eq!(
            normalize_uri("file:///tmp/schema.json/".to_string()),
            "file:///tmp/schema.json"
        );
    }

    // ── casefold_file_uri_path ────────────────────────────────────────────────

    #[test]
    fn casefold_file_uri_path_lowercases_path_component() {
        let input = "file:///C:/Schemas/MySchema.json".to_owned();
        let result = casefold_file_uri_path(input);
        assert_eq!(result, "file:///c:/schemas/myschema.json");
    }

    #[test]
    fn casefold_file_uri_path_lowercases_path_with_authority() {
        let input = "file://localhost/C:/SCHEMAS/schema.json".to_owned();
        let result = casefold_file_uri_path(input);
        assert_eq!(result, "file://localhost/c:/schemas/schema.json");
    }

    #[test]
    fn casefold_file_uri_path_leaves_non_file_uri_unchanged() {
        let input = "https://EXAMPLE.com/SCHEMAS/Schema.json".to_owned();
        let result = casefold_file_uri_path(input.clone());
        assert_eq!(result, input);
    }

    #[test]
    fn casefold_file_uri_path_leaves_urn_unchanged() {
        let input = "urn:UPPER:case".to_owned();
        let result = casefold_file_uri_path(input.clone());
        assert_eq!(result, input);
    }

    #[cfg(windows)]
    #[test]
    fn normalize_uri_casefoldsfile_uri_on_windows() {
        let result = normalize_uri("file:///C:/Schemas/MySchema.json".to_owned());
        assert_eq!(result, "file:///c:/schemas/myschema.json");
    }
}
