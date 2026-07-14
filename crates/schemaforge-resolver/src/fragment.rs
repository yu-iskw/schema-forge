//! URI fragment application for resolved JSON Schema documents.

use serde_json::{Map, Value};

use crate::ResolveError;

// ── Schema-child keyword allowlists ──────────────────────────────────────────
//
// These lists mirror the constants in `schemaforge-jsonschema` and must be
// kept in sync with them.  Only keywords whose value is a sub-schema (or a
// collection of sub-schemas) are included.  Non-schema annotations such as
// `default`, `const`, `enum`, `examples`, `title`, and `description` are
// deliberately excluded so that `$anchor` strings inside those values are
// never mistaken for registered anchors.

/// Keywords whose value is a single sub-schema.
const SCHEMA_SINGLE_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "contains",
    "contentSchema",
    "else",
    "if",
    "items",
    "not",
    "propertyNames",
    "then",
    "unevaluatedItems",
    "unevaluatedProperties",
];

/// Keywords whose value is an array of sub-schemas.
const SCHEMA_ARRAY_KEYWORDS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];

/// Keywords whose value is an object mapping names to sub-schemas.
const SCHEMA_MAP_KEYWORDS: &[&str] = &[
    "$defs",
    "definitions",
    "dependentSchemas",
    "patternProperties",
    "properties",
];

/// Apply a URI fragment to a loaded JSON document.
///
/// - Empty fragment → return the document unchanged.
/// - Fragment starting with `/` → follow as a JSON Pointer (RFC 6901).
/// - Any other fragment → scan the document for a sub-schema whose
///   `"$anchor"` property matches the name and return that sub-schema.
pub(crate) fn apply(doc: Value, fragment: &str, uri: &str) -> Result<Value, ResolveError> {
    if fragment.is_empty() {
        return Ok(doc);
    }
    if fragment.starts_with('/') {
        doc.pointer(fragment)
            .cloned()
            .ok_or_else(|| ResolveError::NotFound(uri.to_owned()))
    } else {
        find_anchor_in_value(&doc, fragment).ok_or_else(|| ResolveError::NotFound(uri.to_owned()))
    }
}

/// Recursively scan `val` for the first JSON object whose `"$anchor"` string
/// property equals `name` and return a clone of that object.
///
/// Only descends into schema-valued keyword positions (the same allowlists
/// used by the `schemaforge-jsonschema` construction-time walks).
/// Non-schema annotations such as `default`, `const`, `enum`, and `examples`
/// are intentionally skipped so that an `$anchor` string inside those values
/// is never mistaken for a registered anchor.
fn find_anchor_in_value(val: &Value, name: &str) -> Option<Value> {
    let Value::Object(obj) = val else {
        return None;
    };
    if let Some(Value::String(anchor)) = obj.get("$anchor")
        && anchor == name
    {
        return Some(val.clone());
    }
    find_anchor_in_schema_children(obj, name)
}

/// Recurse into all schema-valued children of `obj`.
fn find_anchor_in_schema_children(obj: &Map<String, Value>, name: &str) -> Option<Value> {
    for &key in SCHEMA_SINGLE_KEYWORDS {
        if let Some(child) = obj.get(key)
            && let Some(v) = find_anchor_in_value(child, name)
        {
            return Some(v);
        }
    }
    for &key in SCHEMA_ARRAY_KEYWORDS {
        if let Some(Value::Array(arr)) = obj.get(key) {
            for child in arr {
                if let Some(v) = find_anchor_in_value(child, name) {
                    return Some(v);
                }
            }
        }
    }
    for &key in SCHEMA_MAP_KEYWORDS {
        if let Some(Value::Object(map)) = obj.get(key) {
            for child in map.values() {
                if let Some(v) = find_anchor_in_value(child, name) {
                    return Some(v);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apply_empty_fragment_returns_doc() {
        let doc = json!({"type": "string"});
        assert_eq!(apply(doc.clone(), "", "u").unwrap(), doc);
    }

    #[test]
    fn apply_pointer_fragment() {
        let doc = json!({"$defs": {"X": {"type": "integer"}}});
        assert_eq!(
            apply(doc, "/$defs/X", "u#/$defs/X").unwrap(),
            json!({"type": "integer"})
        );
    }

    #[test]
    fn apply_anchor_in_array_keyword() {
        // Anchors under allOf (an array) must be found.
        let doc = json!({
            "allOf": [
                {"$anchor": "inAllOf", "type": "string"}
            ]
        });
        let got = apply(doc, "inAllOf", "u#inAllOf").unwrap();
        assert_eq!(got["type"], json!("string"));
        assert_eq!(got["$anchor"], json!("inAllOf"));
    }

    #[test]
    fn apply_missing_anchor_is_not_found() {
        let doc = json!({"type": "object"});
        assert!(matches!(
            apply(doc, "missing", "u#missing"),
            Err(ResolveError::NotFound(_))
        ));
    }

    #[test]
    fn anchor_in_default_is_not_found() {
        // $anchor inside a `default` value is a plain JSON annotation, not a
        // sub-schema position.  It must not be reachable via the anchor scan.
        let doc = json!({
            "default": {"$anchor": "ghost", "some": "value"}
        });
        assert!(
            matches!(
                apply(doc, "ghost", "u#ghost"),
                Err(ResolveError::NotFound(_))
            ),
            "$anchor inside `default` must not be found"
        );
    }

    #[test]
    fn anchor_in_defs_found_when_default_has_same_name() {
        // The $anchor in `$defs` must be returned; the identically-named
        // $anchor in `default` must not shadow it or cause a conflict.
        let doc = json!({
            "$defs": {
                "Str": {"$anchor": "myAnchor", "type": "string"}
            },
            "default": {"$anchor": "myAnchor", "this-is": "not-a-schema"}
        });
        let got = apply(doc, "myAnchor", "u#myAnchor").unwrap();
        assert_eq!(
            got["type"],
            json!("string"),
            "must resolve to the $defs anchor"
        );
        assert!(
            got.get("this-is").is_none(),
            "must not return the object from `default`"
        );
    }

    #[test]
    fn anchor_in_const_is_not_found() {
        // $anchor inside `const` is a literal JSON value, not a sub-schema.
        let doc = json!({"const": {"$anchor": "ghost"}});
        assert!(
            matches!(
                apply(doc, "ghost", "u#ghost"),
                Err(ResolveError::NotFound(_))
            ),
            "$anchor inside `const` must not be found"
        );
    }

    #[test]
    fn anchor_in_enum_is_not_found() {
        // enum values are not sub-schemas; $anchor inside them must be ignored.
        let doc = json!({"enum": [{"$anchor": "ghost"}]});
        assert!(
            matches!(
                apply(doc, "ghost", "u#ghost"),
                Err(ResolveError::NotFound(_))
            ),
            "$anchor inside `enum` must not be found"
        );
    }
}
