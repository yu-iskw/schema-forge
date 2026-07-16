//! Applicator vocabulary: `allOf`, `anyOf`, `oneOf`, `not`, `if/then/else`,
//! `properties`, `patternProperties`, `additionalProperties`, `propertyNames`,
//! `dependentSchemas`, `items`, `prefixItems`, `contains`.

use serde_json::{Map, Value};

use crate::{ValidationContext, ValidationError, ValidationOutput, validate_schema};

/// Apply all applicator keywords.
pub(crate) fn apply(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    apply_all_of(obj, instance, path, ctx, out);
    apply_any_of(obj, instance, path, ctx, out);
    apply_one_of(obj, instance, path, ctx, out);
    apply_not(obj, instance, path, ctx, out);
    apply_if_then_else(obj, instance, path, ctx, out);
    apply_properties(obj, instance, path, ctx, out);
    apply_pattern_properties(obj, instance, path, ctx, out);
    apply_additional_properties(obj, instance, path, ctx, out);
    apply_property_names(obj, instance, path, ctx, out);
    apply_dependent_schemas(obj, instance, path, ctx, out);
    apply_items(obj, instance, path, ctx, out);
    apply_prefix_items(obj, instance, path, ctx, out);
    apply_contains(obj, instance, path, ctx, out);
}

fn apply_all_of(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::Array(schemas)) = obj.get("allOf") else {
        return;
    };
    for s in schemas {
        // Pass the instance path (path) so that errors report where in the
        // instance the failure occurred, not where in the schema.
        out.merge(validate_schema(s, instance, path, ctx));
    }
}

fn apply_any_of(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::Array(schemas)) = obj.get("anyOf") else {
        return;
    };
    let mut passed = false;
    for s in schemas {
        let result = validate_schema(s, instance, path, ctx);
        if result.aborted {
            out.merge(result);
            return;
        }
        if result.is_valid() {
            passed = true;
        }
    }
    if !passed {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/anyOf"),
            "instance does not match any subschema in anyOf",
        )));
    }
}

fn apply_one_of(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::Array(schemas)) = obj.get("oneOf") else {
        return;
    };
    let mut count = 0usize;
    for s in schemas {
        let result = validate_schema(s, instance, path, ctx);
        if result.aborted {
            out.merge(result);
            return;
        }
        if result.is_valid() {
            count += 1;
        }
    }
    if count != 1 {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/oneOf"),
            format!("instance must match exactly one subschema (matched {count})"),
        )));
    }
}

fn apply_not(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(not_schema) = obj.get("not") else {
        return;
    };
    let not_out = validate_schema(not_schema, instance, path, ctx);
    if not_out.aborted {
        // An aborted sub-schema (e.g. unresolved $ref) cannot be inverted;
        // propagate the abort so callers know the result is unreliable.
        out.merge(not_out);
    } else if not_out.is_valid() {
        // The sub-schema passed, so `not` must fail.
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/not"),
            "instance must not match the `not` schema",
        )));
    }
    // Sub-schema failed normally → `not` passes; nothing to add.
}

fn apply_if_then_else(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(if_schema) = obj.get("if") else {
        return;
    };
    let cond_out = validate_schema(if_schema, instance, path, ctx);
    if cond_out.aborted {
        // An aborted condition (e.g. unresolved $ref) makes the branch
        // indeterminate; propagate the abort rather than choosing then/else.
        out.merge(cond_out);
        return;
    }
    if cond_out.is_valid() {
        if let Some(then_schema) = obj.get("then") {
            out.merge(validate_schema(then_schema, instance, path, ctx));
        }
    } else if let Some(else_schema) = obj.get("else") {
        out.merge(validate_schema(else_schema, instance, path, ctx));
    }
}

fn apply_properties(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(Value::Object(props)), Value::Object(inst)) = (obj.get("properties"), instance)
    else {
        return;
    };
    for (key, prop_schema) in props {
        let Some(value) = inst.get(key) else { continue };
        let prop_path = format!("{path}/{key}");
        out.merge(validate_schema(prop_schema, value, &prop_path, ctx));
    }
}

