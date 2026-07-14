//! Validation vocabulary: `type`, `enum`, `const`, numeric/string/array/object
//! constraints, `required`, `dependentRequired`, and `format`.

use serde_json::{Map, Value};

use crate::{ValidationContext, ValidationError, ValidationOutput};

/// Apply all validation vocabulary keywords.
pub(crate) fn apply(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    apply_type(obj, instance, path, out);
    apply_enum(obj, instance, path, out);
    apply_const(obj, instance, path, out);
    apply_string_constraints(obj, instance, path, ctx, out);
    apply_numeric_constraints(obj, instance, path, out);
    apply_array_constraints(obj, instance, path, out);
    apply_object_constraints(obj, instance, path, out);
}

fn apply_type(obj: &Map<String, Value>, instance: &Value, path: &str, out: &mut ValidationOutput) {
    let Some(type_val) = obj.get("type") else {
        return;
    };
    let allowed = collect_types(type_val);
    if !instance_matches_types(instance, &allowed) {
        let actual = json_type_name(instance);
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/type"),
            format!("expected type(s) {allowed:?}, got `{actual}`"),
        )));
    }
}

fn collect_types(v: &Value) -> Vec<&str> {
    match v {
        Value::String(s) => vec![s.as_str()],
        Value::Array(arr) => arr.iter().filter_map(|x| x.as_str()).collect(),
        _ => Vec::new(),
    }
}

fn instance_matches_types(instance: &Value, allowed: &[&str]) -> bool {
    allowed.iter().any(|&t| instance_matches_type(instance, t))
}

fn instance_matches_type(instance: &Value, t: &str) -> bool {
    match t {
        "null" => instance.is_null(),
        "boolean" => instance.is_boolean(),
        "integer" => is_integer(instance),
        "number" => instance.is_number(),
        "string" => instance.is_string(),
        "array" => instance.is_array(),
        "object" => instance.is_object(),
        _ => false,
    }
}

fn is_integer(v: &Value) -> bool {
    match v {
        Value::Number(n) => {
            n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        _ => false,
    }
}

fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn apply_enum(obj: &Map<String, Value>, instance: &Value, path: &str, out: &mut ValidationOutput) {
    let Some(Value::Array(variants)) = obj.get("enum") else {
        return;
    };
    if !variants.contains(instance) {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/enum"),
            "instance is not one of the allowed enum values",
        )));
    }
}

fn apply_const(obj: &Map<String, Value>, instance: &Value, path: &str, out: &mut ValidationOutput) {
    let Some(const_val) = obj.get("const") else {
        return;
    };
    if instance != const_val {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/const"),
            format!("instance must be equal to the const value {const_val}"),
        )));
    }
}

fn apply_string_constraints(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Value::String(s) = instance else { return };
    let char_count = s.chars().count() as u64;

    if let Some(min) = obj.get("minLength").and_then(Value::as_u64)
        && char_count < min
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/minLength"),
            format!("string length {char_count} < minLength {min}"),
        )));
    }
    if let Some(max) = obj.get("maxLength").and_then(Value::as_u64)
        && char_count > max
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/maxLength"),
            format!("string length {char_count} > maxLength {max}"),
        )));
    }
    apply_pattern(obj, s, path, ctx, out);
    apply_format(obj, s, path, ctx, out);
}

fn apply_pattern(
    obj: &Map<String, Value>,
    s: &str,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::String(pattern)) = obj.get("pattern") else {
        return;
    };
    let Some(re) = ctx.patterns.get(pattern.as_str()) else {
        // Pattern keyword is present but the regex is absent from the compiled
        // cache — treat as a validation failure (fail-closed).
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/pattern"),
            format!("pattern `{pattern}` is not a valid regular expression"),
        )));
        return;
    };
    if !re.is_match(s) {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/pattern"),
            format!("string does not match pattern `{pattern}`"),
        )));
    }
}

fn apply_format(
    obj: &Map<String, Value>,
    s: &str,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::String(format)) = obj.get("format") else {
        return;
    };
    let result = ctx.formats.validate(format, s);
    if !result.is_ok() {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/format"),
            format!("format `{format}` validation failed"),
        )));
    }
}

fn apply_numeric_constraints(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut ValidationOutput,
) {
    let Some(n) = instance.as_f64() else { return };

    check_minimum(obj, n, path, out);
    check_maximum(obj, n, path, out);
    check_multiple_of(obj, n, path, out);
}

