//! Core compilation pipeline: JSON Schema source → Schemaforge IR.
//!
//! The [`Compiler`] accepts raw JSON or YAML source, detects the dialect,
//! resolves `$ref` references, and produces a [`SchemaIr`].

use std::sync::Arc;

use indexmap::IndexMap;
use schemaforge_dialect::{Dialect, detect};
use schemaforge_ir::{
    ArrayConstraints, NumericBound, NumericConstraints, ObjectConstraints, SchemaIr, SchemaNode,
    StringConstraints, TypeSet,
};
use schemaforge_resolver::{OfflineResolver, ResolveError, Resolver};
use schemaforge_runtime::RUNTIME_PLAN;
use schemaforge_source::SourceMap;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Error returned when compilation fails.
#[derive(Debug, Error)]
pub enum CompileError {
    /// The source JSON is malformed.
    #[error("JSON parse error: {0}")]
    JsonParse(String),
    /// The source YAML is malformed.
    #[error("YAML parse error: {0}")]
    YamlParse(String),
    /// A `$ref` could not be resolved.
    #[error("unresolved ref `{uri}`: {source}")]
    UnresolvedRef {
        /// The unresolvable URI.
        uri: String,
        /// The underlying resolve error.
        #[source]
        source: ResolveError,
    },
    /// The dialect is not supported.
    #[error("unsupported dialect: {0}")]
    UnsupportedDialect(String),
}

/// Options that control the compiler's behaviour.
#[derive(Debug, Clone)]
pub struct CompilerOptions {
    /// Default dialect when `$schema` is absent.
    pub default_dialect: Dialect,
    /// Base URI for relative `$id` resolution.
    pub base_uri: String,
    /// Pre-loaded schemas for offline `$ref` resolution.
    pub preloaded_schemas: Vec<(String, Value)>,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            default_dialect: Dialect::Draft202012,
            base_uri: String::new(),
            preloaded_schemas: Vec::new(),
        }
    }
}

/// The Schemaforge compiler: transforms source text into an IR.
pub struct Compiler {
    #[allow(dead_code)]
    options: CompilerOptions,
    resolver: Arc<dyn Resolver>,
    source_map: SourceMap,
}

