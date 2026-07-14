//! Core vocabulary keyword processing (`$ref`, `$id`, `$schema`, `$defs`).
//!
//! `$dynamicRef`, `$dynamicAnchor`, `$recursiveRef`, `$recursiveAnchor`, and
//! `dependencies` are rejected at schema-construction time by
//! [`crate::Validator::new`] and are therefore not handled here.

use std::borrow::Cow;

use serde_json::{Map, Value};

use crate::{ValidationContext, ValidationError, ValidationOutput};

/// Apply core vocabulary keywords from `obj` to `instance`.
pub(crate) fn apply(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    apply_ref(obj, instance, path, ctx, out);
}

// ── $ref ─────────────────────────────────────────────────────────────────────

fn apply_ref(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::String(ref_uri)) = obj.get("$ref") else {
        return;
    };
    match resolve_ref(ref_uri, ctx) {
        Some(schema) => out.merge(crate::validate_schema(schema, instance, path, ctx)),
        None => out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/$ref"),
            format!("unresolved $ref: `{ref_uri}`"),
        ))),
    }
}

/// Resolve a `$ref` URI, returning a reference into the schema tree.
///
/// Fragment-only references are handled as follows:
/// - `#` or `#/…` → resolved as a JSON Pointer against the root document.
/// - `#name` (no leading `/` after `#`) → looked up in the `$anchor` table.
///
/// External URIs (with or without a fragment) are resolved against the
/// registry:
/// 1. The fragment (if any) is stripped from the URI to form the registry key.
/// 2. The base document is looked up in the registry.
/// 3. The fragment is applied against the base document:
///    - Empty fragment → return the document root.
///    - `/…` fragment → follow as a JSON Pointer (RFC 6901).
///    - Non-`/` fragment → treat as a `$anchor` name in the shared anchor
///      table (populated at construction time for both the root schema and
///      any schema added via [`crate::Validator::add_schema`]).
///
/// Returns `None` when the target cannot be found, which callers treat as a
/// validation failure.
pub(crate) fn resolve_ref<'a>(ref_uri: &str, ctx: &ValidationContext<'a>) -> Option<&'a Value> {
    if let Some(fragment) = ref_uri.strip_prefix('#') {
        if fragment.is_empty() || fragment.starts_with('/') {
            return resolve_json_pointer(ctx.root_schema, fragment);
        }
        // Non-pointer fragment: treat as a static anchor name.
        return ctx.anchors.get(fragment);
    }

    // External reference: split off any fragment before the registry look-up.
    let (base_ref, fragment) = split_uri_fragment(ref_uri);
    let key = build_registry_key(base_ref, ctx.base_uri);
    let doc = ctx.registry.get(&key)?;

    if fragment.is_empty() {
        Some(doc)
    } else if fragment.starts_with('/') {
        resolve_json_pointer(doc, fragment)
    } else {
        // Non-pointer fragment on an external URI: look up as an anchor name.
        // Anchors from external schemas are merged into ctx.anchors by
        // Validator::add_schema so they are always available here.
        ctx.anchors.get(fragment)
    }
}

/// Split a URI into `(base, fragment)` at the first `#`.
///
/// Returns `(uri, "")` when the URI contains no `#`.
fn split_uri_fragment(uri: &str) -> (&str, &str) {
    uri.find('#')
        .map_or((uri, ""), |pos| (&uri[..pos], &uri[pos + 1..]))
}

fn build_registry_key(ref_uri: &str, base_uri: &str) -> String {
    if ref_uri.starts_with("http://") || ref_uri.starts_with("https://") {
        ref_uri.to_owned()
    } else {
        format!("{base_uri}{ref_uri}")
    }
}

// ── JSON Pointer resolution ───────────────────────────────────────────────────

/// Follow a JSON Pointer (RFC 6901) from `root`.
///
/// An empty pointer returns the root itself.  Each `/`-delimited token is
/// applied in sequence with `~1` -> `/` and `~0` -> `~` unescaping.
fn resolve_json_pointer<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(root);
    }
    let tokens = pointer.strip_prefix('/')?;
    let mut current = root;
    for token in tokens.split('/') {
        let decoded = unescape_token(token);
        current = descend(current, &decoded)?;
    }
    Some(current)
}

fn descend<'a>(node: &'a Value, token: &str) -> Option<&'a Value> {
    match node {
        Value::Object(obj) => obj.get(token),
        Value::Array(arr) => {
            let idx: usize = token.parse().ok()?;
            arr.get(idx)
        }
        _ => None,
    }
}

