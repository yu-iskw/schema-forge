//! OpenAPI 3.x document parsing and JSON Schema extraction for Schemaforge.
//!
//! Supports parsing OpenAPI 3.0, 3.1, and 3.2 documents in JSON or YAML
//! format, extracting the component schemas as JSON Schema values for further
//! compilation.
//!
//! Swagger 2.0 documents are detected and normalised to a simplified
//! OpenAPI-like form (definitions → components/schemas, body parameters →
//! requestBody schemas).  A provenance warning is attached to every such
//! document because the conversion is lossy.

use indexmap::IndexMap;
use schemaforge_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Error returned when parsing an OpenAPI document fails.
#[derive(Debug, Error)]
pub enum OpenApiError {
    /// The document JSON is malformed.
    #[error("JSON parse error: {0}")]
    JsonParse(String),
    /// The document YAML is malformed.
    #[error("YAML parse error: {0}")]
    YamlParse(String),
    /// The document is not a recognised OpenAPI version.
    #[error("unsupported OpenAPI version: {0}")]
    UnsupportedVersion(String),
    /// A required field is missing from the document.
    #[error("missing required field `{0}` in OpenAPI document")]
    MissingField(String),
}

/// OpenAPI version detected from the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenApiVersion {
    /// OpenAPI 3.0.x
    V30,
    /// OpenAPI 3.1.x
    V31,
    /// OpenAPI 3.2.x
    V32,
    /// Swagger 2.0 (normalised)
    Swagger20,
}

impl std::fmt::Display for OpenApiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V30 => f.write_str("3.0"),
            Self::V31 => f.write_str("3.1"),
            Self::V32 => f.write_str("3.2"),
            Self::Swagger20 => f.write_str("2.0 (swagger)"),
        }
    }
}

/// A schema reference extracted from an OpenAPI document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaEntry {
    /// The schema name (key in `components/schemas`).
    pub name: String,
    /// The raw JSON Schema value.
    pub schema: Value,
    /// The fully-qualified JSON Pointer path within the document.
    pub pointer: String,
}

/// A parsed OpenAPI document.
#[derive(Debug)]
pub struct OpenApiDoc {
    /// The detected OpenAPI version.
    pub version: OpenApiVersion,
    /// Non-fatal diagnostics accumulated during parsing/normalisation.
    pub diagnostics: Vec<Diagnostic>,
    /// Raw document value (after normalisation for Swagger 2.0).
    raw: Value,
}

impl OpenApiDoc {
    /// Parse an OpenAPI document from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`OpenApiError`] when the JSON is malformed or the version is
    /// unsupported.
    pub fn from_json(json: &str) -> Result<Self, OpenApiError> {
        let raw: Value =
            serde_json::from_str(json).map_err(|e| OpenApiError::JsonParse(e.to_string()))?;
        Self::from_value(raw)
    }

    /// Parse an OpenAPI document from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns [`OpenApiError`] when the YAML is malformed or the version is
    /// unsupported.
    pub fn from_yaml(yaml: &str) -> Result<Self, OpenApiError> {
        let raw: Value =
            serde_saphyr::from_str(yaml).map_err(|e| OpenApiError::YamlParse(e.to_string()))?;
        Self::from_value(raw)
    }

    fn from_value(raw: Value) -> Result<Self, OpenApiError> {
        let version = detect_version(&raw)?;
        let mut diagnostics = Vec::new();
        let normalised = normalise(raw, version, &mut diagnostics);
        Ok(Self {
            version,
            diagnostics,
            raw: normalised,
        })
    }

    /// Extract all component schemas from the document.
    ///
    /// Returns an ordered map of schema name to [`SchemaEntry`].
    #[must_use]
    pub fn component_schemas(&self) -> IndexMap<String, SchemaEntry> {
        extract_component_schemas(&self.raw, self.version)
    }

    /// Extract path item request/response schemas.
    #[must_use]
    pub fn path_schemas(&self) -> Vec<SchemaEntry> {
        extract_path_schemas(&self.raw, self.version)
    }
}

// ── Version detection ─────────────────────────────────────────────────────────

fn detect_version(doc: &Value) -> Result<OpenApiVersion, OpenApiError> {
    if let Some(v) = doc.get("swagger").and_then(Value::as_str) {
        return detect_swagger_version(v);
    }
    let version_str = doc
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenApiError::MissingField("openapi".to_owned()))?;
    parse_openapi_version(version_str)
}

