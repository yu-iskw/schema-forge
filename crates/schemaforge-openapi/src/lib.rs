//! OpenAPI 3.x document parsing and JSON Schema extraction for Schemaforge.
//!
//! Supports parsing OpenAPI 3.0 and 3.1 documents in JSON or YAML format,
//! extracting the component schemas as JSON Schema values for further
//! compilation.

use indexmap::IndexMap;
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
    /// The compiler failed to compile an extracted schema.
    #[error("compiler error for schema `{name}`: {reason}")]
    CompileError {
        /// Schema name.
        name: String,
        /// Compile error message.
        reason: String,
    },
}

/// OpenAPI version detected from the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenApiVersion {
    /// OpenAPI 3.0.x
    V30,
    /// OpenAPI 3.1.x
    V31,
}

impl std::fmt::Display for OpenApiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V30 => f.write_str("3.0"),
            Self::V31 => f.write_str("3.1"),
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
    /// Raw document value.
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
        let version = detect_version(&raw)?;
        Ok(Self { version, raw })
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
        let version = detect_version(&raw)?;
        Ok(Self { version, raw })
    }

    /// Extract all component schemas from the document.
    ///
    /// Returns an ordered map of schema name → [`SchemaEntry`].
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

fn detect_version(doc: &Value) -> Result<OpenApiVersion, OpenApiError> {
    let version_str = doc
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| OpenApiError::MissingField("openapi".to_owned()))?;
    if version_str.starts_with("3.1") {
        Ok(OpenApiVersion::V31)
    } else if version_str.starts_with("3.0") {
        Ok(OpenApiVersion::V30)
    } else {
        Err(OpenApiError::UnsupportedVersion(version_str.to_owned()))
    }
}

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

/// Adapt an OpenAPI schema to a standalone JSON Schema.
///
/// OpenAPI 3.0 schemas use a subset of JSON Schema Draft 4/7 with some
/// extensions (`nullable`, `discriminator`). OpenAPI 3.1 uses JSON Schema
/// 2020-12 directly.
fn adapt_schema(schema: &Value, version: OpenApiVersion) -> Value {
    match version {
        OpenApiVersion::V31 => schema.clone(),
        OpenApiVersion::V30 => adapt_oas30_schema(schema),
    }
}

fn adapt_oas30_schema(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut new_obj = obj.clone();
    // Handle `nullable: true` → add `null` to `type`
    if new_obj
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        new_obj.remove("nullable");
        let type_val = new_obj.get("type").cloned().unwrap_or(Value::Null);
        new_obj.insert("type".to_owned(), make_nullable_type(type_val));
    }
    Value::Object(new_obj)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SIMPLE_OPENAPI: &str = r#"{
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
        let doc = OpenApiDoc::from_json(SIMPLE_OPENAPI).unwrap();
        assert_eq!(doc.version, OpenApiVersion::V31);
    }

    #[test]
    fn extract_component_schemas() {
        let doc = OpenApiDoc::from_json(SIMPLE_OPENAPI).unwrap();
        let schemas = doc.component_schemas();
        assert!(schemas.contains_key("User"));
        let user = &schemas["User"];
        assert_eq!(user.name, "User");
        assert!(user.pointer.contains("/components/schemas/User"));
    }

    #[test]
    fn unsupported_version_error() {
        let bad = r#"{"openapi": "2.0", "info": {}, "paths": {}}"#;
        let result = OpenApiDoc::from_json(bad);
        assert!(matches!(result, Err(OpenApiError::UnsupportedVersion(_))));
    }

    #[test]
    fn adapt_oas30_nullable() {
        let schema = json!({"type": "string", "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        let types = adapted["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")));
        assert!(types.contains(&json!("string")));
    }
}
