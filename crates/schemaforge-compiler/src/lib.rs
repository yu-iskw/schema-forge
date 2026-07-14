//! Core compilation pipeline: JSON Schema source → Schemaforge IR.
//!
//! The [`Compiler`] accepts raw JSON or YAML source, detects the dialect,
//! lowers local `$ref` references (fragments pointing into `#/$defs/…` or
//! the document root `#`), and produces a [`SchemaIr`].  External `$ref`
//! URIs are not yet fetched; they produce [`CompileError::UnresolvedRef`].
//!
//! # Helper modules
//!
//! - [`inspect`]: inspect a compiled IR (dialect, node count, capabilities).
//! - [`explain`]: explain representation strategy and codegen decisions.

pub mod explain;
pub mod inspect;

pub use explain::{ExplainResult, explain_ir};
pub use inspect::{InspectResult, inspect_ir};

use indexmap::IndexMap;
use schemaforge_dialect::detect;
use schemaforge_ir::{
    ArrayConstraints, NumericBound, NumericConstraints, ObjectConstraints, SchemaIr, SchemaNode,
    StringConstraints, TypeSet,
};
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
    ///
    /// For local fragment refs (`#/…`) this means the JSON Pointer did not
    /// point to an existing location in the document.  External URI refs are
    /// not yet resolved and always produce this error.
    #[error("unresolved ref `{uri}`: {reason}")]
    UnresolvedRef {
        /// The unresolvable URI or JSON Pointer.
        uri: String,
        /// Human-readable reason the ref could not be lowered.
        reason: String,
    },
    /// A `$ref` introduces a reference cycle.
    ///
    /// The same local fragment ref (`#/…` or `#`) appeared recursively in its
    /// own lowering call stack.  Silently accepting a cycle would produce an
    /// unbounded schema that cannot be represented in the IR; the compiler
    /// therefore rejects the schema.
    #[error("cyclic $ref detected: `{uri}` is referenced within its own definition")]
    CyclicRef {
        /// The URI that forms the cycle.
        uri: String,
    },
    /// The schema value is neither a JSON boolean nor a JSON object.
    ///
    /// JSON Schema allows only `true`, `false`, and object schemas.  Any other
    /// JSON value (number, string, array, null) is invalid and rejected
    /// fail-closed rather than treated as an open schema.
    #[error("invalid schema: must be a boolean or object, got `{kind}`")]
    InvalidSchemaKind {
        /// The JSON type name of the unexpected value.
        kind: String,
    },
}

/// Options that control the compiler's behaviour.
#[derive(Debug, Clone, Default)]
pub struct CompilerOptions {
    /// Base URI used when the schema document URI is empty or absent.
    ///
    /// When a non-empty `base_uri` is set, it is substituted as the document
    /// URI during compilation and source-map registration for any call where
    /// the caller passes an empty string.
    pub base_uri: String,
}

/// The Schemaforge compiler: transforms source text into an IR.
pub struct Compiler {
    /// Fallback document URI (from [`CompilerOptions::base_uri`]).
    base_uri: String,
}

impl Compiler {
    /// Create a new compiler with default options.
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(&CompilerOptions::default())
    }

    /// Create a new compiler with custom options.
    #[must_use]
    pub fn with_options(options: &CompilerOptions) -> Self {
        Self {
            base_uri: options.base_uri.clone(),
        }
    }

    /// Compile a JSON Schema from a JSON string.
    ///
    /// `uri` identifies the document; when empty, [`CompilerOptions::base_uri`]
    /// is used as a fallback.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] when the source is invalid or a `$ref` cannot
    /// be resolved.
    pub fn compile_json(&mut self, uri: &str, source: &str) -> Result<SchemaIr, CompileError> {
        let effective_uri = self.resolve_uri(uri);
        let value: Value =
            serde_json::from_str(source).map_err(|e| CompileError::JsonParse(e.to_string()))?;
        let digest = sha256_hex(source.as_bytes());
        compile_value(&effective_uri, &value, &digest)
    }

    /// Compile a JSON Schema from a YAML string.
    ///
    /// `uri` identifies the document; when empty, [`CompilerOptions::base_uri`]
    /// is used as a fallback.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError`] when the source is invalid or a `$ref` cannot
    /// be resolved.
    pub fn compile_yaml(&mut self, uri: &str, source: &str) -> Result<SchemaIr, CompileError> {
        let effective_uri = self.resolve_uri(uri);
        let value: Value =
            serde_saphyr::from_str(source).map_err(|e| CompileError::YamlParse(e.to_string()))?;
        let digest = sha256_hex(source.as_bytes());
        compile_value(&effective_uri, &value, &digest)
    }

    /// Return `uri` when non-empty, otherwise fall back to `self.base_uri`.
    fn resolve_uri(&self, uri: &str) -> String {
        if uri.is_empty() {
            self.base_uri.clone()
        } else {
            uri.to_owned()
        }
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

fn compile_value(uri: &str, value: &Value, digest: &str) -> Result<SchemaIr, CompileError> {
    let dialect = detect(value);
    let mut ctx = LowerCtx::new(value);
    let root = lower_schema(value, &mut ctx)?;
    Ok(SchemaIr::new(root, dialect.uri(), digest, uri))
}

/// Context threaded through schema lowering; carries the root document for
/// local `$ref` resolution and a visiting stack to detect reference cycles.
struct LowerCtx<'a> {
    /// The root document value, used to follow `#/…` JSON-pointer references.
    root: &'a Value,
    /// JSON Pointer strings of `$ref`s currently being lowered (cycle guard).
    visiting: Vec<String>,
}

