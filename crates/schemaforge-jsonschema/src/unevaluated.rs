//! Unevaluated vocabulary: `unevaluatedProperties` and `unevaluatedItems`.
//!
//! This implementation collects evaluated names/indices from `properties`,
//! `patternProperties`, `additionalProperties`, `prefixItems`, `items`, and
//! `contains` at the current schema level, as well as property names declared
//! in `allOf`, `anyOf`, `oneOf`, `if`/`then`/`else`, `dependentSchemas`,
//! and sibling `$ref` sub-schemas.
//!
//! For `allOf` the union of every branch's properties is considered evaluated
//! (all branches must pass).  For `anyOf`/`oneOf` only properties from
//! branches that actually validated the instance successfully are counted.
//! For `if`/`then`/`else`, properties from the matching branch are included.
//! For `dependentSchemas`, properties from each triggered dependent schema
//! are included when its trigger property is present in the instance.
//!
//! A top-level sibling `$ref` is treated the same as an `allOf` branch ref:
//! its declared `properties`, `patternProperties`, and `additionalProperties`
//! are resolved and contribute to the evaluated set.
//!
//! Branch `additionalProperties` (when not `false`) marks all instance keys as
//! evaluated for that branch.  Branch `patternProperties` marks matching keys.

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
    // (or covered by a branch's additionalProperties/patternProperties) is
    // considered evaluated by that applicator at this schema level.
    branch_props.contains(key)
}

/// Collect property names that are considered evaluated by each applicator.
///
/// - `allOf`: every branch must pass, so all declared properties from every
///   branch are evaluated regardless of whether the instance has them.
/// - `anyOf` / `oneOf`: only properties from branches that actually validate
///   the current instance successfully are counted as evaluated.
/// - `if`/`then`/`else`: properties from the branch that matches the condition.
/// - `dependentSchemas`: properties from each triggered schema when its trigger
///   key is present in the instance.
/// - sibling `$ref`: treated like an additional allOf branch — properties from
///   the referenced schema are unconditionally evaluated.
///
/// If a branch contains `$ref` the referenced schema's `properties` are also
/// collected (local fragment references only).
fn applicator_branch_evaluated_properties(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
) -> HashSet<String> {
    let mut evaluated: HashSet<String> = HashSet::new();

    // Top-level sibling $ref: resolve and collect as if it were an allOf branch.
    if let Some(Value::String(ref_uri)) = obj.get("$ref")
        && let Some(target) = crate::core::resolve_ref(ref_uri, ctx)
    {
        collect_branch_props_depth(target, instance, &mut evaluated, ctx, 0);
    }

    // allOf: union all branches unconditionally (all must pass).
    if let Some(Value::Array(branches)) = obj.get("allOf") {
        for branch in branches {
            collect_branch_props(branch, instance, &mut evaluated, ctx);
        }
    }

    // anyOf/oneOf: only include props from branches the instance actually matches.
    for applicator_key in &["anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = obj.get(*applicator_key) else {
            continue;
        };
        for branch in branches {
            if validate_schema(branch, instance, "", ctx).is_valid() {
                collect_branch_props(branch, instance, &mut evaluated, ctx);
            }
        }
    }

    // if/then/else: include properties from the active branch.
    collect_if_then_else_props(obj, instance, ctx, &mut evaluated);

    // dependentSchemas: include properties when the trigger key is present.
    collect_dependent_schemas_props(obj, instance, ctx, &mut evaluated);

    evaluated
}

/// Collect property names from `if`, and from `then` when `if` validates or
/// from `else` when `if` fails.
///
/// The `if` schema always runs against the instance, so its declared
/// `properties` are always considered evaluated (Draft 2020-12 §11.2).
fn collect_if_then_else_props(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
    evaluated: &mut HashSet<String>,
) {
    let Some(if_schema) = obj.get("if") else {
        return;
    };
    // `if` always evaluates, regardless of outcome.
    collect_branch_props(if_schema, instance, evaluated, ctx);

    let condition_met = validate_schema(if_schema, instance, "", ctx).is_valid();
    let branch_key = if condition_met { "then" } else { "else" };
    if let Some(branch) = obj.get(branch_key) {
        collect_branch_props(branch, instance, evaluated, ctx);
    }
}

/// Collect property names from `dependentSchemas` entries whose trigger
/// property is present in `instance`.
fn collect_dependent_schemas_props(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
    evaluated: &mut HashSet<String>,
) {
    let (Some(Value::Object(dep_schemas)), Value::Object(inst)) =
        (obj.get("dependentSchemas"), instance)
    else {
        return;
    };
    for (trigger, dep_schema) in dep_schemas {
        if inst.contains_key(trigger) {
            collect_branch_props(dep_schema, instance, evaluated, ctx);
        }
    }
}