impl Compiler {
    /// Create a new compiler with default options and offline resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(CompilerOptions::default())
    }

    /// Create a new compiler with custom options.
    #[must_use]
    pub fn with_options(options: CompilerOptions) -> Self {
        let mut resolver = OfflineResolver::new();
        for (uri, schema) in &options.preloaded_schemas {
            resolver.register(uri.clone(), schema.clone());
        }
        Self {
            options,
            resolver: Arc::new(resolver),
            source_map: SourceMap::new(),
        }
    }

    /// Compile a JSON Schema from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] when the source is invalid or a `$ref` cannot
    /// be resolved.
    pub fn compile_json(&mut self, uri: &str, source: &str) -> Result<SchemaIr, CompileError> {
        let value: Value =
            serde_json::from_str(source).map_err(|e| CompileError::JsonParse(e.to_string()))?;
        let digest = sha256_hex(source.as_bytes());
        self.source_map.add(uri, Arc::from(source));
        self.compile_value(uri, &value, &digest)
    }

    /// Compile a JSON Schema from a YAML string.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] when the source is invalid or a `$ref` cannot
    /// be resolved.
    pub fn compile_yaml(&mut self, uri: &str, source: &str) -> Result<SchemaIr, CompileError> {
        let value: Value =
            serde_saphyr::from_str(source).map_err(|e| CompileError::YamlParse(e.to_string()))?;
        let digest = sha256_hex(source.as_bytes());
        self.source_map.add(uri, Arc::from(source));
        self.compile_value(uri, &value, &digest)
    }

    fn compile_value(
        &self,
        uri: &str,
        value: &Value,
        digest: &str,
    ) -> Result<SchemaIr, CompileError> {
        let dialect = detect(value);
        let _ = RUNTIME_PLAN;
        let root = lower_schema(value, uri, &*self.resolver)?;
        Ok(SchemaIr::new(root, dialect.uri(), digest, uri))
    }

    /// Access the source map populated during compilation.
    #[must_use]
    pub const fn source_map(&self) -> &SourceMap {
        &self.source_map
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Lower a JSON Schema [`Value`] into an IR [`SchemaNode`].
fn lower_schema(
    value: &Value,
    base_uri: &str,
    resolver: &dyn Resolver,
) -> Result<SchemaNode, CompileError> {
    match value {
        Value::Bool(true) => Ok(SchemaNode::boolean_schema(true)),
        Value::Bool(false) => Ok(SchemaNode::boolean_schema(false)),
        Value::Object(obj) => lower_object_schema(obj, base_uri, resolver),
        _ => Ok(SchemaNode::any()),
    }
}

fn lower_object_schema(
    obj: &serde_json::Map<String, Value>,
    base_uri: &str,
    resolver: &dyn Resolver,
) -> Result<SchemaNode, CompileError> {
    let mut node = SchemaNode::default();
    extract_types(obj, &mut node);
    extract_metadata(obj, &mut node);
    extract_string_constraints(obj, &mut node);
    extract_numeric_constraints(obj, &mut node);
    extract_array_constraints(obj, &mut node);
    extract_object_constraints(obj, &mut node);
    lower_properties(obj, base_uri, resolver, &mut node)?;
    lower_combinators(obj, base_uri, resolver, &mut node)?;
    lower_items(obj, base_uri, resolver, &mut node)?;
    lower_defs(obj, base_uri, resolver, &mut node)?;
    Ok(node)
}

fn extract_types(obj: &serde_json::Map<String, Value>, node: &mut SchemaNode) {
    if let Some(t) = obj.get("type") {
        node.types = TypeSet::from_json(t);
    }
    if let Some(vals) = obj.get("enum").and_then(Value::as_array) {
        node.enum_values.clone_from(vals);
    }
    if let Some(c) = obj.get("const") {
        node.const_value = Some(c.clone());
    }
}

fn extract_metadata(obj: &serde_json::Map<String, Value>, node: &mut SchemaNode) {
    node.title = obj.get("title").and_then(Value::as_str).map(str::to_owned);
    node.description = obj
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    node.id = obj.get("$id").and_then(Value::as_str).map(str::to_owned);
}

fn extract_string_constraints(obj: &serde_json::Map<String, Value>, node: &mut SchemaNode) {
    node.string = StringConstraints {
        min_length: obj.get("minLength").and_then(Value::as_u64),
        max_length: obj.get("maxLength").and_then(Value::as_u64),
        pattern: obj
            .get("pattern")
            .and_then(Value::as_str)
            .map(str::to_owned),
        format: obj.get("format").and_then(Value::as_str).map(str::to_owned),
    };
}

fn extract_numeric_constraints(obj: &serde_json::Map<String, Value>, node: &mut SchemaNode) {
    let minimum = obj
        .get("minimum")
        .and_then(Value::as_f64)
        .map(|v| NumericBound {
            value: v,
            exclusive: false,
        });
    let exclusive_min = obj
        .get("exclusiveMinimum")
        .and_then(Value::as_f64)
        .map(|v| NumericBound {
            value: v,
            exclusive: true,
        });
    let maximum = obj
        .get("maximum")
        .and_then(Value::as_f64)
        .map(|v| NumericBound {
            value: v,
            exclusive: false,
        });
    let exclusive_max = obj
        .get("exclusiveMaximum")
        .and_then(Value::as_f64)
        .map(|v| NumericBound {
            value: v,
            exclusive: true,
        });
    node.numeric = NumericConstraints {
        minimum: exclusive_min.or(minimum),
        maximum: exclusive_max.or(maximum),
        multiple_of: obj.get("multipleOf").and_then(Value::as_f64),
    };
}

fn extract_array_constraints(obj: &serde_json::Map<String, Value>, node: &mut SchemaNode) {
    node.array = ArrayConstraints {
        min_items: obj.get("minItems").and_then(Value::as_u64),
        max_items: obj.get("maxItems").and_then(Value::as_u64),
        unique_items: obj
            .get("uniqueItems")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        min_contains: obj.get("minContains").and_then(Value::as_u64),
        max_contains: obj.get("maxContains").and_then(Value::as_u64),
    };
}

fn extract_object_constraints(obj: &serde_json::Map<String, Value>, node: &mut SchemaNode) {
    let required = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    node.object = ObjectConstraints {
        required,
        min_properties: obj.get("minProperties").and_then(Value::as_u64),
        max_properties: obj.get("maxProperties").and_then(Value::as_u64),
    };
}

fn lower_properties(
    obj: &serde_json::Map<String, Value>,
    base_uri: &str,
    resolver: &dyn Resolver,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    if let Some(Value::Object(props)) = obj.get("properties") {
        let mut map = IndexMap::new();
        for (k, v) in props {
            map.insert(k.clone(), lower_schema(v, base_uri, resolver)?);
        }
        node.properties = map;
    }
    if let Some(ap) = obj.get("additionalProperties") {
        node.additional_properties = Some(Box::new(lower_schema(ap, base_uri, resolver)?));
    }
    Ok(())
}

fn lower_combinators(
    obj: &serde_json::Map<String, Value>,
    base_uri: &str,
    resolver: &dyn Resolver,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    lower_schema_array(obj, "allOf", base_uri, resolver, &mut node.all_of)?;
    lower_schema_array(obj, "anyOf", base_uri, resolver, &mut node.any_of)?;
    lower_schema_array(obj, "oneOf", base_uri, resolver, &mut node.one_of)?;
    if let Some(not_val) = obj.get("not") {
        node.not = Some(Box::new(lower_schema(not_val, base_uri, resolver)?));
    }
    Ok(())
}

fn lower_schema_array(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    base_uri: &str,
    resolver: &dyn Resolver,
    target: &mut Vec<SchemaNode>,
) -> Result<(), CompileError> {
    let Some(Value::Array(arr)) = obj.get(key) else {
        return Ok(());
    };
    for v in arr {
        target.push(lower_schema(v, base_uri, resolver)?);
    }
    Ok(())
}

fn lower_items(
    obj: &serde_json::Map<String, Value>,
    base_uri: &str,
    resolver: &dyn Resolver,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    if let Some(items_val) = obj.get("items") {
        node.items = Some(Box::new(lower_schema(items_val, base_uri, resolver)?));
    }
    if let Some(Value::Array(prefix)) = obj.get("prefixItems") {
        for v in prefix {
            node.prefix_items.push(lower_schema(v, base_uri, resolver)?);
        }
    }
    Ok(())
}

fn lower_defs(
    obj: &serde_json::Map<String, Value>,
    base_uri: &str,
    resolver: &dyn Resolver,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    let defs_key = if obj.contains_key("$defs") {
        "$defs"
    } else {
        "definitions"
    };
    let Some(Value::Object(defs)) = obj.get(defs_key) else {
        return Ok(());
    };
    for (k, v) in defs {
        node.defs
            .insert(k.clone(), lower_schema(v, base_uri, resolver)?);
    }
    Ok(())
}

/// Compute the SHA-256 hex digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_simple_json_schema() {
        let mut c = Compiler::new();
        let ir = c
            .compile_json("test://schema.json", r#"{"type":"string","minLength":1}"#)
            .unwrap();
        assert!(ir.root.types.string);
        assert!(!ir.root.types.number);
        assert_eq!(ir.root.string.min_length, Some(1));
    }

    #[test]
    fn compile_boolean_schema_true() {
        let mut c = Compiler::new();
        let ir = c.compile_json("test://bool.json", "true").unwrap();
        assert!(ir.root.types.string);
    }

    #[test]
    fn compile_boolean_schema_false() {
        let mut c = Compiler::new();
        let ir = c.compile_json("test://bool.json", "false").unwrap();
        assert!(ir.root.is_never());
    }

    #[test]
    fn compile_object_with_properties() {
        let mut c = Compiler::new();
        let src = r#"{
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age":  {"type": "integer"}
            },
            "required": ["name"]
        }"#;
        let ir = c.compile_json("test://obj.json", src).unwrap();
        assert!(ir.root.types.object);
        assert!(ir.root.properties.contains_key("name"));
        assert!(ir.root.object.required.contains(&"name".to_owned()));
    }

    #[test]
    fn compile_invalid_json_returns_error() {
        let mut c = Compiler::new();
        let result = c.compile_json("test://bad.json", "{invalid}");
        assert!(result.is_err());
    }

    #[test]
    fn source_digest_is_hex() {
        let mut c = Compiler::new();
        let ir = c.compile_json("test://digest.json", "{}").unwrap();
        assert!(ir.source_digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(ir.source_digest.len(), 64);
    }
}
