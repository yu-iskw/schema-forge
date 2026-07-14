//! OpenAPI document version detection, normalisation, and schema extraction.

use indexmap::IndexMap;
use schemaforge_diagnostics::Diagnostic;
use serde_json::Value;

use crate::adapt::adapt_schema;
use crate::{OpenApiError, OpenApiVersion, SchemaEntry};

// ── Version detection ─────────────────────────────────────────────────────────

pub(crate) fn detect_version(doc: &Value) -> Result<OpenApiVersion, OpenApiError> {
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

pub(crate) fn normalise(
    raw: Value,
    version: OpenApiVersion,
    diagnostics: &mut Vec<Diagnostic>,
) -> Value {
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

pub(crate) fn extract_component_schemas(
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

pub(crate) fn extract_path_schemas(doc: &Value, version: OpenApiVersion) -> Vec<SchemaEntry> {
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