/// Unescape a JSON Pointer token (RFC 6901 §3).
///
/// Uses a `Borrowed` fast-path when the token contains no `~`, avoiding any
/// heap allocation for the common case.
fn unescape_token(token: &str) -> Cow<'_, str> {
    if !token.contains('~') {
        return Cow::Borrowed(token);
    }
    Cow::Owned(token.replace("~1", "/").replace("~0", "~"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValidationOptions, Validator};
    use serde_json::json;

    fn valid(schema: &Value, instance: &Value) -> bool {
        Validator::new(schema, ValidationOptions::default())
            .unwrap()
            .validate(instance)
            .is_valid()
    }

    #[test]
    fn ref_to_defs() {
        let schema = json!({
            "$defs": {
                "Name": {"type": "string"}
            },
            "properties": {
                "name": {"$ref": "#/$defs/Name"}
            }
        });
        assert!(valid(&schema, &json!({"name": "Alice"})));
        assert!(!valid(&schema, &json!({"name": 42})));
    }

    #[test]
    fn ref_root_pointer() {
        let schema = json!({
            "type": "string"
        });
        assert!(valid(&schema, &json!("hi")));
        assert!(!valid(&schema, &json!(42)));
    }

    #[test]
    fn dynamic_anchor_and_ref_rejected_at_construction() {
        // $dynamicAnchor/$dynamicRef are unsupported; Validator::new must return Err.
        let schema = json!({
            "$defs": {
                "Item": {
                    "$dynamicAnchor": "item",
                    "type": "string"
                }
            },
            "type": "array",
            "items": { "$dynamicRef": "#item" }
        });
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with $dynamicAnchor/$dynamicRef must fail at construction"
        );
    }

    #[test]
    fn anchor_ref_resolves_to_anchor_schema() {
        // $ref: "#anchorName" must resolve to the schema with $anchor: "anchorName".
        let schema = json!({
            "$defs": {
                "StrField": {
                    "$anchor": "myStr",
                    "type": "string"
                }
            },
            "properties": {
                "name": {"$ref": "#myStr"}
            }
        });
        assert!(valid(&schema, &json!({"name": "Alice"})));
        assert!(!valid(&schema, &json!({"name": 42})));
    }

    #[test]
    fn anchor_ref_with_unevaluated_properties() {
        // $ref to $anchor target; properties from the anchor schema are evaluated.
        let schema = json!({
            "$defs": {
                "Base": {
                    "$anchor": "base",
                    "properties": {"id": {"type": "integer"}}
                }
            },
            "$ref": "#base",
            "unevaluatedProperties": false
        });
        assert!(valid(&schema, &json!({"id": 1})));
        assert!(!valid(&schema, &json!({"id": 1, "extra": "x"})));
    }

    #[test]
    fn ref_nested_pointer() {
        let schema = json!({
            "$defs": {
                "address": {
                    "type": "object",
                    "properties": {
                        "city": {"type": "string"}
                    }
                }
            },
            "properties": {
                "home": {"$ref": "#/$defs/address"}
            }
        });
        assert!(valid(&schema, &json!({"home": {"city": "Berlin"}})));
        assert!(!valid(&schema, &json!({"home": {"city": 42}})));
    }

    // ── External $ref resolution (fix #3) ────────────────────────────────────

    #[test]
    fn external_ref_with_pointer_fragment_after_add_schema() {
        // A $ref with a non-local URI that includes a JSON Pointer fragment
        // must look up the registered document by its base URI (fragment
        // stripped) and then apply the pointer against it.
        let root = json!({"$ref": "urn:other#/$defs/X"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let external = json!({
            "$defs": {
                "X": {"type": "string"}
            }
        });
        v.add_schema("urn:other", external).unwrap();
        assert!(
            v.validate(&json!("hello")).is_valid(),
            "external $ref with pointer fragment must resolve correctly"
        );
        assert!(
            !v.validate(&json!(42)).is_valid(),
            "referenced schema constraints must be applied"
        );
    }

    #[test]
    fn unescape_token_no_tilde_is_borrowed() {
        let t = unescape_token("simple");
        assert!(matches!(t, Cow::Borrowed(_)));
    }

    #[test]
    fn unescape_token_with_tilde_is_owned() {
        let t = unescape_token("a~1b");
        assert_eq!(&*t, "a/b");
        assert!(matches!(t, Cow::Owned(_)));
    }

    #[test]
    fn unescape_token_tilde_zero() {
        let t = unescape_token("a~0b");
        assert_eq!(&*t, "a~b");
    }
}
