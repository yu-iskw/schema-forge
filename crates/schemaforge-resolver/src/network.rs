//! Network-deny resolver — always rejects remote URI resolution.

use serde_json::Value;

use crate::{ResolveError, Resolver};

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
