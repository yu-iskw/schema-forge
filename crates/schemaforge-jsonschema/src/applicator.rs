//! Applicator vocabulary: `allOf`, `anyOf`, `oneOf`, `not`, `if/then/else`,
//! `properties`, `patternProperties`, `additionalProperties`, `items`,
//! `prefixItems`, `contains`.

use regex::Regex;
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
    for (i, s) in schemas.iter().enumerate() {
        let kpath = format!("{path}/allOf/{i}");
        out.merge(validate_schema(s, instance, &kpath, ctx));
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
    let passed = schemas
        .iter()
        .any(|s| validate_schema(s, instance, path, ctx).is_valid());
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
    let count = schemas
        .iter()
        .filter(|s| validate_schema(s, instance, path, ctx).is_valid())
        .count();
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
    if validate_schema(not_schema, instance, path, ctx).is_valid() {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/not"),
            "instance must not match the `not` schema",
        )));
    }
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
    let condition_met = validate_schema(if_schema, instance, path, ctx).is_valid();
    if condition_met {
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
        let Ok(re) = Regex::new(pattern) else {
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
    let known_props = collect_known_property_names(obj);
    let pattern_props = collect_pattern_property_regexes(obj);

    for (key, value) in inst {
        if known_props.contains(&key.as_str()) {
            continue;
        }
        if pattern_props.iter().any(|re| re.is_match(key)) {
            continue;
        }
        let prop_path = format!("{path}/{key}");
        out.merge(validate_schema(ap_schema, value, &prop_path, ctx));
    }
}

fn collect_known_property_names(obj: &Map<String, Value>) -> Vec<&str> {
    obj.get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

fn collect_pattern_property_regexes(obj: &Map<String, Value>) -> Vec<Regex> {
    obj.get("patternProperties")
        .and_then(Value::as_object)
        .map(|pp| pp.keys().filter_map(|k| Regex::new(k).ok()).collect())
        .unwrap_or_default()
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

    let count = items
        .iter()
        .filter(|item| validate_schema(contains_schema, item, path, ctx).is_valid())
        .count() as u64;

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