fn check_minimum(obj: &Map<String, Value>, n: f64, path: &str, out: &mut ValidationOutput) {
    // Draft 4 / OAS 3.0: `exclusiveMinimum: true` promotes `minimum` to an
    // exclusive bound.  Draft 2020-12 uses a numeric `exclusiveMinimum` directly.
    let excl_bool = obj
        .get("exclusiveMinimum")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(min) = obj.get("minimum").and_then(Value::as_f64)
        && ((excl_bool && n <= min) || (!excl_bool && n < min))
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/minimum"),
            format!("{n} < minimum {min}"),
        )));
    }
    if let Some(emin) = obj.get("exclusiveMinimum").and_then(Value::as_f64)
        && n <= emin
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/exclusiveMinimum"),
            format!("{n} <= exclusiveMinimum {emin}"),
        )));
    }
}

fn check_maximum(obj: &Map<String, Value>, n: f64, path: &str, out: &mut ValidationOutput) {
    // Draft 4 / OAS 3.0: `exclusiveMaximum: true` promotes `maximum` to an
    // exclusive bound.  Draft 2020-12 uses a numeric `exclusiveMaximum` directly.
    let excl_bool = obj
        .get("exclusiveMaximum")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if let Some(max) = obj.get("maximum").and_then(Value::as_f64)
        && ((excl_bool && n >= max) || (!excl_bool && n > max))
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/maximum"),
            format!("{n} > maximum {max}"),
        )));
    }
    if let Some(emax) = obj.get("exclusiveMaximum").and_then(Value::as_f64)
        && n >= emax
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/exclusiveMaximum"),
            format!("{n} >= exclusiveMaximum {emax}"),
        )));
    }
}

fn check_multiple_of(obj: &Map<String, Value>, n: f64, path: &str, out: &mut ValidationOutput) {
    let Some(m) = obj.get("multipleOf").and_then(Value::as_f64) else {
        return;
    };
    if m <= 0.0 {
        return;
    }
    let remainder = (n / m).fract().abs();
    let epsilon = 1e-10;
    if remainder > epsilon && (1.0 - remainder) > epsilon {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/multipleOf"),
            format!("{n} is not a multiple of {m}"),
        )));
    }
}

fn apply_array_constraints(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut ValidationOutput,
) {
    let Value::Array(arr) = instance else { return };
    let len = arr.len() as u64;

    if let Some(min) = obj.get("minItems").and_then(Value::as_u64)
        && len < min
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/minItems"),
            format!("array length {len} < minItems {min}"),
        )));
    }
    if let Some(max) = obj.get("maxItems").and_then(Value::as_u64)
        && len > max
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/maxItems"),
            format!("array length {len} > maxItems {max}"),
        )));
    }
    check_unique_items(obj, arr, path, out);
}

fn check_unique_items(
    obj: &Map<String, Value>,
    arr: &[Value],
    path: &str,
    out: &mut ValidationOutput,
) {
    if !obj
        .get("uniqueItems")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return;
    }
    let has_dups = (0..arr.len()).any(|i| (i + 1..arr.len()).any(|j| arr[i] == arr[j]));
    if has_dups {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/uniqueItems"),
            "array items must be unique",
        )));
    }
}

fn apply_object_constraints(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    out: &mut ValidationOutput,
) {
    let Value::Object(inst) = instance else {
        return;
    };
    let count = inst.len() as u64;

    if let Some(min) = obj.get("minProperties").and_then(Value::as_u64)
        && count < min
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/minProperties"),
            format!("object has {count} properties, need at least {min}"),
        )));
    }
    if let Some(max) = obj.get("maxProperties").and_then(Value::as_u64)
        && count > max
    {
        out.merge(ValidationOutput::fail(ValidationError::new(
            path,
            format!("{path}/maxProperties"),
            format!("object has {count} properties, max is {max}"),
        )));
    }
    apply_required(obj, inst, path, out);
    apply_dependent_required(obj, inst, path, out);
}

fn apply_required(
    obj: &Map<String, Value>,
    inst: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut ValidationOutput,
) {
    let Some(Value::Array(required)) = obj.get("required") else {
        return;
    };
    for req in required {
        let Some(key) = req.as_str() else { continue };
        if !inst.contains_key(key) {
            out.merge(ValidationOutput::fail(ValidationError::new(
                path,
                format!("{path}/required"),
                format!("required property `{key}` is missing"),
            )));
        }
    }
}