impl<'a> LowerCtx<'a> {
    const fn new(root: &'a Value) -> Self {
        Self {
            root,
            visiting: Vec::new(),
        }
    }
}

/// Lower a JSON Schema [`Value`] into an IR [`SchemaNode`].
///
/// JSON Schema allows only `true`, `false`, and object values as schemas.
/// Any other JSON value is rejected fail-closed with
/// [`CompileError::InvalidSchemaKind`] rather than silently treated as an
/// open schema.
fn lower_schema(value: &Value, ctx: &mut LowerCtx<'_>) -> Result<SchemaNode, CompileError> {
    match value {
        Value::Bool(true) => Ok(SchemaNode::boolean_schema(true)),
        Value::Bool(false) => Ok(SchemaNode::boolean_schema(false)),
        Value::Object(obj) => lower_object_schema(obj, ctx),
        other => Err(CompileError::InvalidSchemaKind {
            kind: json_type_name(other).to_owned(),
        }),
    }
}

/// Return the JSON type name of `value` for use in error messages.
const fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Resolve a local fragment `$ref` (e.g. `#`, `#/$defs/Foo`) into an IR node.
///
/// Returns [`CompileError::CyclicRef`] when the same ref appears in the
/// current call stack, because a recursive schema cannot be faithfully
/// lowered to a finite IR.
fn lower_local_ref(ref_str: &str, ctx: &mut LowerCtx<'_>) -> Result<SchemaNode, CompileError> {
    if ctx.visiting.iter().any(|v| v == ref_str) {
        return Err(CompileError::CyclicRef {
            uri: ref_str.to_owned(),
        });
    }
    // Clone the target so that `ctx` is free to be mutably borrowed during
    // the recursive lowering call (the root document is small-ish in practice).
    let target: Value = if ref_str == "#" {
        ctx.root.clone()
    } else {
        let pointer = ref_str.strip_prefix('#').unwrap_or(ref_str);
        ctx.root
            .pointer(pointer)
            .ok_or_else(|| CompileError::UnresolvedRef {
                uri: ref_str.to_owned(),
                reason: format!("JSON pointer `{pointer}` not found in document"),
            })?
            .clone()
    };
    ctx.visiting.push(ref_str.to_owned());
    let result = lower_schema(&target, ctx);
    ctx.visiting.pop();
    result
}

fn lower_object_schema(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
) -> Result<SchemaNode, CompileError> {
    // A `$ref` short-circuits all sibling keywords (pre-2019-09 behaviour).
    // We lower the referenced schema directly; sibling keywords are ignored
    // because the overwhelming majority of real-world schemas follow this
    // convention and mixing $ref with siblings requires full merging logic.
    if let Some(Value::String(ref_str)) = obj.get("$ref") {
        if ref_str.starts_with('#') {
            return lower_local_ref(ref_str, ctx);
        }
        // External URI – not yet supported.
        return Err(CompileError::UnresolvedRef {
            uri: ref_str.clone(),
            reason: "external $ref URIs are not yet resolved; only local fragment refs (#/…) are supported".to_owned(),
        });
    }
    let mut node = SchemaNode::default();
    extract_types(obj, &mut node);
    extract_metadata(obj, &mut node);
    extract_string_constraints(obj, &mut node);
    extract_numeric_constraints(obj, &mut node);
    extract_array_constraints(obj, &mut node);
    extract_object_constraints(obj, &mut node);
    lower_properties(obj, ctx, &mut node)?;
    lower_combinators(obj, ctx, &mut node)?;
    lower_items(obj, ctx, &mut node)?;
    lower_defs(obj, ctx, &mut node)?;
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
    ctx: &mut LowerCtx<'_>,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    if let Some(Value::Object(props)) = obj.get("properties") {
        let mut map = IndexMap::new();
        for (k, v) in props {
            map.insert(k.clone(), lower_schema(v, ctx)?);
        }
        node.properties = map;
    }
    if let Some(ap) = obj.get("additionalProperties") {
        node.additional_properties = Some(Box::new(lower_schema(ap, ctx)?));
    }
    Ok(())
}

