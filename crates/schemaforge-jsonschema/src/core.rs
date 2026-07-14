//! Core vocabulary keyword processing (`$ref`, `$dynamicRef`, `$id`, `$schema`, `$defs`).

use serde_json::{Map, Value};

use crate::{ValidationContext, ValidationOutput};

/// Apply core vocabulary keywords from `obj` to `instance`.
pub(crate) fn apply(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    apply_ref(obj, instance, path, ctx, out);
    apply_dynamic_ref(obj, instance, path, ctx, out);
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
    if let Some(schema) = resolve_ref(ref_uri, ctx) {
        let result = crate::validate_schema(&schema, instance, path, ctx);
        out.merge(result);
    }
}

/// Resolve a `$ref` URI.
///
/// Fragment-only references (e.g. `#`, `#/$defs/Foo`) are resolved as JSON
/// Pointers against the root schema document.  Absolute or relative URIs are
/// looked up in the external registry.
fn resolve_ref(ref_uri: &str, ctx: &ValidationContext<'_>) -> Option<Value> {
    if let Some(pointer) = ref_uri.strip_prefix('#') {
        return resolve_json_pointer(ctx.root_schema, pointer).cloned();
    }
    let key = build_registry_key(ref_uri, ctx.base_uri);
    ctx.registry.get(&key).cloned()
}

fn build_registry_key(ref_uri: &str, base_uri: &str) -> String {
    if ref_uri.starts_with("http://") || ref_uri.starts_with("https://") {
        ref_uri.to_owned()
    } else {
        format!("{base_uri}{ref_uri}")
    }
}

// ── $dynamicRef ───────────────────────────────────────────────────────────────

fn apply_dynamic_ref(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::String(dyn_ref)) = obj.get("$dynamicRef") else {
        return;
    };
    if let Some(schema) = resolve_dynamic_ref(dyn_ref, ctx) {
        out.merge(crate::validate_schema(&schema, instance, path, ctx));
    }
}

/// Resolve a `$dynamicRef`.
///
/// A `$dynamicRef` of the form `#anchor-name` is looked up in the root
/// document's `$dynamicAnchor` registry.  This is a simplified single-document
/// implementation; cross-document dynamic scope resolution is not yet supported.
fn resolve_dynamic_ref(dyn_ref: &str, ctx: &ValidationContext<'_>) -> Option<Value> {
    let anchor = dyn_ref.strip_prefix('#')?;
    ctx.dynamic_anchors.get(anchor).cloned()
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

fn unescape_token(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
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
    fn dynamic_anchor_and_ref() {
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
        assert!(valid(&schema, &json!(["a", "b"])));
        assert!(!valid(&schema, &json!(["a", 1])));
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
}
