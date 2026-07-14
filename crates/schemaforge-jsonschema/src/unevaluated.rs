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

use crate::{ValidationContext, ValidationError, ValidationOutput, validate_schema};

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

/// Fail closed when a branch-ref pre-pass hit [`MAX_BRANCH_REF_DEPTH`].
///
/// Clears and returns `true` when the flag was set so callers abort instead of
/// treating an incomplete evaluated set as authoritative.
fn take_branch_depth_exceeded(
    path: &str,
    keyword: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) -> bool {
    if !ctx.branch_depth_exceeded.get() {
        return false;
    }
    ctx.branch_depth_exceeded.set(false);
    out.merge(ValidationOutput::fail(ValidationError::new(
        path,
        format!("{path}/{keyword}"),
        format!(
            "{keyword} analysis aborted: \
             schema branch depth exceeded limit of {MAX_BRANCH_REF_DEPTH}"
        ),
    )));
    true
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
    // Fresh flag per pass: the Cell is shared across nested validate_schema
    // calls, so a prior depth abort must not poison this walk.
    ctx.branch_depth_exceeded.set(false);
    // Build once — not per instance key — so large objects don't re-walk
    // allOf/anyOf/oneOf branches on every property.
    let branch_props = applicator_branch_evaluated_properties(obj, instance, ctx);
    if take_branch_depth_exceeded(path, "unevaluatedProperties", ctx, out) {
        return;
    }
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

/// Maximum depth for recursive applicator / `$ref` walks when collecting
/// evaluated properties, items, and contains schemas.
///
/// Matches [`crate::MAX_DEPTH`] so that deeply nested schemas that are
/// structurally valid at validation time are also fully analysed during the
/// unevaluated-properties/items pre-pass rather than being silently
/// under-evaluated.
const MAX_BRANCH_REF_DEPTH: usize = crate::MAX_DEPTH as usize;

/// Invoke `f` for each successful `anyOf` / `oneOf` branch.
fn for_each_successful_anyof_oneof<'a>(
    obj: &'a Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
    mut f: impl FnMut(&'a Value),
) {
    for applicator_key in ["anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = obj.get(applicator_key) else {
            continue;
        };
        for branch in branches {
            if validate_schema(branch, instance, "", ctx).is_valid() {
                f(branch);
            }
        }
    }
}

/// Invoke `f` for `if` (always evaluated) and the active `then` / `else` branch.
///
/// Draft 2020-12 §11.2: `if` always runs; `then` applies when it succeeds and
/// `else` when it fails.
fn for_each_if_then_else<'a>(
    obj: &'a Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
    mut f: impl FnMut(&'a Value),
) {
    let Some(if_schema) = obj.get("if") else {
        return;
    };
    f(if_schema);
    let branch_key = if validate_schema(if_schema, instance, "", ctx).is_valid() {
        "then"
    } else {
        "else"
    };
    if let Some(branch) = obj.get(branch_key) {
        f(branch);
    }
}

/// Invoke `f` for each child schema reached via `allOf`, successful
/// `anyOf`/`oneOf`, active `if`/`then`/`else`, or `$ref`.
fn for_each_child_schema<'a>(
    obj: &'a Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'a>,
    mut f: impl FnMut(&'a Value),
) {
    if let Some(Value::Array(branches)) = obj.get("allOf") {
        for branch in branches {
            f(branch);
        }
    }
    for_each_successful_anyof_oneof(obj, instance, ctx, &mut f);
    for_each_if_then_else(obj, instance, ctx, &mut f);
    if let Some(Value::String(ref_uri)) = obj.get("$ref")
        && let Some(target) = crate::core::resolve_ref(ref_uri, ctx)
    {
        f(target);
    }
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
    // Top-level applicators only — local `properties` are handled separately
    // by `is_property_evaluated`.
    for_each_child_schema(obj, instance, ctx, |branch| {
        collect_branch_props_depth(branch, instance, &mut evaluated, ctx, 0);
    });
    collect_dependent_schemas_props(obj, instance, ctx, &mut evaluated, 0);
    evaluated
}

