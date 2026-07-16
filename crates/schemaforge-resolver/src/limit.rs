//! Size- and depth-limiting resolver wrapper.

use serde_json::Value;

use crate::{ResolveError, Resolver};

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
        // Single tree walk for size + depth (estimate may under-count slightly
        // for strings that need multi-byte `\uXXXX` escapes).
        let (size, depth) = json_size_and_depth(&value);
        if size > self.max_bytes {
            return Err(ResolveError::SizeExceeded {
                uri: reference.to_owned(),
                size,
                limit: self.max_bytes,
            });
        }
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

/// Compute nesting depth and estimated compact-JSON byte size in one walk.
pub(crate) fn json_size_and_depth(v: &Value) -> (usize, usize) {
    match v {
        Value::Null | Value::Bool(true) => (4, 1),
        Value::Bool(false) => (5, 1),
        Value::Number(n) => (n.to_string().len(), 1),
        Value::String(s) => (json_string_byte_size(s), 1),
        Value::Array(arr) => {
            let mut size = 2 + arr.len().saturating_sub(1); // "[" "]" and commas
            let mut child_depth = 0;
            for item in arr {
                let (s, d) = json_size_and_depth(item);
                size += s;
                child_depth = child_depth.max(d);
            }
            (size, child_depth + 1)
        }
        Value::Object(obj) => {
            let mut size = 2 + obj.len().saturating_sub(1); // "{" "}" and commas
            let mut child_depth = 0;
            for (k, val) in obj {
                size += json_string_byte_size(k) + 1; // "key":
                let (s, d) = json_size_and_depth(val);
                size += s;
                child_depth = child_depth.max(d);
            }
            (size, child_depth + 1)
        }
    }
}

/// Quoted JSON string length including single-char escapes (`"`, `\`, controls).
fn json_string_byte_size(s: &str) -> usize {
    let escapes = s
        .bytes()
        .filter(|b| matches!(b, b'"' | b'\\' | 0x00..=0x1f))
        .count();
    2 + s.len() + escapes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::OfflineResolver;
    use serde_json::json;

    fn json_depth(v: &Value) -> usize {
        json_size_and_depth(v).1
    }

    fn json_byte_size(v: &Value) -> usize {
        json_size_and_depth(v).0
    }

    fn build_deep_schema(depth: usize) -> Value {
        let mut v = json!("leaf");
        for _ in 0..depth {
            v = json!({"nested": v});
        }
        v
    }

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

    #[test]
    fn json_byte_size_primitives() {
        assert_eq!(json_byte_size(&json!(null)), 4);
        assert_eq!(json_byte_size(&json!(true)), 4);
        assert_eq!(json_byte_size(&json!(false)), 5);
        assert_eq!(json_byte_size(&json!("hi")), 4); // "hi"
    }

    #[test]
    fn json_byte_size_is_close_to_serialised_len() {
        let schema = json!({"type": "string", "minLength": 1, "maxLength": 100});
        let serialised = serde_json::to_string(&schema).unwrap();
        let estimated = json_byte_size(&schema);
        assert!(
            estimated.abs_diff(serialised.len()) <= 10,
            "estimate {estimated} too far from actual {actual}",
            actual = serialised.len()
        );
    }

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
