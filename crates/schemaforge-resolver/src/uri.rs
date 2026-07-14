//! URI helpers: normalization, resolution, fragment stripping.

/// Apply RFC 3986 Section 5.2.4 remove-dot-segments to a URI path component.
///
/// Handles `.` (current directory) and `..` (parent directory) path segments
/// so that `file:///a/./b.json` and `file:///a/b.json` produce the same path.
fn remove_dot_segments(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "." => {}
            ".." => {
                // Never pop an empty leading segment (represents the root `/`).
                if stack.last().is_some_and(|s: &&str| !s.is_empty()) {
                    stack.pop();
                }
            }
            other => stack.push(other),
        }
    }
    stack.join("/")
}

/// Normalize the path component of a `scheme://authority/path` URI by applying
/// RFC 3986 remove-dot-segments.  The scheme, authority, query, and fragment
/// are left unchanged.  URIs without `://` (e.g. URNs) are returned as-is.
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

    format!("{prefix}{}{query}{fragment}", remove_dot_segments(path))
}

/// Normalize a URI for use as a registry key.
///
/// - Strips a trailing bare `#`.
/// - Applies RFC 3986 remove-dot-segments to the path component so that
///   `file:///a/./b.json` and `file:///a/b.json` map to the same key.
pub(crate) fn normalize_uri(mut uri: String) -> String {
    if uri.ends_with('#') {
        uri.pop();
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
}
