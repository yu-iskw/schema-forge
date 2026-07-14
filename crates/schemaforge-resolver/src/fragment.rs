//! URI fragment application for resolved JSON Schema documents.

use serde_json::Value;

use crate::ResolveError;

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
/// Walks both objects and arrays so anchors inside `allOf` / `anyOf` /
/// `oneOf` / `prefixItems` are reachable.  The scan is a plain JSON walk and
/// is not limited to schema-valued positions; callers should ensure the
/// document is a JSON Schema where `$anchor` carries its defined meaning.
fn find_anchor_in_value(val: &Value, name: &str) -> Option<Value> {
    match val {
        Value::Object(obj) => {
            if let Some(Value::String(anchor)) = obj.get("$anchor")
                && anchor == name
            {
                return Some(val.clone());
            }
            obj.values()
                .find_map(|child| find_anchor_in_value(child, name))
        }
        Value::Array(arr) => arr
            .iter()
            .find_map(|child| find_anchor_in_value(child, name)),
        _ => None,
    }
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
}
