//! Unevaluated vocabulary: `unevaluatedProperties` and `unevaluatedItems`.
//!
//! This implementation collects evaluated names/indices from `properties`,
//! `patternProperties`, `additionalProperties`, `prefixItems`, and `items`
//! at the current schema level, as well as property names declared in
//! `allOf`, `anyOf`, and `oneOf` sub-schema `properties` objects.
//!
//! For `allOf` the union of every branch's properties is considered evaluated
//! (all branches must pass).  For `anyOf`/`oneOf` only properties from
//! branches that actually validated the instance successfully are counted.

use std::collections::HashSet;

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
    let explicit = crate::collect_known_property_names(obj);
    let has_additional = obj.contains_key("additionalProperties");
    // Build once — not per instance key — so large objects don't re-walk
    // allOf/anyOf/oneOf branches on every property.
    let branch_props = applicator_branch_evaluated_properties(obj, instance, ctx);
    for (key, value) in inst {
        if is_property_evaluated(key, &explicit, obj, has_additional, &branch_props, ctx) {
            continue;
        }
        let prop_path = format!("{path}/{key}");
        out.merge(validate_schema(up_schema, value, &prop_path, ctx));
    }
}

fn is_property_evaluated(
    key: &str,
    explicit: &HashSet<&str>,
    obj: &Map<String, Value>,
    has_additional: bool,
    branch_props: &HashSet<String>,
    ctx: &ValidationContext<'_>,
) -> bool {
    if explicit.contains(key) {
        return true;
    }
    if has_additional {
        return true;
    }
    if crate::applicator::matches_any_pattern_property(obj, key, ctx) {
        return true;
    }
    // A property listed in a successful allOf/anyOf/oneOf branch's `properties`
    // is considered evaluated by that applicator at this schema level.
    branch_props.contains(key)
}

/// Collect property names that are considered evaluated by each applicator.
///
/// - `allOf`: every branch must pass, so all declared properties from every
///   branch are evaluated regardless of whether the instance has them.
/// - `anyOf` / `oneOf`: only properties from branches that actually validate
///   the current instance successfully are counted as evaluated.
///
/// If a branch contains `$ref` the referenced schema's `properties` are also
/// collected (local fragment references only).
fn applicator_branch_evaluated_properties(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
) -> HashSet<String> {
    let mut evaluated: HashSet<String> = HashSet::new();

    // allOf: union all branches unconditionally (all must pass).
    if let Some(Value::Array(branches)) = obj.get("allOf") {
        for branch in branches {
            collect_branch_props(branch, &mut evaluated, ctx);
        }
    }

    // anyOf/oneOf: only include props from branches the instance actually matches.
    for applicator_key in &["anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = obj.get(*applicator_key) else {
            continue;
        };
        for branch in branches {
            if validate_schema(branch, instance, "", ctx).is_valid() {
                collect_branch_props(branch, &mut evaluated, ctx);
            }
        }
    }

    evaluated
}

/// Maximum depth for recursive `$ref` chain following when collecting branch properties.
const MAX_BRANCH_REF_DEPTH: usize = 8;

/// Collect property names declared in `branch.properties` (and in any schema
/// reached via a local `$ref` chain or `allOf` nesting) into `evaluated`.
fn collect_branch_props(
    branch: &Value,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
) {
    collect_branch_props_depth(branch, evaluated, ctx, 0);
}

/// Recursively collect branch properties up to `MAX_BRANCH_REF_DEPTH`.
fn collect_branch_props_depth(
    branch: &Value,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
    depth: usize,
) {
    if depth > MAX_BRANCH_REF_DEPTH {
        return;
    }
    let Value::Object(obj) = branch else {
        return;
    };
    collect_props_from_obj(obj, evaluated);
    collect_allof_branch_props(obj, evaluated, ctx, depth);
    follow_ref_branch_props(obj, evaluated, ctx, depth);
}

/// Collect property names from every `allOf` sub-schema (all must pass, so all
/// declared properties are unconditionally considered evaluated).
fn collect_allof_branch_props(
    obj: &Map<String, Value>,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
    depth: usize,
) {
    let Some(Value::Array(all_of)) = obj.get("allOf") else {
        return;
    };
    for sub in all_of {
        collect_branch_props_depth(sub, evaluated, ctx, depth + 1);
    }
}

/// Follow a local `$ref` one hop and continue recursive collection.
fn follow_ref_branch_props(
    obj: &Map<String, Value>,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
    depth: usize,
) {
    let Some(Value::String(ref_uri)) = obj.get("$ref") else {
        return;
    };
    let Some(target) = crate::core::resolve_ref(ref_uri, ctx) else {
        return;
    };
    collect_branch_props_depth(target, evaluated, ctx, depth + 1);
}