/// Maximum depth for recursive `$ref` chain following when collecting branch properties.
const MAX_BRANCH_REF_DEPTH: usize = 8;

/// Collect property names declared in `branch.properties` (and in any schema
/// reached via a local `$ref` chain or `allOf` nesting) into `evaluated`.
///
/// Also accounts for `additionalProperties` (not `false`) and
/// `patternProperties` in the branch.
fn collect_branch_props(
    branch: &Value,
    instance: &Value,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
) {
    collect_branch_props_depth(branch, instance, evaluated, ctx, 0);
}

/// Recursively collect branch properties up to `MAX_BRANCH_REF_DEPTH`.
fn collect_branch_props_depth(
    branch: &Value,
    instance: &Value,
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
    collect_props_from_obj(obj, instance, evaluated, ctx);
    collect_allof_branch_props(obj, instance, evaluated, ctx, depth);
    follow_ref_branch_props(obj, instance, evaluated, ctx, depth);
}

/// Collect property names from every `allOf` sub-schema (all must pass, so all
/// declared properties are unconditionally considered evaluated).
fn collect_allof_branch_props(
    obj: &Map<String, Value>,
    instance: &Value,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
    depth: usize,
) {
    let Some(Value::Array(all_of)) = obj.get("allOf") else {
        return;
    };
    for sub in all_of {
        collect_branch_props_depth(sub, instance, evaluated, ctx, depth + 1);
    }
}

/// Follow a local `$ref` one hop and continue recursive collection.
fn follow_ref_branch_props(
    obj: &Map<String, Value>,
    instance: &Value,
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
    collect_branch_props_depth(target, instance, evaluated, ctx, depth + 1);
}