fn detect_swagger_version(v: &str) -> Result<OpenApiVersion, OpenApiError> {
    if v.starts_with("2.0") {
        Ok(OpenApiVersion::Swagger20)
    } else {
        Err(OpenApiError::UnsupportedVersion(format!("swagger {v}")))
    }
}

fn parse_openapi_version(s: &str) -> Result<OpenApiVersion, OpenApiError> {
    if s.starts_with("3.2") {
        Ok(OpenApiVersion::V32)
    } else if s.starts_with("3.1") {
        Ok(OpenApiVersion::V31)
    } else if s.starts_with("3.0") {
        Ok(OpenApiVersion::V30)
    } else {
        Err(OpenApiError::UnsupportedVersion(s.to_owned()))
    }
}

// ── Normalisation ─────────────────────────────────────────────────────────────

fn normalise(raw: Value, version: OpenApiVersion, diagnostics: &mut Vec<Diagnostic>) -> Value {
    if version == OpenApiVersion::Swagger20 {
        normalise_swagger(raw, diagnostics)
    } else {
        raw
    }
}

fn normalise_swagger(raw: Value, diagnostics: &mut Vec<Diagnostic>) -> Value {
    diagnostics.push(Diagnostic::warning(
        "Swagger 2.0 document detected; converting to simplified OpenAPI form. \
         The conversion is lossy: some Swagger-specific features are ignored.",
    ));
    let Value::Object(mut obj) = raw else {
        return Value::Object(serde_json::Map::new());
    };
    lift_definitions(&mut obj, diagnostics);
    lift_body_parameters(&mut obj, diagnostics);
    Value::Object(obj)
}

