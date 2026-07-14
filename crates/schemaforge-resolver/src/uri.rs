//! URI helpers: normalization, resolution, fragment stripping.

/// Normalize a URI for use as a registry key (strip trailing `#`).
pub(crate) fn normalize_uri(mut uri: String) -> String {
    if uri.ends_with('#') {
        uri.pop();
    }
    uri
}

/// Split a URI into `(base, fragment)` at the first `#`.
///
/// Returns `(uri, "")` when the URI contains no `#`.
pub(crate) fn split_uri_fragment(uri: &str) -> (&str, &str) {
    uri.find('#')
        .map_or((uri, ""), |pos| (&uri[..pos], &uri[pos + 1..]))
}

/// Strip the fragment component (`#...`) from a URI.
pub(crate) fn strip_fragment(uri: &str) -> &str {
    split_uri_fragment(uri).0
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

pub(crate) fn is_absolute_uri(uri: &str) -> bool {
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
}