/// Collect property names from `obj` into `evaluated`.
///
/// - Explicit `properties` keys are always collected.
/// - If `additionalProperties` is present and not `false`, all instance object
///   keys are marked as evaluated (they are validated by `additionalProperties`
///   for instance keys not in `properties`/`patternProperties`).
/// - `patternProperties` patterns are matched against instance keys; matching
///   keys are marked as evaluated.
fn collect_props_from_obj(
    obj: &Map<String, Value>,
    instance: &Value,
    evaluated: &mut HashSet<String>,
    ctx: &ValidationContext<'_>,
) {
    if let Some(Value::Object(props)) = obj.get("properties") {
        for key in props.keys() {
            evaluated.insert(key.clone());
        }
    }
    let Value::Object(inst) = instance else {
        return;
    };
    // additionalProperties (anything except `false`) evaluates the remaining
    // instance keys not covered by properties/patternProperties.  Marking all
    // instance keys is safe because keys already in `properties` are idempotent.
    if let Some(ap) = obj.get("additionalProperties")
        && ap != &Value::Bool(false)
    {
        for key in inst.keys() {
            evaluated.insert(key.clone());
        }
    }
    // patternProperties evaluates every instance key that matches any pattern.
    if obj.contains_key("patternProperties") {
        for key in inst.keys() {
            if crate::applicator::matches_any_pattern_property(obj, key, ctx) {
                evaluated.insert(key.clone());
            }
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

    let (effective_prefix_len, has_items) = items_eval_range(obj, instance, ctx);
    // `items` evaluates every remaining index, so unevaluatedItems is a no-op.
    if has_items {
        return;
    }

    for (i, item) in items.iter().enumerate().skip(effective_prefix_len) {
        // Draft 2020-12 §11.2: items matching `contains` (including those from
        // allOf/$ref targets) are evaluated.
        if item_matches_any_contains(obj, item, ctx, 0) {
            continue;
        }
        let item_path = format!("{path}/{i}");
        out.merge(validate_schema(ui_schema, item, &item_path, ctx));
    }
}

/// Return `true` when `item` matches a `contains` schema from `obj`, any of
/// its `allOf` branches, or through a `$ref` chain.
fn item_matches_any_contains(
    obj: &Map<String, Value>,
    item: &Value,
    ctx: &ValidationContext<'_>,
    depth: usize,
) -> bool {
    if depth > MAX_BRANCH_REF_DEPTH {
        return false;
    }
    if let Some(contains) = obj.get("contains")
        && validate_schema(contains, item, "", ctx).is_valid()
    {
        return true;
    }
    if let Some(Value::Array(branches)) = obj.get("allOf") {
        for branch in branches {
            if let Value::Object(b) = branch
                && item_matches_any_contains(b, item, ctx, depth + 1)
            {
                return true;
            }
        }
    }
    if let Some(Value::String(ref_uri)) = obj.get("$ref")
        && let Some(Value::Object(target)) = crate::core::resolve_ref(ref_uri, ctx)
        && item_matches_any_contains(target, item, ctx, depth + 1)
    {
        return true;
    }
    false
}

/// Return `(effective_prefix_len, has_items)` by merging the direct schema
/// keywords with any `items`/`prefixItems` found inside `allOf`/`anyOf`/`oneOf`
/// branches, a sibling `$ref`, or through `$ref` chains (analogous to property
/// collection for `unevaluatedProperties`).
///
/// For `allOf` all branches contribute unconditionally.  For `anyOf`/`oneOf`
/// only branches that actually validate the instance contribute.  A sibling
/// `$ref` is treated like an extra allOf branch.
fn items_eval_range(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
) -> (usize, bool) {
    let direct_prefix = obj
        .get("prefixItems")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let direct_items = obj.contains_key("items");

    let mut branch_prefix = 0usize;
    let mut branch_items = false;

    // allOf: all branches must pass — include unconditionally.
    if let Some(Value::Array(branches)) = obj.get("allOf") {
        for branch in branches {
            collect_branch_items_eval(branch, &mut branch_prefix, &mut branch_items, ctx, 0);
        }
    }

    // anyOf/oneOf: only include from branches the instance actually validates.
    for applicator_key in &["anyOf", "oneOf"] {
        if let Some(Value::Array(branches)) = obj.get(*applicator_key) {
            for branch in branches {
                if validate_schema(branch, instance, "", ctx).is_valid() {
                    collect_branch_items_eval(
                        branch,
                        &mut branch_prefix,
                        &mut branch_items,
                        ctx,
                        0,
                    );
                }
            }
        }
    }

    // Top-level sibling $ref: treated like an additional allOf branch.
    if let Some(Value::String(ref_uri)) = obj.get("$ref")
        && let Some(target) = crate::core::resolve_ref(ref_uri, ctx)
    {
        collect_branch_items_eval(target, &mut branch_prefix, &mut branch_items, ctx, 0);
    }

    (
        direct_prefix.max(branch_prefix),
        direct_items || branch_items,
    )
}

/// Recursively collect `prefixItems` length and `items` presence from a branch
/// schema, following `$ref` and nested `allOf`, up to `MAX_BRANCH_REF_DEPTH`.
fn collect_branch_items_eval(
    branch: &Value,
    max_prefix: &mut usize,
    has_items: &mut bool,
    ctx: &ValidationContext<'_>,
    depth: usize,
) {
    if depth > MAX_BRANCH_REF_DEPTH {
        return;
    }
    let Value::Object(obj) = branch else {
        return;
    };
    if obj.contains_key("items") {
        *has_items = true;
    }
    if let Some(prefix_len) = obj
        .get("prefixItems")
        .and_then(Value::as_array)
        .map(Vec::len)
    {
        *max_prefix = (*max_prefix).max(prefix_len);
    }
    // Follow $ref one hop.
    if let Some(Value::String(ref_uri)) = obj.get("$ref")
        && let Some(target) = crate::core::resolve_ref(ref_uri, ctx)
    {
        collect_branch_items_eval(target, max_prefix, has_items, ctx, depth + 1);
    }
    // Recurse into nested allOf (all branches must pass).
    if let Some(Value::Array(all_of)) = obj.get("allOf") {
        for sub in all_of {
            collect_branch_items_eval(sub, max_prefix, has_items, ctx, depth + 1);
        }
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

    // ── Sibling $ref + unevaluatedProperties ─────────────────────────────────

    #[test]
    fn unevaluated_properties_sibling_ref_properties_evaluated() {
        // When a schema has a top-level `$ref` alongside `unevaluatedProperties`,
        // properties declared in the `$ref` target must be considered evaluated.
        let s = json!({
            "$defs": {
                "Base": {"properties": {"id": {"type": "integer"}}}
            },
            "$ref": "#/$defs/Base",
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"id": 1})),
            "property from sibling $ref target must be treated as evaluated"
        );
        assert!(
            !valid(&s, &json!({"id": 1, "extra": "x"})),
            "unevaluated extra property must still be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_sibling_ref_with_allof_evaluated() {
        // Sibling $ref combined with allOf: both contribute evaluated properties.
        let s = json!({
            "$defs": {
                "Base": {"properties": {"id": {"type": "integer"}}},
                "Named": {"properties": {"name": {"type": "string"}}}
            },
            "$ref": "#/$defs/Base",
            "allOf": [{"$ref": "#/$defs/Named"}],
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"id": 1, "name": "Alice"})),
            "properties from both sibling $ref and allOf must be evaluated"
        );
        assert!(
            !valid(&s, &json!({"id": 1, "name": "Alice", "x": 0})),
            "any unevaluated key must still be rejected"
        );
    }

    // ── Branch additionalProperties evaluates all instance keys ───────────────

    #[test]
    fn unevaluated_properties_branch_additional_properties_evaluates_all_keys() {
        // When a successful allOf branch has additionalProperties (not false),
        // all instance keys are considered evaluated by that branch.
        let s = json!({
            "allOf": [{"additionalProperties": {"type": "integer"}}],
            "unevaluatedProperties": false
        });
        // All keys are evaluated by the branch's additionalProperties.
        assert!(
            valid(&s, &json!({"a": 1, "b": 2})),
            "keys evaluated by branch additionalProperties must not be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_branch_pattern_properties_evaluates_matching_keys() {
        // When a successful allOf branch has patternProperties, instance keys
        // matching the pattern are considered evaluated.
        let s = json!({
            "allOf": [{"patternProperties": {"^x-": {"type": "string"}}}],
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"x-foo": "bar"})),
            "key matching branch patternProperties must be considered evaluated"
        );
        assert!(
            !valid(&s, &json!({"y-foo": "bar"})),
            "key not matching branch pattern must still be rejected"
        );
    }

    // ── unevaluatedItems + contains ───────────────────────────────────────────

    #[test]
    fn unevaluated_items_contains_evaluates_matching_items() {
        // When contains is present, items matching the contains schema are
        // considered evaluated and must not be rejected by unevaluatedItems:false.
        let s = json!({
            "contains": {"type": "string"},
            "unevaluatedItems": false
        });
        // A string item matches contains → evaluated, not rejected.
        assert!(
            valid(&s, &json!(["hello"])),
            "item matching contains must be treated as evaluated"
        );
        // A number does not match contains → unevaluated, rejected.
        assert!(
            !valid(&s, &json!(["hello", 42])),
            "item not matching contains must be rejected by unevaluatedItems"
        );
    }

    #[test]
    fn unevaluated_items_allof_contains_evaluates_matching_items() {
        // `contains` from an allOf branch must also mark matching items as evaluated.
        let s = json!({
            "allOf": [{"contains": {"type": "string"}}],
            "unevaluatedItems": false
        });
        assert!(
            valid(&s, &json!(["hello"])),
            "item matching allOf branch contains must be treated as evaluated"
        );
        assert!(
            !valid(&s, &json!(["hello", 42])),
            "item not matching any contains must be rejected"
        );
    }

    #[test]
    fn unevaluated_items_allof_items_evaluates_all() {
        // An allOf branch with `items` means all items after any prefixItems
        // are evaluated by that branch; unevaluatedItems should not flag them.
        let s = json!({
            "allOf": [{"items": {"type": "integer"}}],
            "unevaluatedItems": false
        });
        assert!(
            valid(&s, &json!([1, 2, 3])),
            "items evaluated by allOf branch items keyword must not be rejected"
        );
        // The allOf branch still validates — integers only.
        assert!(
            !valid(&s, &json!(["not-int"])),
            "allOf items validation still applies"
        );
    }

    #[test]
    fn unevaluated_items_allof_prefix_items_evaluates_prefix() {
        // An allOf branch with prefixItems evaluates the covered prefix.
        let s = json!({
            "allOf": [{"prefixItems": [{"type": "string"}]}],
            "unevaluatedItems": false
        });
        // Index 0 covered by allOf prefixItems → evaluated.
        assert!(
            valid(&s, &json!(["hello"])),
            "item in allOf prefixItems range must not be rejected"
        );
        // Index 1 is beyond allOf prefixItems and there's no items → rejected.
        assert!(
            !valid(&s, &json!(["hello", 42])),
            "item beyond allOf prefixItems range must still be rejected"
        );
    }

    // ── Sibling $ref + unevaluatedItems ──────────────────────────────────────

    #[test]
    fn unevaluated_items_sibling_ref_prefix_items_evaluated() {
        // A top-level $ref alongside unevaluatedItems; prefixItems in the $ref
        // target must mark the covered prefix as evaluated.
        let s = json!({
            "$defs": {
                "TwoStrings": {"prefixItems": [{"type": "string"}, {"type": "string"}]}
            },
            "$ref": "#/$defs/TwoStrings",
            "unevaluatedItems": false
        });
        // Indices 0 and 1 covered by $ref target's prefixItems → evaluated.
        assert!(
            valid(&s, &json!(["a", "b"])),
            "items in sibling $ref target prefixItems must be evaluated"
        );
        // Index 2 is beyond the prefix → rejected.
        assert!(
            !valid(&s, &json!(["a", "b", "c"])),
            "item beyond sibling $ref target prefix must be rejected"
        );
    }

    #[test]
    fn unevaluated_items_sibling_ref_items_evaluates_all() {
        // A top-level $ref with `items` in the target means all items are
        // evaluated by that branch.
        let s = json!({
            "$defs": {
                "IntList": {"items": {"type": "integer"}}
            },
            "$ref": "#/$defs/IntList",
            "unevaluatedItems": false
        });
        assert!(
            valid(&s, &json!([1, 2, 3])),
            "items evaluated by sibling $ref target items must not be rejected"
        );
    }

    // ── unevaluatedItems anyOf/oneOf ──────────────────────────────────────────

    #[test]
    fn unevaluated_items_anyof_failed_branch_prefix_not_counted() {
        // anyOf branch 0 requires type=string (will fail for arrays),
        // branch 1 has a prefixItems covering index 0.
        // Only branch 1 succeeds so only its prefix should count.
        let s = json!({
            "anyOf": [
                // Branch 0: valid for strings only, has 3-item prefixItems
                {"type": "string", "prefixItems": [true, true, true]},
                // Branch 1: valid for anything, covers index 0
                {"prefixItems": [{"type": "integer"}]}
            ],
            "unevaluatedItems": false
        });
        let instance = json!([1, 99]);
        // Branch 0 fails (not a string), so its prefixItems (covering 3 items)
        // must NOT count.  Branch 1 succeeds and covers index 0 only.
        // Index 1 is unevaluated → rejected.
        assert!(
            !valid(&s, &instance),
            "item beyond successful-branch prefix must be rejected"
        );
        // Instance with only index 0 should pass.
        assert!(valid(&s, &json!([1])));
    }

    // ── unevaluatedProperties + if/then/else ──────────────────────────────────

    #[test]
    fn unevaluated_properties_then_branch_evaluated_when_if_matches() {
        // When `if` validates, properties declared in `then` are considered
        // evaluated and must not be rejected by unevaluatedProperties.
        let s = json!({
            "if": {"properties": {"type": {"const": "user"}}, "required": ["type"]},
            "then": {"properties": {"name": {"type": "string"}}},
            "unevaluatedProperties": false
        });
        assert!(
            valid(&s, &json!({"type": "user", "name": "Alice"})),
            "properties from then must be evaluated when if matches"
        );
        assert!(
            !valid(&s, &json!({"type": "user", "name": "Alice", "extra": 1})),
            "extra property not in then must still be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_else_branch_evaluated_when_if_fails() {
        // When `if` fails, properties from `else` are evaluated.
        let s = json!({
            "if": {"properties": {"kind": {"const": "admin"}}, "required": ["kind"]},
            "else": {"properties": {"role": {"type": "string"}}},
            "unevaluatedProperties": false
        });
        // `if` fails (kind absent) → else applies → `role` is evaluated.
        assert!(
            valid(&s, &json!({"role": "editor"})),
            "property from else must be evaluated when if fails"
        );
        assert!(
            !valid(&s, &json!({"role": "editor", "extra": 1})),
            "extra property not in else must still be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_dependent_schemas_props_evaluated_when_triggered() {
        // Properties declared inside a triggered dependentSchemas entry must be
        // considered evaluated.  The trigger key itself must appear in the
        // top-level `properties` so it is independently evaluated.
        let s = json!({
            "properties": {
                "credit_card": {"type": "string"}
            },
            "dependentSchemas": {
                "credit_card": {
                    "properties": {"billing_address": {"type": "string"}}
                }
            },
            "unevaluatedProperties": false
        });
        // `credit_card` is present (evaluated via top-level properties) →
        // its dependentSchema is triggered → `billing_address` is evaluated.
        assert!(
            valid(
                &s,
                &json!({"credit_card": "1234", "billing_address": "123 Main"})
            ),
            "billing_address must be evaluated via triggered dependentSchema"
        );
        // Without the trigger key, billing_address is NOT evaluated.
        assert!(
            !valid(&s, &json!({"billing_address": "123 Main"})),
            "billing_address must be rejected when trigger is absent"
        );
    }
}