fn lower_combinators(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    lower_schema_array(obj, "allOf", ctx, &mut node.all_of)?;
    lower_schema_array(obj, "anyOf", ctx, &mut node.any_of)?;
    lower_schema_array(obj, "oneOf", ctx, &mut node.one_of)?;
    if let Some(not_val) = obj.get("not") {
        node.not = Some(Box::new(lower_schema(not_val, ctx)?));
    }
    Ok(())
}

fn lower_schema_array(
    obj: &serde_json::Map<String, Value>,
    key: &str,
    ctx: &mut LowerCtx<'_>,
    target: &mut Vec<SchemaNode>,
) -> Result<(), CompileError> {
    let Some(Value::Array(arr)) = obj.get(key) else {
        return Ok(());
    };
    for v in arr {
        target.push(lower_schema(v, ctx)?);
    }
    Ok(())
}

fn lower_items(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
    node: &mut SchemaNode,
) -> Result<(), CompileError> {
    if let Some(items_val) = obj.get("items") {
        node.items = Some(Box::new(lower_schema(items_val, ctx)?));
    }
    if let Some(Value::Array(prefix)) = obj.get("prefixItems") {
        for v in prefix {
            node.prefix_items.push(lower_schema(v, ctx)?);
        }
    }
    Ok(())
}

fn lower_defs(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
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
        node.defs.insert(k.clone(), lower_schema(v, ctx)?);
    }
    Ok(())
}

/// Shared test helper: wrap a [`SchemaNode`] in a minimal [`SchemaIr`].
#[cfg(test)]
pub(crate) fn make_test_ir(root: SchemaNode) -> SchemaIr {
    SchemaIr::new(
        root,
        "https://json-schema.org/draft/2020-12/schema",
        "digest",
        "test://s",
    )
}

/// Compute the SHA-256 hex digest of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
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

    #[test]
    fn compile_local_ref_to_defs() {
        let mut c = Compiler::new();
        let src = r##"{
            "type": "object",
            "properties": {
                "name": {"$ref": "#/$defs/StringField"}
            },
            "$defs": {
                "StringField": {"type": "string", "minLength": 1}
            }
        }"##;
        let ir = c.compile_json("test://ref.json", src).unwrap();
        let name_prop = ir.root.properties.get("name").expect("property exists");
        assert!(name_prop.types.string, "ref lowered to string type");
        assert_eq!(name_prop.string.min_length, Some(1));
    }

    #[test]
    fn compile_root_ref_cycle_is_error() {
        let mut c = Compiler::new();
        // A `$defs` entry that `$ref`s back to the document root creates an
        // unbounded recursive schema.  The compiler must reject it with a
        // `CyclicRef` error rather than silently returning an open schema.
        let src = r##"{"$defs": {"Self": {"$ref": "#"}}}"##;
        let result = c.compile_json("test://root-ref.json", src);
        assert!(
            matches!(result, Err(CompileError::CyclicRef { .. })),
            "expected CyclicRef error but got: {result:?}"
        );
    }

    #[test]
    fn compile_non_schema_number_returns_error() {
        let mut c = Compiler::new();
        let result = c.compile_json("test://bad.json", "42");
        assert!(
            matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
            "expected InvalidSchemaKind for number schema, got: {result:?}"
        );
    }

    #[test]
    fn compile_non_schema_string_returns_error() {
        let mut c = Compiler::new();
        let result = c.compile_json("test://bad.json", r#""just a string""#);
        assert!(
            matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
            "expected InvalidSchemaKind for string schema, got: {result:?}"
        );
    }

    #[test]
    fn compile_non_schema_array_returns_error() {
        let mut c = Compiler::new();
        let result = c.compile_json("test://bad.json", "[]");
        assert!(
            matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
            "expected InvalidSchemaKind for array schema, got: {result:?}"
        );
    }

    #[test]
    fn compile_non_schema_null_returns_error() {
        let mut c = Compiler::new();
        let result = c.compile_json("test://bad.json", "null");
        assert!(
            matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
            "expected InvalidSchemaKind for null schema, got: {result:?}"
        );
    }

    #[test]
    fn compile_non_cyclic_ref_succeeds() {
        let mut c = Compiler::new();
        // A ref that points to a definition that does not refer back should
        // still compile successfully.
        let src = r##"{
            "type": "object",
            "properties": {
                "name": {"$ref": "#/$defs/StringField"}
            },
            "$defs": {
                "StringField": {"type": "string"}
            }
        }"##;
        let ir = c.compile_json("test://non-cyclic.json", src).unwrap();
        assert!(ir.root.properties.contains_key("name"));
    }
}
