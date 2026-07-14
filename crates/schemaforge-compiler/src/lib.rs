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
    /// A long acyclic `$ref` chain exceeded the maximum allowed resolution
    /// depth.
    ///
    /// The compiler limits `$ref` indirection to [`LowerCtx::MAX_DEPTH`]
    /// levels to prevent stack overflows on pathologically deep schemas.
    /// Schemas that require deeper chains cannot be compiled.
    #[error("$ref resolution exceeded maximum depth of {depth} levels")]
    DepthExceeded {
        /// The depth at which the limit was reached.
        depth: usize,
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
    /// A schema keyword is recognised but not yet lowered by the compiler.
    ///
    /// The compiler is fail-closed: any applicator or assertion keyword whose
    /// semantics are not yet represented in the IR causes compilation to abort
    /// rather than silently dropping the constraint.
    #[error("unsupported schema keyword `{keyword}`")]
    UnsupportedKeyword {
        /// The name of the unsupported keyword.
        keyword: String,
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
    /// Maximum allowed `$ref` resolution depth (matching the validator limit).
    ///
    /// Schemas with longer acyclic `$ref` chains are rejected with
    /// [`CompileError::DepthExceeded`] rather than risking a stack overflow.
    pub(crate) const MAX_DEPTH: usize = 128;

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
    let depth = ctx.visiting.len();
    if depth >= LowerCtx::MAX_DEPTH {
        return Err(CompileError::DepthExceeded { depth });
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

/// Schema keywords that are recognised by JSON Schema but are not yet lowered
/// by the compiler into IR.
///
/// The compiler is fail-closed: encountering any of these keywords during
/// object-schema lowering returns [`CompileError::UnsupportedKeyword`] rather
/// than silently dropping semantics-bearing constraints.
const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "unevaluatedProperties",
    "unevaluatedItems",
    "patternProperties",
    "if",
    "then",
    "else",
    "dependentSchemas",
    "propertyNames",
    "contains",
    // Dynamic reference applicators (Draft 2020-12) — not yet lowered.
    // $dynamicRef is an applicator with runtime resolution semantics.
    // $dynamicAnchor is its declaration counterpart; both are rejected for
    // honesty until dynamic scoping is represented in the IR.
    "$dynamicRef",
    "$dynamicAnchor",
    // Property dependency assertions (Draft 2019-09 / 2020-12).
    "dependentRequired",
    // Content annotations — not assertions in standard validators, but they
    // carry semantics (media type, encoding, schema) that the IR cannot yet
    // represent faithfully.
    "contentSchema",
    "contentMediaType",
    "contentEncoding",
];

/// Return an error if `obj` contains any keyword that the compiler cannot yet lower.
fn check_unsupported_keywords(obj: &serde_json::Map<String, Value>) -> Result<(), CompileError> {
    for &keyword in UNSUPPORTED_KEYWORDS {
        if obj.contains_key(keyword) {
            return Err(CompileError::UnsupportedKeyword {
                keyword: keyword.to_owned(),
            });
        }
    }
    Ok(())
}

/// Schema keywords that do not add applicator or assertion constraints and
/// can be ignored when deciding whether a `$ref` has meaningful siblings.
const REF_PASSTHROUGH_KEYS: &[&str] = &[
    "$ref",
    "$id",
    "$anchor",
    "$dynamicAnchor",
    "$schema",
    "$defs",
    "$comment",
];

fn lower_object_schema(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
) -> Result<SchemaNode, CompileError> {
    if let Some(Value::String(ref_str)) = obj.get("$ref") {
        if ref_str.starts_with('#') {
            return lower_local_ref_with_siblings(ref_str, obj, ctx);
        }
        // External URI – not yet supported.
        return Err(CompileError::UnresolvedRef {
            uri: ref_str.clone(),
            reason: "external $ref URIs are not yet resolved; only local fragment refs (#/…) are supported".to_owned(),
        });
    }
    lower_object_keywords(obj, ctx)
}

/// Lower a local `$ref` that may have sibling keywords (Draft 2020-12 semantics).
///
/// When constraint keywords appear alongside `$ref` (anything beyond the
/// pure-annotation pass-through set), the compiler builds an `allOf` of
/// the referenced schema and the sibling constraints so that both are
/// applied.  If only metadata / structural keywords accompany `$ref`, the
/// short-circuit path is kept for efficiency.
fn lower_local_ref_with_siblings(
    ref_str: &str,
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
) -> Result<SchemaNode, CompileError> {
    let has_siblings = obj
        .keys()
        .any(|k| !REF_PASSTHROUGH_KEYS.contains(&k.as_str()));
    if !has_siblings {
        return lower_local_ref(ref_str, ctx);
    }
    // Build allOf([ref_target, sibling_constraints]) so both are enforced.
    let ref_node = lower_local_ref(ref_str, ctx)?;
    let mut obj_without_ref = obj.clone();
    obj_without_ref.remove("$ref");
    let sibling_node = lower_object_keywords(&obj_without_ref, ctx)?;
    Ok(SchemaNode {
        all_of: vec![ref_node, sibling_node],
        ..SchemaNode::default()
    })
}

/// Lower all non-`$ref` keywords of an object schema into a [`SchemaNode`].
fn lower_object_keywords(
    obj: &serde_json::Map<String, Value>,
    ctx: &mut LowerCtx<'_>,
) -> Result<SchemaNode, CompileError> {
    check_unsupported_keywords(obj)?;
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
    node.numeric = NumericConstraints {
        minimum: extract_bound(
            obj,
            "exclusiveMinimum",
            obj.get("minimum").and_then(Value::as_f64),
        ),
        maximum: extract_bound(
            obj,
            "exclusiveMaximum",
            obj.get("maximum").and_then(Value::as_f64),
        ),
        multiple_of: obj.get("multipleOf").and_then(Value::as_f64),
    };
}

/// Resolve the effective numeric bound for a minimum or maximum constraint.
///
/// Handles both Draft 2020-12 style (`exclusiveMinimum: <number>`) and the
/// legacy boolean style used in Draft 4 / OpenAPI 3.0 (`exclusiveMinimum:
/// true` paired with a numeric `minimum`).
///
/// - `exclusiveMinimum: <number>` → exclusive bound at that value.
/// - `exclusiveMinimum: true`     → promote adjacent `minimum` to exclusive.
/// - `exclusiveMinimum: false`    → keep adjacent `minimum` as non-exclusive.
/// - `exclusiveMinimum` absent    → keep adjacent `minimum` as non-exclusive.
fn extract_bound(
    obj: &serde_json::Map<String, Value>,
    exclusive_key: &str,
    adjacent_value: Option<f64>,
) -> Option<NumericBound> {
    match obj.get(exclusive_key) {
        Some(Value::Number(n)) => n.as_f64().map(|v| NumericBound {
            value: v,
            exclusive: true,
        }),
        Some(Value::Bool(true)) => adjacent_value.map(|v| NumericBound {
            value: v,
            exclusive: true,
        }),
        _ => adjacent_value.map(|v| NumericBound {
            value: v,
            exclusive: false,
        }),
    }
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

    // ── $ref + siblings (Draft 2020-12) ──────────────────────────────────────

    #[test]
    fn compile_ref_with_type_sibling_applies_both() {
        // A schema with $ref AND a sibling `type` keyword should produce an
        // allOf that enforces both the referenced schema and the type assertion.
        let mut c = Compiler::new();
        let src = r##"{
            "$defs": {
                "Base": {"minLength": 1}
            },
            "$ref": "#/$defs/Base",
            "type": "string"
        }"##;
        let ir = c.compile_json("test://ref-siblings.json", src).unwrap();
        // The root node must carry an allOf with exactly two members.
        assert_eq!(
            ir.root.all_of.len(),
            2,
            "expected allOf([ref_node, sibling_node]), got {:?}",
            ir.root.all_of.len()
        );
        // One member should carry the minLength from the $ref target.
        let has_min_length = ir
            .root
            .all_of
            .iter()
            .any(|n| n.string.min_length == Some(1));
        assert!(
            has_min_length,
            "allOf should contain the $ref target with minLength"
        );
        // One member should carry the type constraint from the sibling.
        let has_type = ir
            .root
            .all_of
            .iter()
            .any(|n| n.types.string && !n.types.number);
        assert!(
            has_type,
            "allOf should contain the sibling with type:string"
        );
    }

    #[test]
    fn compile_ref_without_siblings_is_shortcut() {
        // A $ref with only metadata-style siblings must still take the
        // short-circuit path (no allOf wrapper).
        let mut c = Compiler::new();
        let src = r##"{
            "$defs": {"S": {"type": "integer"}},
            "$ref": "#/$defs/S",
            "$comment": "just a comment"
        }"##;
        let ir = c.compile_json("test://ref-no-siblings.json", src).unwrap();
        assert!(
            ir.root.all_of.is_empty(),
            "pure $ref with only metadata siblings must not produce allOf"
        );
        assert!(
            ir.root.types.integer,
            "root must be lowered from the $ref target"
        );
    }

    // ── exclusiveMinimum / exclusiveMaximum boolean ───────────────────────────

    #[test]
    fn compile_exclusive_minimum_bool_true_converts() {
        // Draft 4 / OAS 3.0 style: exclusiveMinimum: true with minimum: X
        // must compile to an exclusive bound at X.
        let mut c = Compiler::new();
        let src = r#"{"type":"number","minimum":5.0,"exclusiveMinimum":true}"#;
        let ir = c.compile_json("test://excl-min.json", src).unwrap();
        let bound = ir
            .root
            .numeric
            .minimum
            .as_ref()
            .expect("minimum bound present");
        assert!(bound.exclusive, "bound must be exclusive");
        assert!(
            (bound.value - 5.0).abs() < f64::EPSILON,
            "bound value must equal minimum"
        );
    }

    #[test]
    fn compile_exclusive_minimum_bool_false_is_inclusive() {
        // exclusiveMinimum: false means minimum is non-exclusive; the keyword
        // is dropped but minimum itself is kept as an inclusive bound.
        let mut c = Compiler::new();
        let src = r#"{"type":"number","minimum":3.0,"exclusiveMinimum":false}"#;
        let ir = c.compile_json("test://excl-min-false.json", src).unwrap();
        let bound = ir
            .root
            .numeric
            .minimum
            .as_ref()
            .expect("minimum bound present");
        assert!(!bound.exclusive, "bound must be non-exclusive");
        assert!((bound.value - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compile_exclusive_maximum_bool_true_converts() {
        // Same conversion for the upper bound.
        let mut c = Compiler::new();
        let src = r#"{"type":"number","maximum":10.0,"exclusiveMaximum":true}"#;
        let ir = c.compile_json("test://excl-max.json", src).unwrap();
        let bound = ir
            .root
            .numeric
            .maximum
            .as_ref()
            .expect("maximum bound present");
        assert!(bound.exclusive, "upper bound must be exclusive");
        assert!((bound.value - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compile_exclusive_minimum_numeric_passthrough() {
        // Draft 2020-12 style numeric exclusiveMinimum must pass through unchanged.
        let mut c = Compiler::new();
        let src = r#"{"type":"number","exclusiveMinimum":7.5}"#;
        let ir = c.compile_json("test://excl-min-num.json", src).unwrap();
        let bound = ir
            .root
            .numeric
            .minimum
            .as_ref()
            .expect("minimum bound present");
        assert!(bound.exclusive);
        assert!((bound.value - 7.5).abs() < f64::EPSILON);
    }

    // ── Unsupported keyword fail-closed ───────────────────────────────────────

    fn assert_unsupported(keyword: &str, schema_json: &str) {
        let mut c = Compiler::new();
        let result = c.compile_json("test://unsupported.json", schema_json);
        assert!(
            matches!(result, Err(CompileError::UnsupportedKeyword { keyword: ref k }) if k == keyword),
            "expected UnsupportedKeyword({keyword}) for schema {schema_json}, got: {result:?}"
        );
    }

    #[test]
    fn compile_unevaluated_properties_is_unsupported() {
        assert_unsupported(
            "unevaluatedProperties",
            r#"{"type":"object","unevaluatedProperties":false}"#,
        );
    }

    #[test]
    fn compile_unevaluated_items_is_unsupported() {
        assert_unsupported(
            "unevaluatedItems",
            r#"{"type":"array","unevaluatedItems":false}"#,
        );
    }

    #[test]
    fn compile_pattern_properties_is_unsupported() {
        assert_unsupported(
            "patternProperties",
            r#"{"type":"object","patternProperties":{"^x-":{"type":"string"}}}"#,
        );
    }

    #[test]
    fn compile_if_is_unsupported() {
        assert_unsupported("if", r#"{"if":{"type":"string"},"then":{"minLength":1}}"#);
    }

    #[test]
    fn compile_dependent_schemas_is_unsupported() {
        assert_unsupported(
            "dependentSchemas",
            r#"{"type":"object","dependentSchemas":{"a":{"required":["b"]}}}"#,
        );
    }

    #[test]
    fn compile_property_names_is_unsupported() {
        assert_unsupported(
            "propertyNames",
            r#"{"type":"object","propertyNames":{"maxLength":5}}"#,
        );
    }

    #[test]
    fn compile_contains_is_unsupported() {
        assert_unsupported(
            "contains",
            r#"{"type":"array","contains":{"type":"integer"}}"#,
        );
    }

    #[test]
    fn compile_dynamic_ref_is_unsupported() {
        assert_unsupported(
            "$dynamicRef",
            r##"{"$dynamicRef":"#items","type":"array"}"##,
        );
    }

    #[test]
    fn compile_dynamic_anchor_is_unsupported() {
        assert_unsupported(
            "$dynamicAnchor",
            r#"{"$dynamicAnchor":"items","type":"array"}"#,
        );
    }

    #[test]
    fn compile_dependent_required_is_unsupported() {
        assert_unsupported(
            "dependentRequired",
            r#"{"type":"object","dependentRequired":{"credit_card":["billing_address"]}}"#,
        );
    }

    #[test]
    fn compile_content_schema_is_unsupported() {
        assert_unsupported(
            "contentSchema",
            r#"{"type":"string","contentSchema":{"type":"object"}}"#,
        );
    }

    #[test]
    fn compile_content_media_type_is_unsupported() {
        assert_unsupported(
            "contentMediaType",
            r#"{"type":"string","contentMediaType":"application/json"}"#,
        );
    }

    #[test]
    fn compile_content_encoding_is_unsupported() {
        assert_unsupported(
            "contentEncoding",
            r#"{"type":"string","contentEncoding":"base64"}"#,
        );
    }

    #[test]
    fn compile_deep_acyclic_ref_chain_exceeds_depth_limit() {
        let mut c = Compiler::new();
        // Build a linear chain of MAX_DEPTH+2 defs (each refs the next, none
        // cyclic) to guarantee the depth bound fires rather than a stack
        // overflow.
        let chain_len = LowerCtx::MAX_DEPTH + 2;
        let mut defs = serde_json::Map::new();
        for i in 0..chain_len {
            let key = format!("def{i}");
            let val = if i + 1 < chain_len {
                serde_json::json!({ "$ref": format!("#/$defs/def{}", i + 1) })
            } else {
                serde_json::json!({ "type": "string" })
            };
            defs.insert(key, val);
        }
        let schema = serde_json::json!({ "$ref": "#/$defs/def0", "$defs": defs });
        let src = serde_json::to_string(&schema).unwrap();
        let result = c.compile_json("test://deep-chain.json", &src);
        assert!(
            matches!(result, Err(CompileError::DepthExceeded { .. })),
            "expected DepthExceeded for long acyclic $ref chain, got: {result:?}"
        );
    }
}
