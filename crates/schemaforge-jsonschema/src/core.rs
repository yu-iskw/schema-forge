//! Core vocabulary keyword processing (`$ref`, `$id`, `$schema`, `$defs`).
//!
//! `$dynamicRef`, `$dynamicAnchor`, `$recursiveRef`, `$recursiveAnchor`, and
//! `dependencies` are rejected at schema-construction time by
//! [`crate::Validator::new`] and are therefore not handled here.

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
/// - `#name` (no leading `/` after `#`) → looked up in the root document's
///   `$anchor` table only (key `""`).
///
/// External URIs (with or without a fragment) are resolved against the
/// registry:
/// 1. The fragment (if any) is stripped from the URI to form the registry key.
/// 2. The base document is looked up in the registry.
/// 3. The fragment is applied against the base document:
///    - Empty fragment → return the document root.
///    - `/…` fragment → follow as a JSON Pointer (RFC 6901).
///    - Non-`/` fragment → treat as a `$anchor` name looked up **only** in
///      that document's own anchor table.  Anchors from other documents are
///      never consulted, so a `urn:other#name` ref cannot accidentally match
///      an anchor defined in the root or a third schema.
///
/// Returns `None` when the target cannot be found, which callers treat as a
/// validation failure.
pub(crate) fn resolve_ref<'a>(ref_uri: &str, ctx: &ValidationContext<'a>) -> Option<&'a Value> {
    if let Some(fragment) = ref_uri.strip_prefix('#') {
        if fragment.is_empty() || fragment.starts_with('/') {
            return resolve_json_pointer(ctx.root_schema, fragment);
        }
        // Non-pointer fragment: look up anchor in the root document only.
        return ctx.anchors_by_doc.get("").and_then(|a| a.get(fragment));
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
        // Non-pointer fragment: look up anchor only in that document's table.
        ctx.anchors_by_doc.get(&key).and_then(|a| a.get(fragment))
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

/// Follow a JSON Pointer (RFC 6901) from `root` via [`Value::pointer`].
fn resolve_json_pointer<'a>(root: &'a Value, pointer: &str) -> Option<&'a Value> {
    root.pointer(pointer)
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

    // ── Per-document anchor isolation ─────────────────────────────────────────

    #[test]
    fn external_anchor_resolves_only_in_its_own_doc() {
        // `"$ref": "urn:ext#anchor"` must find the anchor in `urn:ext` only,
        // not in the root schema or any other document.
        let root = json!({"$ref": "urn:ext#remoteAnchor"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let external = json!({"$anchor": "remoteAnchor", "type": "integer"});
        v.add_schema("urn:ext", external).unwrap();
        assert!(
            v.validate(&json!(1)).is_valid(),
            "external anchor must be reachable via urn:ext#remoteAnchor"
        );
        assert!(
            !v.validate(&json!("not-int")).is_valid(),
            "anchor schema constraints must apply"
        );
    }

    #[test]
    fn root_anchor_not_reachable_via_foreign_uri_ref() {
        // An anchor defined only in the root schema must NOT be resolved when
        // the $ref names an unrelated external document.
        let root = json!({
            "$defs": {
                "Root": {"$anchor": "rootAnchor", "type": "string"}
            },
            // This ref asks for "rootAnchor" from urn:ext, which doesn't have it.
            "$ref": "urn:ext#rootAnchor"
        });
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let external = json!({"type": "object"});
        v.add_schema("urn:ext", external).unwrap();
        // urn:ext has no anchor named "rootAnchor", so the ref must fail.
        assert!(
            !v.validate(&json!("hello")).is_valid(),
            "anchor from root must not resolve via an external URI ref"
        );
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
    fn pointer_tilde_unescape() {
        let root = json!({"a/b": {"c~d": 1}});
        assert_eq!(resolve_json_pointer(&root, "/a~1b/c~0d"), Some(&json!(1)));
    }
}