fn lift_definitions(obj: &mut serde_json::Map<String, Value>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(defs) = obj.remove("definitions") else {
        return;
    };
    let components = obj
        .entry("components".to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Value::Object(comp) = components {
        comp.insert("schemas".to_owned(), defs);
    }
    diagnostics.push(Diagnostic::info(
        "Swagger `definitions` moved to `components/schemas`.",
    ));
}

fn lift_body_parameters(
    obj: &mut serde_json::Map<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(Value::Object(paths)) = obj.get_mut("paths") else {
        return;
    };
    let mut converted = 0usize;
    for path_item in paths.values_mut() {
        converted += lift_body_params_in_path(path_item);
    }
    if converted > 0 {
        diagnostics.push(Diagnostic::info(format!(
            "Converted {converted} Swagger body parameter(s) to requestBody schemas.",
        )));
    }
}

fn lift_body_params_in_path(path_item: &mut Value) -> usize {
    let Value::Object(methods) = path_item else {
        return 0;
    };
    let mut count = 0;
    for operation in methods.values_mut() {
        count += lift_body_from_operation(operation);
    }
    count
}

fn lift_body_from_operation(operation: &mut Value) -> usize {
    let Value::Object(op) = operation else {
        return 0;
    };
    let Some(Value::Array(params)) = op.get_mut("parameters") else {
        return 0;
    };
    let mut body_schema: Option<Value> = None;
    params.retain(|p| {
        let is_body = p.get("in").and_then(Value::as_str) == Some("body");
        if is_body {
            body_schema = p.get("schema").cloned();
        }
        !is_body
    });
    body_schema.map_or(0, |schema| {
        let rb = serde_json::json!({
            "content": { "application/json": { "schema": schema } }
        });
        op.insert("requestBody".to_owned(), rb);
        1
    })
}

// ── Schema extraction ─────────────────────────────────────────────────────────

fn extract_component_schemas(
    doc: &Value,
    version: OpenApiVersion,
) -> IndexMap<String, SchemaEntry> {
    let mut result = IndexMap::new();
    let Some(Value::Object(schemas)) = doc.get("components").and_then(|c| c.get("schemas")) else {
        return result;
    };
    for (name, schema) in schemas {
        let schema = adapt_schema(schema, version);
        let pointer = format!("#/components/schemas/{name}");
        result.insert(
            name.clone(),
            SchemaEntry {
                name: name.clone(),
                schema,
                pointer,
            },
        );
    }
    result
}

fn extract_path_schemas(doc: &Value, version: OpenApiVersion) -> Vec<SchemaEntry> {
    let mut result = Vec::new();
    let Some(Value::Object(paths)) = doc.get("paths") else {
        return result;
    };
    for (path, path_item) in paths {
        let Some(Value::Object(methods)) = Some(path_item) else {
            continue;
        };
        for (method, operation) in methods {
            collect_operation_schemas(operation, path, method, version, &mut result);
        }
    }
    result
}

fn collect_operation_schemas(
    operation: &Value,
    path: &str,
    method: &str,
    version: OpenApiVersion,
    result: &mut Vec<SchemaEntry>,
) {
    collect_request_body_schemas(operation, path, method, version, result);
    collect_response_schemas(operation, path, method, version, result);
}

fn collect_request_body_schemas(
    operation: &Value,
    path: &str,
    method: &str,
    version: OpenApiVersion,
    result: &mut Vec<SchemaEntry>,
) {
    let Some(Value::Object(content)) = operation
        .get("requestBody")
        .and_then(|rb| rb.get("content"))
    else {
        return;
    };
    for (media_type, media_obj) in content {
        if let Some(schema) = media_obj.get("schema") {
            let name = format!("{method}_{}_request_{media_type}", path.trim_matches('/'));
            result.push(SchemaEntry {
                pointer: format!("#/paths/{path}/{method}/requestBody/content/{media_type}/schema"),
                schema: adapt_schema(schema, version),
                name,
            });
        }
    }
}

fn collect_response_schemas(
    operation: &Value,
    path: &str,
    method: &str,
    version: OpenApiVersion,
    result: &mut Vec<SchemaEntry>,
) {
    let Some(Value::Object(responses)) = operation.get("responses") else {
        return;
    };
    for (status, response) in responses {
        if let Some(Value::Object(content)) = response.get("content") {
            for (media_type, media_obj) in content {
                if let Some(schema) = media_obj.get("schema") {
                    let name = format!(
                        "{method}_{}_response_{status}_{media_type}",
                        path.trim_matches('/')
                    );
                    result.push(SchemaEntry {
                        pointer: format!(
                            "#/paths/{path}/{method}/responses/{status}/content/{media_type}/schema"
                        ),
                        schema: adapt_schema(schema, version),
                        name,
                    });
                }
            }
        }
    }
}

// ── Schema adaptation ─────────────────────────────────────────────────────────

/// Adapt an OpenAPI schema to a standalone JSON Schema.
///
/// OpenAPI 3.0 schemas use a subset of JSON Schema Draft 4/7 with some
/// extensions (`nullable`, `discriminator`). OpenAPI 3.1 and 3.2 use JSON
/// Schema 2020-12 directly. Swagger 2.0 schemas are treated like 3.0.
fn adapt_schema(schema: &Value, version: OpenApiVersion) -> Value {
    match version {
        OpenApiVersion::V31 | OpenApiVersion::V32 => schema.clone(),
        OpenApiVersion::V30 | OpenApiVersion::Swagger20 => adapt_oas30_schema(schema),
    }
}

fn adapt_oas30_schema(schema: &Value) -> Value {
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

        if let Some(Value::Array(enum_vals)) = new_obj.get_mut("enum") {
            // enum present: add null to the allowed values so the schema accepts null.
            if !enum_vals.contains(&Value::Null) {
                enum_vals.push(Value::Null);
            }
        } else if let Some(const_val) = new_obj.remove("const") {
            // const present: convert to enum [const_val, null] so null is permitted.
            new_obj.insert(
                "enum".to_owned(),
                serde_json::json!([const_val, Value::Null]),
            );
        } else {
            // No enum/const: widen the type to include null.
            let type_val = new_obj.get("type").cloned().unwrap_or(Value::Null);
            new_obj.insert("type".to_owned(), make_nullable_type(type_val));
        }
    }

    // Rewrite OAS 3.0 boolean exclusiveMinimum/Maximum to Draft 2020-12 style.
    adapt_exclusive_bound(&mut new_obj, "exclusiveMinimum", "minimum");
    adapt_exclusive_bound(&mut new_obj, "exclusiveMaximum", "maximum");

    // Recurse into all sub-schema locations.
    adapt_map_values(&mut new_obj, "properties");
    adapt_map_values(&mut new_obj, "$defs");
    adapt_map_values(&mut new_obj, "definitions");
    adapt_single(&mut new_obj, "items");
    adapt_single(&mut new_obj, "additionalProperties");
    adapt_single(&mut new_obj, "not");
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_diagnostics::Severity;
    use serde_json::json;

    const SIMPLE_OPENAPI_31: &str = r#"{
        "openapi": "3.1.0",
        "info": {"title": "Test", "version": "1.0.0"},
        "paths": {},
        "components": {
            "schemas": {
                "User": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "age":  {"type": "integer"}
                    },
                    "required": ["name"]
                }
            }
        }
    }"#;

    #[test]
    fn parse_openapi_3_1() {
        let doc = OpenApiDoc::from_json(SIMPLE_OPENAPI_31).unwrap();
        assert_eq!(doc.version, OpenApiVersion::V31);
    }

    #[test]
    fn parse_openapi_3_2() {
        let json = r#"{
            "openapi": "3.2.0",
            "info": {"title": "Test", "version": "1.0.0"},
            "paths": {},
            "components": {"schemas": {"Item": {"type": "string"}}}
        }"#;
        let doc = OpenApiDoc::from_json(json).unwrap();
        assert_eq!(doc.version, OpenApiVersion::V32);
    }

    #[test]
    fn parse_openapi_3_2_display() {
        assert_eq!(OpenApiVersion::V32.to_string(), "3.2");
    }

    #[test]
    fn extract_component_schemas() {
        let doc = OpenApiDoc::from_json(SIMPLE_OPENAPI_31).unwrap();
        let schemas = doc.component_schemas();
        assert!(schemas.contains_key("User"));
        let user = &schemas["User"];
        assert_eq!(user.name, "User");
        assert!(user.pointer.contains("/components/schemas/User"));
    }

    #[test]
    fn unsupported_version_error() {
        let bad = r#"{"openapi": "4.0", "info": {}, "paths": {}}"#;
        let result = OpenApiDoc::from_json(bad);
        assert!(matches!(result, Err(OpenApiError::UnsupportedVersion(_))));
    }

    // ── nullable conformance fixtures ─────────────────────────────────────────

    #[test]
    fn adapt_oas30_nullable_string() {
        let schema = json!({"type": "string", "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        let types = adapted["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
        assert!(types.contains(&json!("string")));
        assert!(
            adapted.get("nullable").is_none(),
            "nullable must be removed"
        );
    }

    #[test]
    fn adapt_oas30_nullable_array_type() {
        let schema = json!({"type": ["string", "integer"], "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        let types = adapted["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
        assert!(types.contains(&json!("string")));
        assert!(types.contains(&json!("integer")));
    }

    #[test]
    fn adapt_oas30_nullable_no_type() {
        let schema = json!({"nullable": true, "description": "anything"});
        let adapted = adapt_oas30_schema(&schema);
        let types = adapted["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
    }

    #[test]
    fn adapt_oas30_not_nullable_passthrough() {
        let schema = json!({"type": "string"});
        let adapted = adapt_oas30_schema(&schema);
        assert_eq!(adapted["type"], json!("string"));
    }

    #[test]
    fn adapt_oas30_nullable_recursive_properties() {
        // A nullable field nested inside `properties` must also be adapted.
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "nullable": true},
                "count": {"type": "integer"}
            }
        });
        let adapted = adapt_oas30_schema(&schema);
        let name_types = adapted["properties"]["name"]["type"].as_array().unwrap();
        assert!(
            name_types.contains(&json!("null")),
            "nested nullable property must have null in type array"
        );
        assert!(
            name_types.contains(&json!("string")),
            "nested property must retain original type"
        );
        assert!(
            adapted["properties"]["name"].get("nullable").is_none(),
            "nullable key must be removed from nested property"
        );
        // Non-nullable sibling must be untouched.
        assert_eq!(adapted["properties"]["count"]["type"], json!("integer"));
    }

    #[test]
    fn adapt_oas30_nullable_with_enum_adds_null_to_enum() {
        // nullable: true + enum must add null to the enum array, not touch type.
        let schema = json!({"enum": ["foo", "bar"], "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        let vals = adapted["enum"].as_array().unwrap();
        assert!(vals.contains(&json!("foo")));
        assert!(vals.contains(&json!("bar")));
        assert!(vals.contains(&Value::Null), "null must be in enum");
        assert!(adapted.get("nullable").is_none());
        // type should NOT be present (only enum controls valid values).
        assert!(adapted.get("type").is_none());
    }

    #[test]
    fn adapt_oas30_nullable_with_enum_does_not_duplicate_null() {
        // If null is already in the enum, it must not be duplicated.
        let schema = json!({"enum": ["foo", null], "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        let vals = adapted["enum"].as_array().unwrap();
        let null_count = vals.iter().filter(|v| v.is_null()).count();
        assert_eq!(null_count, 1, "null must appear exactly once in enum");
    }

    #[test]
    fn adapt_oas30_nullable_with_const_converts_to_enum() {
        // nullable: true + const must become enum [const_val, null].
        let schema = json!({"const": "active", "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        assert!(adapted.get("const").is_none(), "const must be removed");
        let vals = adapted["enum"].as_array().unwrap();
        assert!(vals.contains(&json!("active")));
        assert!(vals.contains(&Value::Null));
    }

    #[test]
    fn adapt_oas30_nullable_with_const_integer() {
        let schema = json!({"const": 42, "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        let vals = adapted["enum"].as_array().unwrap();
        assert!(vals.contains(&json!(42)));
        assert!(vals.contains(&Value::Null));
    }

    #[test]
    fn adapt_oas30_nullable_recursive_allof() {
        let schema = json!({
            "allOf": [
                {"type": "string", "nullable": true},
                {"type": "integer"}
            ]
        });
        let adapted = adapt_oas30_schema(&schema);
        let first = &adapted["allOf"][0];
        let types = first["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
        assert!(types.contains(&json!("string")));
        assert!(first.get("nullable").is_none());
    }

    // ── Swagger 2.0 fixtures ──────────────────────────────────────────────────

    const SWAGGER_20: &str = r#"{
        "swagger": "2.0",
        "info": {"title": "Petstore", "version": "1.0"},
        "paths": {
            "/pets": {
                "post": {
                    "parameters": [
                        {
                            "name": "body",
                            "in": "body",
                            "schema": {"type": "object", "properties": {"name": {"type": "string"}}}
                        }
                    ],
                    "responses": {"200": {"description": "OK"}}
                }
            }
        },
        "definitions": {
            "Pet": {"type": "object", "properties": {"name": {"type": "string"}}}
        }
    }"#;

    #[test]
    fn swagger_version_detected() {
        let doc = OpenApiDoc::from_json(SWAGGER_20).unwrap();
        assert_eq!(doc.version, OpenApiVersion::Swagger20);
    }

    #[test]
    fn swagger_provenance_warning() {
        let doc = OpenApiDoc::from_json(SWAGGER_20).unwrap();
        let has_warning = doc
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("Swagger 2.0"));
        assert!(has_warning, "expected Swagger provenance warning");
    }

    #[test]
    fn swagger_definitions_lifted_to_components() {
        let doc = OpenApiDoc::from_json(SWAGGER_20).unwrap();
        let schemas = doc.component_schemas();
        assert!(
            schemas.contains_key("Pet"),
            "Pet should be in components/schemas"
        );
    }

    #[test]
    fn swagger_body_param_converted_to_request_body() {
        let doc = OpenApiDoc::from_json(SWAGGER_20).unwrap();
        let path_schemas = doc.path_schemas();
        assert!(
            !path_schemas.is_empty(),
            "body parameter should yield a path schema entry"
        );
        let has_request = path_schemas.iter().any(|s| s.name.contains("request"));
        assert!(has_request, "expected a requestBody-derived schema entry");
    }

    #[test]
    fn swagger_definitions_info_diagnostic() {
        let doc = OpenApiDoc::from_json(SWAGGER_20).unwrap();
        let has_info = doc
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Info && d.message.contains("definitions"));
        assert!(has_info, "expected definitions-lift info diagnostic");
    }

    #[test]
    fn swagger_nullable_schema_adapted() {
        let swagger = r#"{
            "swagger": "2.0",
            "info": {"title": "T", "version": "1"},
            "paths": {},
            "definitions": {
                "MaybeNull": {"type": "string", "nullable": true}
            }
        }"#;
        let doc = OpenApiDoc::from_json(swagger).unwrap();
        let schemas = doc.component_schemas();
        let s = &schemas["MaybeNull"].schema;
        let types = s["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
        assert!(types.contains(&json!("string")));
    }

    #[test]
    fn openapi_30_nullable_conformance() {
        let oas30 = r#"{
            "openapi": "3.0.3",
            "info": {"title": "T", "version": "1"},
            "paths": {},
            "components": {
                "schemas": {
                    "MaybeNull": {"type": "string", "nullable": true}
                }
            }
        }"#;
        let doc = OpenApiDoc::from_json(oas30).unwrap();
        let schemas = doc.component_schemas();
        let s = &schemas["MaybeNull"].schema;
        let types = s["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
        assert!(types.contains(&json!("string")));
    }

    // ── exclusiveMinimum / exclusiveMaximum boolean rewrite ───────────────────

    #[test]
    fn adapt_oas30_exclusive_minimum_true_with_minimum() {
        // OAS 3.0: exclusiveMinimum: true + minimum: 5 → exclusiveMinimum: 5
        let schema = json!({"minimum": 5.0, "exclusiveMinimum": true});
        let adapted = adapt_oas30_schema(&schema);
        assert_eq!(
            adapted["exclusiveMinimum"],
            json!(5.0),
            "exclusiveMinimum must carry the minimum value"
        );
        assert!(
            adapted.get("minimum").is_none(),
            "minimum must be removed after rewrite"
        );
    }

    #[test]
    fn adapt_oas30_exclusive_maximum_true_with_maximum() {
        // OAS 3.0: exclusiveMaximum: true + maximum: 10 → exclusiveMaximum: 10
        let schema = json!({"maximum": 10.0, "exclusiveMaximum": true});
        let adapted = adapt_oas30_schema(&schema);
        assert_eq!(
            adapted["exclusiveMaximum"],
            json!(10.0),
            "exclusiveMaximum must carry the maximum value"
        );
        assert!(
            adapted.get("maximum").is_none(),
            "maximum must be removed after rewrite"
        );
    }

    #[test]
    fn adapt_oas30_exclusive_minimum_zero_boundary() {
        // Boundary case: minimum: 0 (falsy in JSON) must still be moved.
        let schema = json!({"minimum": 0, "exclusiveMinimum": true});
        let adapted = adapt_oas30_schema(&schema);
        assert_eq!(
            adapted["exclusiveMinimum"],
            json!(0),
            "exclusiveMinimum must carry the zero minimum value"
        );
        assert!(
            adapted.get("minimum").is_none(),
            "minimum: 0 must be removed after rewrite"
        );
    }

    #[test]
    fn adapt_oas30_exclusive_minimum_false_removes_keyword() {
        // exclusiveMinimum: false means not exclusive; just drop the keyword.
        let schema = json!({"minimum": 1.0, "exclusiveMinimum": false});
        let adapted = adapt_oas30_schema(&schema);
        assert!(
            adapted.get("exclusiveMinimum").is_none(),
            "false exclusiveMinimum must be removed"
        );
        // minimum itself must remain unchanged (it is the non-exclusive bound).
        assert_eq!(adapted["minimum"], json!(1.0));
    }

    #[test]
    fn adapt_oas30_exclusive_minimum_true_no_minimum_removes_keyword() {
        // exclusiveMinimum: true but no minimum → nothing to convert; drop the keyword.
        let schema = json!({"type": "number", "exclusiveMinimum": true});
        let adapted = adapt_oas30_schema(&schema);
        assert!(
            adapted.get("exclusiveMinimum").is_none(),
            "exclusiveMinimum: true with no minimum must be removed"
        );
    }

    #[test]
    fn adapt_oas30_numeric_exclusive_minimum_passthrough() {
        // A numeric exclusiveMinimum (Draft 2020-12 style) must not be modified.
        let schema = json!({"exclusiveMinimum": 3.5});
        let adapted = adapt_oas30_schema(&schema);
        assert_eq!(
            adapted["exclusiveMinimum"],
            json!(3.5),
            "numeric exclusiveMinimum must pass through unchanged"
        );
    }

    #[test]
    fn adapt_oas30_exclusive_bounds_recursive_in_properties() {
        // Exclusive bound rewrite must happen recursively inside nested schemas.
        let schema = json!({
            "type": "object",
            "properties": {
                "count": {"type": "integer", "minimum": 0, "exclusiveMinimum": true}
            }
        });
        let adapted = adapt_oas30_schema(&schema);
        let count = &adapted["properties"]["count"];
        assert_eq!(count["exclusiveMinimum"], json!(0));
        assert!(count.get("minimum").is_none());
    }

    #[test]
    fn swagger_exclusive_minimum_rewritten() {
        // Swagger 2.0 schemas go through the same OAS 3.0 adaptation path.
        let swagger = r#"{
            "swagger": "2.0",
            "info": {"title": "T", "version": "1"},
            "paths": {},
            "definitions": {
                "PositiveInt": {"type": "integer", "minimum": 0, "exclusiveMinimum": true}
            }
        }"#;
        let doc = OpenApiDoc::from_json(swagger).unwrap();
        let schemas = doc.component_schemas();
        let s = &schemas["PositiveInt"].schema;
        assert_eq!(s["exclusiveMinimum"], json!(0));
        assert!(s.get("minimum").is_none());
    }
}