/// Collect property names from `dependentSchemas` entries whose trigger
/// property is present in `instance`.
///
/// `depth` continues the surrounding branch-walk budget so nested
/// `dependentSchemas` cannot reset [`MAX_BRANCH_REF_DEPTH`].
fn collect_dependent_schemas_props(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
    evaluated: &mut HashSet<String>,
    depth: usize,
) {
    let (Some(Value::Object(dep_schemas)), Value::Object(inst)) =
        (obj.get("dependentSchemas"), instance)
    else {
        return;
    };
    for (trigger, dep_schema) in dep_schemas {
        if inst.contains_key(trigger) {
            collect_branch_props_depth(dep_schema, instance, evaluated, ctx, depth);
        }
    }
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
        // Mark the context so callers can fail closed rather than returning
        // an incomplete evaluated set.
        ctx.branch_depth_exceeded.set(true);
        return;
    }
    let Value::Object(obj) = branch else {
        return;
    };
    collect_props_from_obj(obj, instance, evaluated, ctx);
    // Nested `dependentSchemas` (e.g. under allOf / $ref) contribute when triggered.
    collect_dependent_schemas_props(obj, instance, ctx, evaluated, depth + 1);
    for_each_child_schema(obj, instance, ctx, |sub| {
        collect_branch_props_depth(sub, instance, evaluated, ctx, depth + 1);
    });
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

    // Fresh flag per pass (see apply_unevaluated_properties).
    ctx.branch_depth_exceeded.set(false);
    let (effective_prefix_len, has_items) = items_eval_range(obj, instance, ctx);
    // `items` keyword at a reachable depth evaluates every remaining index, so
    // unevaluatedItems is a no-op regardless of depth.
    if has_items {
        ctx.branch_depth_exceeded.set(false);
        return;
    }

    // Collect contains schemas once — not per array element — so large arrays
    // don't re-walk allOf/anyOf/$ref for every index.
    let contains_schemas = collect_contains_schemas(obj, instance, ctx);

    if take_branch_depth_exceeded(path, "unevaluatedItems", ctx, out) {
        return;
    }

    for (i, item) in items.iter().enumerate().skip(effective_prefix_len) {
        // Draft 2020-12 §11.2: items matching `contains` (including those from
        // allOf/anyOf/oneOf/if/then/else/$ref) are evaluated.
        if contains_schemas
            .iter()
            .any(|contains| validate_schema(contains, item, "", ctx).is_valid())
        {
            continue;
        }
        let item_path = format!("{path}/{i}");
        out.merge(validate_schema(ui_schema, item, &item_path, ctx));
    }
}

/// Collect `contains` schemas from `obj` and all reachable sub-schemas.
///
/// Reaches through `allOf` (unconditionally), successful `anyOf`/`oneOf`
/// branches, `if` (always) plus the active `then`/`else` branch, and `$ref`
/// targets — mirroring the reachability rules used for properties.
fn collect_contains_schemas<'a>(
    obj: &'a Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'a>,
) -> Vec<&'a Value> {
    let mut out = Vec::new();
    collect_contains_schemas_depth(obj, instance, ctx, 0, &mut out);
    out
}

fn collect_contains_schemas_depth<'a>(
    obj: &'a Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'a>,
    depth: usize,
    out: &mut Vec<&'a Value>,
) {
    if depth > MAX_BRANCH_REF_DEPTH {
        ctx.branch_depth_exceeded.set(true);
        return;
    }
    if let Some(contains) = obj.get("contains") {
        out.push(contains);
    }
    for_each_child_schema(obj, instance, ctx, |branch| {
        if let Value::Object(b) = branch {
            collect_contains_schemas_depth(b, instance, ctx, depth + 1, out);
        }
    });
}

/// Return `(effective_prefix_len, has_items)` from this schema and reachable
/// applicator / `$ref` sub-schemas (same reachability as `contains`).
fn items_eval_range(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
) -> (usize, bool) {
    let mut max_prefix = 0usize;
    let mut has_items = false;
    collect_items_eval_depth(obj, instance, ctx, 0, &mut max_prefix, &mut has_items);
    (max_prefix, has_items)
}

