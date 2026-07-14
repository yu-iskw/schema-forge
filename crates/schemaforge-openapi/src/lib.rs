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

mod adapt;
mod parse;

use indexmap::IndexMap;
use schemaforge_diagnostics::Diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use parse::{detect_version, extract_component_schemas, extract_path_schemas, normalise};

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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use schemaforge_diagnostics::Severity;
    use serde_json::json;

    use super::*;
    use crate::adapt::adapt_oas30_schema;

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
    fn adapt_oas30_nullable_with_type_and_enum_widens_type() {
        // nullable: true + type + enum: null must be added to both enum AND type
        // so that a null value passes both the type check and the enum check.
        let schema = json!({"type": "string", "enum": ["foo", "bar"], "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        // null must be in the enum array.
        let vals = adapted["enum"].as_array().unwrap();
        assert!(vals.contains(&Value::Null), "null must be in enum");
        // type must also include null so the null enum value is reachable.
        let types = adapted["type"].as_array().unwrap();
        assert!(types.contains(&json!("null")), "null must be in type");
        assert!(
            types.contains(&json!("string")),
            "string must remain in type"
        );
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
    fn adapt_oas30_nullable_with_const_and_type_widens_type() {
        // nullable: true + type + const: null must be added to enum AND type must
        // be widened so that a null value passes both the type check and the enum check.
        let schema = json!({"type": "string", "const": "active", "nullable": true});
        let adapted = adapt_oas30_schema(&schema);
        // const must be removed and replaced by enum.
        assert!(adapted.get("const").is_none(), "const must be removed");
        // enum must contain the original const value and null.
        let vals = adapted["enum"].as_array().unwrap();
        assert!(
            vals.contains(&json!("active")),
            "original const must be in enum"
        );
        assert!(vals.contains(&Value::Null), "null must be in enum");
        // type must be widened to include null so that the null enum value validates.
        let types = adapted["type"].as_array().unwrap();
        assert!(
            types.contains(&json!("string")),
            "string must remain in type"
        );
        assert!(types.contains(&json!("null")), "null must be added to type");
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
    fn adapt_oas30_nullable_recursive_pattern_properties() {
        // Nullable fields nested inside `patternProperties` must also be adapted.
        let schema = json!({
            "type": "object",
            "patternProperties": {
                "^x-": {"type": "string", "nullable": true}
            }
        });
        let adapted = adapt_oas30_schema(&schema);
        let pp_schema = &adapted["patternProperties"]["^x-"];
        let types = pp_schema["type"].as_array().unwrap();
        assert!(
            types.contains(&json!("null")),
            "nested nullable patternProperty must have null in type"
        );
        assert!(
            types.contains(&json!("string")),
            "nested patternProperty must retain original type"
        );
        assert!(
            pp_schema.get("nullable").is_none(),
            "nullable key must be removed from nested patternProperty"
        );
    }

    #[test]
    fn adapt_oas30_nullable_recursive_if_then_else() {
        // Nullable inside if/then/else must be adapted.
        let schema = json!({
            "if": {"type": "string"},
            "then": {"type": "string", "nullable": true},
            "else": {"type": "integer", "nullable": true}
        });
        let adapted = adapt_oas30_schema(&schema);
        let then_types = adapted["then"]["type"].as_array().unwrap();
        assert!(then_types.contains(&json!("null")));
        let else_types = adapted["else"]["type"].as_array().unwrap();
        assert!(else_types.contains(&json!("null")));
    }

    #[test]
    fn adapt_oas30_nullable_recursive_contains() {
        // Nullable nested under `contains` must be adapted via the shared
        // SCHEMA_SINGLE_KEYWORDS walk (not a hand-maintained keyword list).
        let schema = json!({
            "type": "array",
            "contains": {"type": "string", "nullable": true}
        });
        let adapted = adapt_oas30_schema(&schema);
        let types = adapted["contains"]["type"].as_array().unwrap();
        assert!(
            types.contains(&json!("null")),
            "nullable under contains must have null in type"
        );
        assert!(adapted["contains"].get("nullable").is_none());
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