fn apply_pattern_properties(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(Value::Object(pat_props)), Value::Object(inst)) =
        (obj.get("patternProperties"), instance)
    else {
        return;
    };
    for (pattern, schema) in pat_props {
        let Some(re) = ctx.patterns.get(pattern.as_str()) else {
            // Pattern key is present in patternProperties but not compiled —
            // fail-closed: the schema is invalid.
            out.merge(ValidationOutput::fail(ValidationError::new(
                path,
                format!("{path}/patternProperties"),
                format!("patternProperties key `{pattern}` is not a valid regular expression"),
            )));
            continue;
        };
        for (key, value) in inst {
            if re.is_match(key) {
                let prop_path = format!("{path}/{key}");
                out.merge(validate_schema(schema, value, &prop_path, ctx));
            }
        }
    }
}

fn apply_additional_properties(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(ap_schema), Value::Object(inst)) = (obj.get("additionalProperties"), instance) else {
        return;
    };
    let known_props = crate::collect_known_property_names(obj);

    for (key, value) in inst {
        if known_props.contains(key.as_str()) {
            continue;
        }
        if matches_any_pattern_property(obj, key, ctx) {
            continue;
        }
        let prop_path = format!("{path}/{key}");
        out.merge(validate_schema(ap_schema, value, &prop_path, ctx));
    }
}

/// Returns `true` when `key` matches any pattern in `patternProperties`.
pub(crate) fn matches_any_pattern_property(
    obj: &Map<String, Value>,
    key: &str,
    ctx: &ValidationContext<'_>,
) -> bool {
    let Some(Value::Object(pp)) = obj.get("patternProperties") else {
        return false;
    };
    pp.keys().any(|pat| {
        ctx.patterns
            .get(pat.as_str())
            .is_some_and(|re| re.is_match(key))
    })
}

/// `propertyNames` - each key in an object must satisfy the given schema.
fn apply_property_names(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(pn_schema), Value::Object(inst)) = (obj.get("propertyNames"), instance) else {
        return;
    };
    for key in inst.keys() {
        let key_val = Value::String(key.clone());
        let key_path = format!("{path}/{key}");
        out.merge(validate_schema(pn_schema, &key_val, &key_path, ctx));
    }
}

/// `dependentSchemas` - when a trigger property is present, the paired schema
/// must also validate the whole instance.
fn apply_dependent_schemas(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(Value::Object(dep_schemas)), Value::Object(inst)) =
        (obj.get("dependentSchemas"), instance)
    else {
        return;
    };
    for (trigger, dep_schema) in dep_schemas {
        if inst.contains_key(trigger) {
            // Pass the instance path so errors point at the instance location,
            // not at the schema location (dependentSchemas/{trigger}).
            out.merge(validate_schema(dep_schema, instance, path, ctx));
        }
    }
}

fn apply_items(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(item_schema), Value::Array(items)) = (obj.get("items"), instance) else {
        return;
    };
    let prefix_count = prefix_items_count(obj);
    for (i, item) in items.iter().enumerate().skip(prefix_count) {
        let item_path = format!("{path}/{i}");
        out.merge(validate_schema(item_schema, item, &item_path, ctx));
    }
}

fn prefix_items_count(obj: &Map<String, Value>) -> usize {
    obj.get("prefixItems")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn apply_prefix_items(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(Value::Array(prefix)), Value::Array(items)) = (obj.get("prefixItems"), instance)
    else {
        return;
    };
    for (i, (schema, item)) in prefix.iter().zip(items.iter()).enumerate() {
        let item_path = format!("{path}/{i}");
        out.merge(validate_schema(schema, item, &item_path, ctx));
    }
}

fn apply_contains(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let (Some(contains_schema), Value::Array(items)) = (obj.get("contains"), instance) else {
        return;
    };
    let min = obj.get("minContains").and_then(Value::as_u64).unwrap_or(1);
    let max = obj.get("maxContains").and_then(Value::as_u64);

    let mut count = 0u64;
    for item in items {
        let result = validate_schema(contains_schema, item, path, ctx);
        if result.aborted {
            out.merge(result);
            return;
        }
        if result.is_valid() {
            count += 1;
        }
    }

    if count < min {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/contains"),
            format!("array must contain at least {min} matching items (found {count})"),
        )));
    }
    if let Some(m) = max
        && count > m
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/maxContains"),
            format!("array must contain at most {m} matching items (found {count})"),
        )));
    }
}
