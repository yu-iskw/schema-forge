//! Unevaluated vocabulary: `unevaluatedProperties` and `unevaluatedItems`.
//!
//! This implementation collects evaluated names/indices from `properties`,
//! `patternProperties`, `additionalProperties`, `prefixItems`, and `items`
//! at the current schema level.

use serde_json::{Map, Value};

use crate::{ValidationContext, ValidationOutput, validate_schema};

/// Apply unevaluated vocabulary keywords.
pub(crate) fn apply(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    apply_unevaluated_properties(obj, instance, path, ctx, out);
    apply_unevaluated_items(obj, instance, path, ctx, out);
}

fn apply_unevaluated_properties(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(up_schema), Value::Object(inst)) = (obj.get("unevaluatedProperties"), instance)
    else {
        return;
    };
    let explicit = collect_explicit_property_names(obj);
    let has_additional = obj.contains_key("additionalProperties");
    for (key, value) in inst {
        if is_property_evaluated(key, &explicit, obj, has_additional, ctx) {
            continue;
        }
        let prop_path = format!("{path}/{key}");
        out.merge(validate_schema(up_schema, value, &prop_path, ctx));
    }
}

fn is_property_evaluated(
    key: &str,
    explicit: &[&str],
    obj: &Map<String, Value>,
    has_additional: bool,
    ctx: &ValidationContext<'_>,
) -> bool {
    if explicit.contains(&key) {
        return true;
    }
    if has_additional {
        return true;
    }
    obj.get("patternProperties")
        .and_then(Value::as_object)
        .is_some_and(|pp| {
            pp.keys().any(|pat| {
                ctx.patterns
                    .get(pat.as_str())
                    .is_some_and(|re| re.is_match(key))
            })
        })
}

fn collect_explicit_property_names(obj: &Map<String, Value>) -> Vec<&str> {
    obj.get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

fn apply_unevaluated_items(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(ui_schema), Value::Array(items)) = (obj.get("unevaluatedItems"), instance) else {
        return;
    };
    let first_unevaluated = first_unevaluated_index(obj, items.len());
    for (i, item) in items.iter().enumerate().skip(first_unevaluated) {
        let item_path = format!("{path}/{i}");
        out.merge(validate_schema(ui_schema, item, &item_path, ctx));
    }
}

fn first_unevaluated_index(obj: &Map<String, Value>, total: usize) -> usize {
    let prefix_len = obj
        .get("prefixItems")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if obj.contains_key("items") {
        total
    } else {
        prefix_len
    }
}

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
    fn unevaluated_properties_blocks_unknown() {
        let s = json!({
            "properties": {"name": {"type": "string"}},
            "unevaluatedProperties": false
        });
        assert!(valid(&s, &json!({"name": "Alice"})));
        assert!(!valid(&s, &json!({"name": "Alice", "extra": 1})));
    }

    #[test]
    fn unevaluated_properties_pattern_evaluated() {
        let s = json!({
            "patternProperties": {"^x-": {}},
            "unevaluatedProperties": false
        });
        assert!(valid(&s, &json!({"x-foo": "bar"})));
        assert!(!valid(&s, &json!({"y-foo": "bar"})));
    }

    #[test]
    fn unevaluated_properties_additional_evaluated() {
        let s = json!({
            "properties": {"name": {"type": "string"}},
            "additionalProperties": {"type": "integer"},
            "unevaluatedProperties": false
        });
        assert!(valid(&s, &json!({"name": "Alice", "age": 30})));
    }

    #[test]
    fn unevaluated_items_prefix_covered() {
        let s = json!({
            "prefixItems": [{"type": "string"}],
            "unevaluatedItems": false
        });
        assert!(valid(&s, &json!(["hello"])));
        assert!(!valid(&s, &json!(["hello", 42])));
    }

    #[test]
    fn unevaluated_items_with_items_all_evaluated() {
        let s = json!({
            "prefixItems": [{"type": "string"}],
            "items": {"type": "integer"},
            "unevaluatedItems": false
        });
        assert!(valid(&s, &json!(["hello", 1, 2])));
    }
}