fn collect_items_eval_depth(
    obj: &Map<String, Value>,
    instance: &Value,
    ctx: &ValidationContext<'_>,
    depth: usize,
    max_prefix: &mut usize,
    has_items: &mut bool,
) {
    if depth > MAX_BRANCH_REF_DEPTH {
        ctx.branch_depth_exceeded.set(true);
        return;
    }
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
    for_each_child_schema(obj, instance, ctx, |branch| {
        if let Value::Object(b) = branch {
            collect_items_eval_depth(b, instance, ctx, depth + 1, max_prefix, has_items);
        }
    });
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

    // ── unevaluatedItems + if/then/else ───────────────────────────────────────

    #[test]
    fn unevaluated_items_if_then_prefix_evaluated_when_if_matches() {
        // When `if` validates, the `then` branch's prefixItems cover the prefix.
        let s = json!({
            "if": {"minItems": 2},
            "then": {"prefixItems": [{"type": "string"}, {"type": "integer"}]},
            "unevaluatedItems": false
        });
        // if matches (2 items) → then applies → indices 0 and 1 are evaluated.
        assert!(
            valid(&s, &json!(["hello", 42])),
            "items covered by then prefixItems must be evaluated when if matches"
        );
        // Index 2 is beyond the then prefix → rejected.
        assert!(
            !valid(&s, &json!(["hello", 42, true])),
            "item beyond then prefixItems must be rejected"
        );
    }

    #[test]
    fn unevaluated_items_if_prefix_always_evaluated() {
        // The `if` schema itself always evaluates; its prefixItems always count.
        let s = json!({
            "if": {"prefixItems": [{"type": "string"}]},
            "unevaluatedItems": false
        });
        // Index 0 covered by if's prefixItems (if always evaluates).
        assert!(
            valid(&s, &json!(["hello"])),
            "item in if prefixItems must always be evaluated"
        );
        // Index 1 is beyond the if prefix → rejected.
        assert!(
            !valid(&s, &json!(["hello", 42])),
            "item beyond if prefixItems must be rejected"
        );
    }

    #[test]
    fn unevaluated_items_else_prefix_evaluated_when_if_fails() {
        // When `if` fails, the `else` branch's prefixItems cover the prefix.
        let s = json!({
            "if": {"minItems": 5},
            "else": {"prefixItems": [{"type": "string"}]},
            "unevaluatedItems": false
        });
        // if fails (only 1 item) → else applies → index 0 is evaluated.
        assert!(
            valid(&s, &json!(["hello"])),
            "item covered by else prefixItems must be evaluated when if fails"
        );
        // Index 1 is unevaluated → rejected.
        assert!(
            !valid(&s, &json!(["hello", 42])),
            "item beyond else prefixItems must be rejected"
        );
    }

    // ── unevaluatedItems + contains from anyOf/oneOf ──────────────────────────

    #[test]
    fn unevaluated_items_anyof_contains_evaluates_matching_items() {
        // `contains` from a successful anyOf branch marks matching items as evaluated.
        let s = json!({
            "anyOf": [
                {"contains": {"type": "string"}},
                {"contains": {"type": "integer"}}
            ],
            "unevaluatedItems": false
        });
        // A string matches the contains in branch 0 (which succeeds for arrays with strings).
        // Since the array ["hello"] validates branch 0, its contains marks "hello" as evaluated.
        assert!(
            valid(&s, &json!(["hello"])),
            "item matching anyOf branch contains must be treated as evaluated"
        );
    }

    #[test]
    fn unevaluated_items_anyof_failed_branch_contains_not_counted() {
        // `contains` from a *failing* anyOf branch must not mark items as evaluated.
        // Branch 0 requires type=string (fails for arrays), branch 1 has no contains.
        let s = json!({
            "anyOf": [
                {"type": "string", "contains": {"type": "integer"}},
                {"type": "array"}
            ],
            "unevaluatedItems": false
        });
        // Branch 0 fails (not a string) → its contains must not mark any item.
        // Branch 1 passes but has no contains.
        // Index 0 is unevaluated → rejected.
        assert!(
            !valid(&s, &json!([42])),
            "contains from a failing anyOf branch must not evaluate items"
        );
    }

    // ── allOf → anyOf → properties ────────────────────────────────────────────

    #[test]
    fn unevaluated_properties_allof_anyof_properties_evaluated() {
        // Properties from a successful anyOf branch nested under an allOf branch
        // must be treated as evaluated.
        let s = json!({
            "allOf": [
                {
                    "anyOf": [
                        {"properties": {"x": {"type": "string"}}},
                        {"properties": {"y": {"type": "integer"}}}
                    ]
                }
            ],
            "unevaluatedProperties": false
        });
        // Instance {"x": "hi"} matches the first anyOf branch → x is evaluated.
        assert!(
            valid(&s, &json!({"x": "hi"})),
            "property from nested anyOf branch must be evaluated"
        );
        // z is not in any branch → rejected.
        assert!(
            !valid(&s, &json!({"z": 1})),
            "property not in any branch must be rejected"
        );
    }

    #[test]
    fn unevaluated_properties_allof_if_then_properties_evaluated() {
        // Properties from if/then/else nested under allOf are evaluated.
        let s = json!({
            "allOf": [
                {
                    "if": {"required": ["kind"]},
                    "then": {"properties": {"kind": {}, "label": {"type": "string"}}}
                }
            ],
            "unevaluatedProperties": false
        });
        // if matches (kind present) → then applies → kind and label are evaluated.
        assert!(
            valid(&s, &json!({"kind": "a", "label": "b"})),
            "properties from nested then branch must be evaluated when if matches"
        );
        assert!(
            !valid(&s, &json!({"kind": "a", "label": "b", "extra": 1})),
            "unevaluated property must still be rejected"
        );
    }

    // ── dependentSchemas under allOf / $ref ───────────────────────────────────

    #[test]
    fn unevaluated_properties_allof_dependent_schemas_props_evaluated() {
        // `dependentSchemas` nested inside an allOf branch must contribute
        // evaluated properties when the trigger key is present in the instance.
        let s = json!({
            "allOf": [
                {
                    "properties": {"credit_card": {"type": "string"}},
                    "dependentSchemas": {
                        "credit_card": {
                            "properties": {"billing_address": {"type": "string"}}
                        }
                    }
                }
            ],
            "unevaluatedProperties": false
        });
        assert!(
            valid(
                &s,
                &json!({"credit_card": "1234", "billing_address": "123 Main"})
            ),
            "billing_address must be evaluated via dependentSchemas inside allOf"
        );
        assert!(
            !valid(
                &s,
                &json!({"credit_card": "1234", "billing_address": "123 Main", "extra": 1})
            ),
            "unevaluated extra property must still be rejected"
        );
        // When the trigger is absent the dependent schema is not activated,
        // so billing_address is not evaluated by it — but credit_card is still
        // covered by the allOf branch's `properties`.
        assert!(
            !valid(&s, &json!({"billing_address": "123 Main"})),
            "billing_address must be rejected when trigger credit_card is absent"
        );
    }

    #[test]
    fn unevaluated_properties_ref_dependent_schemas_props_evaluated() {
        // `dependentSchemas` inside a sibling $ref target must contribute
        // evaluated properties when the trigger key is present.
        let s = json!({
            "$defs": {
                "WithCard": {
                    "properties": {"credit_card": {"type": "string"}},
                    "dependentSchemas": {
                        "credit_card": {
                            "properties": {"billing_address": {"type": "string"}}
                        }
                    }
                }
            },
            "$ref": "#/$defs/WithCard",
            "unevaluatedProperties": false
        });
        assert!(
            valid(
                &s,
                &json!({"credit_card": "1234", "billing_address": "123 Main"})
            ),
            "billing_address must be evaluated via dependentSchemas in $ref target"
        );
        assert!(
            !valid(
                &s,
                &json!({"credit_card": "1234", "billing_address": "123 Main", "extra": 1})
            ),
            "extra property not in any schema must still be rejected"
        );
        assert!(
            !valid(&s, &json!({"billing_address": "123 Main"})),
            "billing_address must be rejected when trigger credit_card is absent"
        );
    }

    // ── depth fail-closed ─────────────────────────────────────────────────────

    #[test]
    fn unevaluated_properties_deep_allof_chain_fails_closed_with_depth_error() {
        // Build an allOf chain nested beyond MAX_BRANCH_REF_DEPTH (128) levels.
        // Property "x" at the leaf is too deep for the branch-property pre-pass
        // to reach; the validator must fail closed with a depth error rather than
        // silently treating "x" as unevaluated and wrongly rejecting the instance.
        let leaf = serde_json::json!({"properties": {"x": true}});
        let mut schema = leaf;
        // 133 > MAX_BRANCH_REF_DEPTH (128)
        for _ in 0..133_usize {
            schema = serde_json::json!({"allOf": [schema]});
        }
        let full_schema = serde_json::json!({
            "allOf": [schema],
            "unevaluatedProperties": false
        });
        let v = Validator::new(&full_schema, ValidationOptions::default()).unwrap();
        let out = v.validate(&serde_json::json!({"x": 1}));
        assert!(
            !out.is_valid(),
            "deep allOf with unevaluatedProperties must fail-closed on depth exceeded"
        );
        assert!(
            out.errors.iter().any(|e| e.message.contains("depth")),
            "at least one error must mention 'depth', got: {:#?}",
            out.errors
        );
    }

    #[test]
    fn unevaluated_items_deep_allof_chain_fails_closed_with_depth_error() {
        // Same depth-exceeded scenario for unevaluatedItems: an allOf chain
        // deeper than MAX_BRANCH_REF_DEPTH must produce a depth error.
        let leaf = serde_json::json!({"prefixItems": [{"type": "integer"}]});
        let mut schema = leaf;
        for _ in 0..133_usize {
            schema = serde_json::json!({"allOf": [schema]});
        }
        let full_schema = serde_json::json!({
            "allOf": [schema],
            "unevaluatedItems": false
        });
        let v = Validator::new(&full_schema, ValidationOptions::default()).unwrap();
        let out = v.validate(&serde_json::json!([1, 2]));
        assert!(
            !out.is_valid(),
            "deep allOf with unevaluatedItems must fail-closed on depth exceeded"
        );
        assert!(
            out.errors.iter().any(|e| e.message.contains("depth")),
            "at least one error must mention 'depth', got: {:#?}",
            out.errors
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