fn collect_props_from_obj(obj: &Map<String, Value>, evaluated: &mut HashSet<String>) {
    if let Some(Value::Object(props)) = obj.get("properties") {
        for key in props.keys() {
            evaluated.insert(key.clone());
        }
    }
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

    #[test]
    fn unevaluated_properties_allof_branch_properties_are_evaluated() {
        // A property declared in an allOf branch's `properties` must be treated
        // as evaluated and NOT rejected by unevaluatedProperties.
        let s = json!({
            "allOf": [{"properties": {"a": true}}],
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"a": 1})),
            "property covered by allOf branch must not be rejected"
        );
        assert!(
            !valid(&s, &json!({"a": 1, "b": 2})),
            "property not covered by any branch must still be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_anyof_branch_properties_are_evaluated() {
        let s = json!({
            "anyOf": [{"properties": {"x": {"type": "string"}}}],
            "unevaluatedProperties": false
        });
        assert!(valid(&s, &json!({"x": "hello"})));
        assert!(!valid(&s, &json!({"x": "hello", "y": 1})));
    }

    // ── anyOf/oneOf: only successful branches count ───────────────────────────

    #[test]
    fn unevaluated_properties_anyof_failed_branch_props_not_evaluated() {
        // Two anyOf branches: branch 0 requires type=string (will fail for objects),
        // branch 1 declares property "b".  A property "a" that exists ONLY in branch 0
        // must NOT be treated as evaluated when branch 0 fails validation.
        let s = json!({
            "anyOf": [
                // Branch 0: only valid for strings, declares prop "a"
                {"type": "string", "properties": {"a": true}},
                // Branch 1: valid for any object, declares prop "b"
                {"properties": {"b": true}}
            ],
            "unevaluatedProperties": false
        });
        let instance = json!({"a": 1, "b": 2});
        // Branch 0 fails (not a string), so "a" must NOT be considered evaluated.
        // Branch 1 passes, so "b" is evaluated.
        // Therefore "a" is an unevaluated property and must be rejected.
        assert!(
            !valid(&s, &instance),
            "property 'a' from a failing anyOf branch must not be treated as evaluated"
        );
        // Instance with only "b" should pass (branch 1 succeeds).
        assert!(valid(&s, &json!({"b": 2})));
    }

    #[test]
    fn unevaluated_properties_oneof_failed_branch_props_not_evaluated() {
        // oneOf with two branches; only the matching branch's props are evaluated.
        let s = json!({
            "oneOf": [
                {"properties": {"a": true}, "required": ["a"], "additionalProperties": false},
                {"properties": {"b": true}, "required": ["b"], "additionalProperties": false}
            ],
            "unevaluatedProperties": false
        });
        // Instance matches branch 1 only; "b" is evaluated.
        assert!(valid(&s, &json!({"b": 2})));
        // Instance matches branch 0 only; "a" is evaluated.
        assert!(valid(&s, &json!({"a": 1})));
        // Instance matches neither branch; oneOf already fails, unevaluated doesn't matter.
        assert!(!valid(&s, &json!({"c": 3})));
    }

    // ── $ref in allOf branch ──────────────────────────────────────────────────

    #[test]
    fn unevaluated_properties_allof_ref_branch_properties_evaluated() {
        // allOf branch uses $ref to a local definition. Properties from the
        // referenced schema must be considered evaluated.
        let s = json!({
            "$defs": {
                "Named": {"properties": {"name": {"type": "string"}}}
            },
            "allOf": [{"$ref": "#/$defs/Named"}],
            "unevaluatedProperties": false
        });
        // "name" is declared in the $ref target → must be treated as evaluated.
        assert!(
            valid(&s, &json!({"name": "Alice"})),
            "property from $ref target in allOf branch must be treated as evaluated"
        );
        // Unknown extra property must still be rejected.
        assert!(!valid(&s, &json!({"name": "Alice", "extra": 1})));
    }

    #[test]
    fn unevaluated_properties_allof_ref_chain_ab_properties_evaluated() {
        // allOf branch has $ref #/$defs/A; A itself has $ref #/$defs/B; B has
        // properties.  The compiler must follow the chain so "foo" is evaluated.
        let s = json!({
            "$defs": {
                "A": {"$ref": "#/$defs/B"},
                "B": {"properties": {"foo": {"type": "integer"}}}
            },
            "allOf": [{"$ref": "#/$defs/A"}],
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"foo": 1})),
            "property in chain-referenced schema must be treated as evaluated"
        );
        assert!(
            !valid(&s, &json!({"foo": 1, "bar": 2})),
            "property not in any chain must still be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_allof_ref_target_allof_properties_evaluated() {
        // allOf branch has $ref #/$defs/Named; Named has allOf that declares
        // a `name` property.  Properties from the nested allOf must also be
        // treated as evaluated.
        let s = json!({
            "$defs": {
                "Named": {
                    "allOf": [{"properties": {"name": {"type": "string"}}}]
                }
            },
            "allOf": [{"$ref": "#/$defs/Named"}],
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"name": "Alice"})),
            "property inside $ref target's allOf must be treated as evaluated"
        );
        assert!(
            !valid(&s, &json!({"name": "Alice", "extra": 1})),
            "unevaluated extra property must still be rejected"
        );
    }
}