fn apply_dependent_required(
    obj: &Map<String, Value>,
    inst: &serde_json::Map<String, Value>,
    path: &str,
    out: &mut ValidationOutput,
) {
    let Some(Value::Object(dep_req)) = obj.get("dependentRequired") else {
        return;
    };
    for (prop, deps) in dep_req {
        if !inst.contains_key(prop) {
            continue;
        }
        let Some(Value::Array(required)) = Some(deps) else {
            continue;
        };
        for req in required {
            let Some(key) = req.as_str() else { continue };
            if !inst.contains_key(key) {
                out.merge(ValidationOutput::fail(ValidationError::new(
                    path,
                    format!("{path}/dependentRequired/{prop}"),
                    format!("property `{key}` is required when `{prop}` is present"),
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ValidationOptions, Validator};
    use serde_json::json;

    fn valid(schema: &serde_json::Value, instance: &serde_json::Value) -> bool {
        Validator::new(schema, ValidationOptions::default())
            .unwrap()
            .validate(instance)
            .is_valid()
    }

    // ── exclusiveMinimum / exclusiveMaximum boolean (Draft 4 / OAS 3.0) ────────

    #[test]
    fn exclusive_minimum_bool_true_rejects_equal_value() {
        // exclusiveMinimum: true makes `minimum` exclusive: n == minimum must fail.
        let schema = json!({"minimum": 5.0, "exclusiveMinimum": true});
        assert!(
            !valid(&schema, &json!(5.0)),
            "value equal to exclusive minimum must be rejected"
        );
        assert!(
            valid(&schema, &json!(5.001)),
            "value strictly above exclusive minimum must be accepted"
        );
        assert!(
            !valid(&schema, &json!(4.999)),
            "value below exclusive minimum must be rejected"
        );
    }

    #[test]
    fn exclusive_maximum_bool_true_rejects_equal_value() {
        // exclusiveMaximum: true makes `maximum` exclusive: n == maximum must fail.
        let schema = json!({"maximum": 10.0, "exclusiveMaximum": true});
        assert!(
            !valid(&schema, &json!(10.0)),
            "value equal to exclusive maximum must be rejected"
        );
        assert!(
            valid(&schema, &json!(9.999)),
            "value strictly below exclusive maximum must be accepted"
        );
        assert!(
            !valid(&schema, &json!(10.001)),
            "value above exclusive maximum must be rejected"
        );
    }

    #[test]
    fn exclusive_minimum_bool_false_inclusive_bound() {
        // exclusiveMinimum: false means minimum is inclusive; equal value must pass.
        let schema = json!({"minimum": 3.0, "exclusiveMinimum": false});
        assert!(
            valid(&schema, &json!(3.0)),
            "value equal to inclusive minimum must be accepted"
        );
        assert!(!valid(&schema, &json!(2.999)));
    }

    #[test]
    fn exclusive_maximum_bool_false_inclusive_bound() {
        let schema = json!({"maximum": 7.0, "exclusiveMaximum": false});
        assert!(
            valid(&schema, &json!(7.0)),
            "value equal to inclusive maximum must be accepted"
        );
        assert!(!valid(&schema, &json!(7.001)));
    }

    #[test]
    fn unknown_type_string_rejects_all_instances() {
        // An unrecognised `type` value must fail-closed: no instance should
        // satisfy the constraint, because the type name is not in the known
        // set and we cannot claim any instance matches an unknown type.
        let schema = json!({"type": "notatype"});
        assert!(
            !valid(&schema, &json!("hello")),
            "string should not satisfy unknown type `notatype`"
        );
        assert!(
            !valid(&schema, &json!(42)),
            "integer should not satisfy unknown type `notatype`"
        );
        assert!(
            !valid(&schema, &json!(null)),
            "null should not satisfy unknown type `notatype`"
        );
        assert!(
            !valid(&schema, &json!({})),
            "object should not satisfy unknown type `notatype`"
        );
    }

    #[test]
    fn unknown_type_in_array_is_ignored_but_known_types_still_match() {
        // When `type` is an array, only instances that match at least one
        // recognised type should pass.  An unknown entry must not make the
        // constraint accept everything.
        let schema = json!({"type": ["string", "notatype"]});
        assert!(valid(&schema, &json!("hello")), "string should match");
        assert!(
            !valid(&schema, &json!(42)),
            "integer must not satisfy [string, notatype]"
        );
    }

    #[test]
    fn known_types_still_validate_correctly() {
        for (type_str, good, bad) in [
            ("null", json!(null), json!("x")),
            ("boolean", json!(true), json!(1)),
            ("integer", json!(1), json!(1.5)),
            ("number", json!(1.5), json!("x")),
            ("string", json!("x"), json!(1)),
            ("array", json!([]), json!({})),
            ("object", json!({}), json!([])),
        ] {
            let schema = json!({"type": type_str});
            assert!(
                valid(&schema, &good),
                "type={type_str} should accept {good}"
            );
            assert!(!valid(&schema, &bad), "type={type_str} should reject {bad}");
        }
    }
}
