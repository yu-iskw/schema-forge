//! OAS 3.0 → JSON Schema 2020-12 schema adaptation.
//!
//! OpenAPI 3.0 schemas use a subset of JSON Schema Draft 4/7 with extensions
//! (`nullable`, `discriminator`, boolean `exclusiveMinimum`/`exclusiveMaximum`).
//! OpenAPI 3.1+ uses JSON Schema 2020-12 directly and needs no adaptation.

use serde_json::Value;

use crate::OpenApiVersion;

/// Adapt an OpenAPI schema to a standalone JSON Schema value.
///
/// OAS 3.1 and 3.2 use JSON Schema 2020-12 directly; no rewriting is needed.
/// OAS 3.0 and Swagger 2.0 use an older dialect subset with extensions that
/// must be normalised to 2020-12 for the compiler.
pub(crate) fn adapt_schema(schema: &Value, version: OpenApiVersion) -> Value {
    match version {
        OpenApiVersion::V31 | OpenApiVersion::V32 => schema.clone(),
        OpenApiVersion::V30 | OpenApiVersion::Swagger20 => adapt_oas30_schema(schema),
    }
}

/// Recursively adapt a single OAS 3.0 / Swagger 2.0 schema object.
///
/// Handles `nullable`, boolean `exclusiveMinimum`/`exclusiveMaximum`, and
/// recurses into all standard sub-schema locations.
pub(crate) fn adapt_oas30_schema(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut new_obj = obj.clone();

    // Handle nullable at this level.
    if new_obj
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        new_obj.remove("nullable");

        // After making null an allowed value, widen `type` so null can pass the
        // type check.  With enum/const, widen only when `type` is already present;
        // otherwise always insert a null-capable type.
        let mut always_widen_type = false;
        if let Some(Value::Array(enum_vals)) = new_obj.get_mut("enum") {
            if !enum_vals.contains(&Value::Null) {
                enum_vals.push(Value::Null);
            }
        } else if let Some(const_val) = new_obj.remove("const") {
            new_obj.insert(
                "enum".to_owned(),
                serde_json::json!([const_val, Value::Null]),
            );
        } else {
            always_widen_type = true;
        }

        if always_widen_type || new_obj.contains_key("type") {
            let type_val = new_obj.get("type").cloned().unwrap_or(Value::Null);
            new_obj.insert("type".to_owned(), make_nullable_type(type_val));
        }
    }

    // Rewrite OAS 3.0 boolean exclusiveMinimum/Maximum to Draft 2020-12 style.
    adapt_exclusive_bound(&mut new_obj, "exclusiveMinimum", "minimum");
    adapt_exclusive_bound(&mut new_obj, "exclusiveMaximum", "maximum");

    // Recurse into all sub-schema locations.
    adapt_map_values(&mut new_obj, "properties");
    adapt_map_values(&mut new_obj, "patternProperties");
    adapt_map_values(&mut new_obj, "$defs");
    adapt_map_values(&mut new_obj, "definitions");
    adapt_map_values(&mut new_obj, "dependentSchemas");
    adapt_single(&mut new_obj, "items");
    adapt_single(&mut new_obj, "additionalProperties");
    adapt_single(&mut new_obj, "not");
    adapt_single(&mut new_obj, "if");
    adapt_single(&mut new_obj, "then");
    adapt_single(&mut new_obj, "else");
    adapt_array_values(&mut new_obj, "prefixItems");
    adapt_array_values(&mut new_obj, "allOf");
    adapt_array_values(&mut new_obj, "anyOf");
    adapt_array_values(&mut new_obj, "oneOf");

    Value::Object(new_obj)
}

/// Rewrite an OAS 3.0 boolean exclusive-bound keyword to Draft 2020-12 style.
///
/// OAS 3.0 uses `exclusiveMinimum: true` to indicate that the adjacent
/// `minimum` value is exclusive.  Draft 2020-12 instead uses a numeric
/// `exclusiveMinimum` directly (the bound value itself).
///
/// Conversion rules:
/// - `exclusiveMinimum: true`  + `minimum: X` → `exclusiveMinimum: X`, remove `minimum`
/// - `exclusiveMinimum: true`  (no `minimum`)  → remove `exclusiveMinimum` (nothing to convert)
/// - `exclusiveMinimum: false`                 → remove `exclusiveMinimum` (not exclusive)
/// - `exclusiveMinimum: <number>`              → leave unchanged (already 2020-12 style)
fn adapt_exclusive_bound(
    obj: &mut serde_json::Map<String, Value>,
    exclusive_key: &str,
    bound_key: &str,
) {
    match obj.get(exclusive_key).and_then(Value::as_bool) {
        Some(true) => {
            if let Some(bound_val) = obj.remove(bound_key) {
                obj.insert(exclusive_key.to_owned(), bound_val);
            } else {
                obj.remove(exclusive_key);
            }
        }
        Some(false) => {
            obj.remove(exclusive_key);
        }
        None => {} // Numeric or absent: leave as-is (already 2020-12 style or not present).
    }
}

/// Recursively adapt every value in an object-typed keyword (e.g. `properties`).
fn adapt_map_values(obj: &mut serde_json::Map<String, Value>, key: &str) {
    if let Some(Value::Object(map)) = obj.get_mut(key) {
        let adapted: serde_json::Map<String, Value> = map
            .iter()
            .map(|(k, v)| (k.clone(), adapt_oas30_schema(v)))
            .collect();
        *map = adapted;
    }
}

/// Recursively adapt a single sub-schema keyword (e.g. `items`, `not`).
fn adapt_single(obj: &mut serde_json::Map<String, Value>, key: &str) {
    if let Some(v) = obj.get(key) {
        let adapted = adapt_oas30_schema(v);
        obj.insert(key.to_owned(), adapted);
    }
}

/// Recursively adapt every element of an array-typed keyword (e.g. `allOf`).
fn adapt_array_values(obj: &mut serde_json::Map<String, Value>, key: &str) {
    if let Some(Value::Array(arr)) = obj.get_mut(key) {
        let adapted: Vec<Value> = arr.iter().map(adapt_oas30_schema).collect();
        *arr = adapted;
    }
}

fn make_nullable_type(existing: Value) -> Value {
    match existing {
        Value::String(t) => serde_json::json!([t, "null"]),
        Value::Array(mut arr) => {
            let null_val = Value::String("null".to_owned());
            if !arr.contains(&null_val) {
                arr.push(null_val);
            }
            Value::Array(arr)
        }
        _ => serde_json::json!([
            "string", "number", "integer", "boolean", "array", "object", "null"
        ]),
    }
}
