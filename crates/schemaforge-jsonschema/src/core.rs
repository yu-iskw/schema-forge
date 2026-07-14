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
        None => out.merge(ValidationOutput::abort(ValidationError::new(
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
    let (base_ref, fragment) = schemaforge_resolver::split_uri_fragment(ref_uri);
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

fn build_registry_key(ref_uri: &str, base_uri: &str) -> String {
    // Match OfflineResolver / FileResolver: resolve, then normalize_uri so
    // registry keys agree with Validator::new / add_schema inserts (dot
    // segments, trailing `#`, and Windows file:// casefold).
    let resolved = schemaforge_resolver::resolve_uri(base_uri, ref_uri);
    let (key, _) = schemaforge_resolver::split_uri_fragment(&resolved);
    schemaforge_resolver::normalize_uri(key.to_owned())
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
    use ValidationOptions as Opts;
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

    // ── build_registry_key relative URI resolution ────────────────────────────

    #[test]
    fn build_registry_key_relative_ref_resolves_against_base() {
        // Relative "b.json" with base "https://example.com/schemas/a.json"
        // must yield "https://example.com/schemas/b.json", not a naive concat.
        let key = build_registry_key("b.json", "https://example.com/schemas/a.json");
        assert_eq!(
            key, "https://example.com/schemas/b.json",
            "relative ref must be resolved against the base URI directory"
        );
    }

    #[test]
    fn build_registry_key_absolute_ref_passthrough() {
        let key = build_registry_key(
            "https://other.com/schema.json",
            "https://example.com/schemas/a.json",
        );
        assert_eq!(key, "https://other.com/schema.json");
    }

    #[test]
    fn relative_ref_resolves_via_validator_registry() {
        // End-to-end: $ref with a relative URI must resolve to the correct
        // registered schema key after resolve_uri is applied.
        let root = json!({"$ref": "b.json"});
        let opts = Opts {
            base_uri: "https://example.com/schemas/a.json".to_owned(),
            ..Default::default()
        };
        let mut v = Validator::new(&root, opts).unwrap();
        v.add_schema(
            "https://example.com/schemas/b.json",
            json!({"type": "integer"}),
        )
        .unwrap();
        assert!(
            v.validate(&json!(42)).is_valid(),
            "relative $ref b.json must resolve to the registered b.json schema"
        );
        assert!(
            !v.validate(&json!("not-int")).is_valid(),
            "schema constraints from b.json must apply"
        );
    }

    // ── base_uri self-ref (fix #2) ─────────────────────────────────────────

    #[test]
    fn base_uri_anchor_ref_resolves_via_absolute_self_ref() {
        // When base_uri is set, a $ref to "<base_uri>#anchorName" must resolve
        // to the anchor defined in the root schema (the self-ref case).
        let schema = json!({
            "$defs": {
                "Str": {"$anchor": "myAnchor", "type": "string"}
            },
            "$ref": "https://example.com/root.json#myAnchor"
        });
        let opts = Opts {
            base_uri: "https://example.com/root.json".to_owned(),
            ..Default::default()
        };
        let v = Validator::new(&schema, opts).unwrap();
        assert!(
            v.validate(&json!("hello")).is_valid(),
            "absolute self-ref with anchor must resolve"
        );
        assert!(
            !v.validate(&json!(42)).is_valid(),
            "anchor schema constraints must apply"
        );
    }

    #[test]
    fn urn_ref_resolves_correctly_when_base_uri_is_http() {
        // A $ref with a urn: scheme is absolute and must NOT be concatenated
        // with the base_uri, even when base_uri is an http URL.
        let root = json!({"$ref": "urn:ext"});
        let opts = Opts {
            base_uri: "https://example.com/root.json".to_owned(),
            ..Default::default()
        };
        let mut v = Validator::new(&root, opts).unwrap();
        let external = json!({"type": "integer"});
        v.add_schema("urn:ext", external).unwrap();
        assert!(
            v.validate(&json!(1)).is_valid(),
            "urn: ref must resolve to the registered schema"
        );
        assert!(
            !v.validate(&json!("not-int")).is_valid(),
            "referenced schema constraints must apply"
        );
    }

    #[test]
    fn base_uri_with_dot_segment_self_ref_resolves_after_normalize() {
        // When base_uri contains a dot segment (e.g. "https://example.com/./root.json"),
        // it must be normalized before storage so that an absolute self-ref to
        // the canonical form ("https://example.com/root.json#anchor") still
        // resolves to the root document's anchor table.
        let schema = json!({
            "$defs": {
                "Str": {"$anchor": "myAnchor", "type": "string"}
            },
            "$ref": "https://example.com/root.json#myAnchor"
        });
        let opts = Opts {
            base_uri: "https://example.com/./root.json".to_owned(),
            ..Default::default()
        };
        let v = Validator::new(&schema, opts).unwrap();
        assert!(
            v.validate(&json!("hello")).is_valid(),
            "absolute self-ref with normalized dot-segment base_uri must resolve"
        );
        assert!(
            !v.validate(&json!(42)).is_valid(),
            "anchor schema constraints must apply"
        );
    }

    #[test]
    fn add_schema_with_dot_segment_id_resolves_via_canonical_ref() {
        // When a schema is added with a dot-segment URI
        // ("https://example.com/./ext.json"), it must be normalised to its
        // canonical form so that a $ref to the canonical URI resolves.
        let root = json!({"$ref": "https://example.com/ext.json"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        v.add_schema("https://example.com/./ext.json", json!({"type": "integer"}))
            .unwrap();
        assert!(
            v.validate(&json!(42)).is_valid(),
            "dot-segment add_schema id must normalise to canonical key"
        );
        assert!(
            !v.validate(&json!("not-int")).is_valid(),
            "referenced schema constraints must apply"
        );
    }

    #[test]
    fn urn_ref_with_anchor_resolves_correctly_when_base_uri_is_http() {
        // urn:ext#anchorName must look up the anchor in urn:ext only,
        // even when the validator has a non-empty http base_uri.
        let root = json!({"$ref": "urn:ext#remoteAnchor"});
        let opts = Opts {
            base_uri: "https://example.com/root.json".to_owned(),
            ..Default::default()
        };
        let mut v = Validator::new(&root, opts).unwrap();
        let external = json!({"$anchor": "remoteAnchor", "type": "string"});
        v.add_schema("urn:ext", external).unwrap();
        assert!(
            v.validate(&json!("hi")).is_valid(),
            "urn: anchor ref must resolve correctly"
        );
        assert!(
            !v.validate(&json!(42)).is_valid(),
            "anchor schema constraints must apply"
        );
    }
}
