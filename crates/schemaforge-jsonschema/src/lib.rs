//! JSON Schema Draft 2020-12 validator for Schemaforge.
//!
//! # Usage
//!
//! ```rust
//! use schemaforge_jsonschema::{Validator, ValidationOptions};
//! use serde_json::json;
//!
//! let schema = json!({"type": "string", "minLength": 1});
//! let validator = Validator::new(&schema, ValidationOptions::default()).unwrap();
//!
//! assert!(validator.validate(&json!("hello")).is_valid());
//! assert!(!validator.validate(&json!("")).is_valid());
//! ```

pub(crate) mod applicator;
pub(crate) mod core;
pub(crate) mod unevaluated;
pub(crate) mod validation;

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use regex::Regex;
use schemaforge_formats::FormatRegistry;
use serde_json::{Map, Value};
use thiserror::Error;

/// Maximum schema evaluation depth before aborting with an error.
const MAX_DEPTH: u32 = 128;

/// Error returned when a schema cannot be compiled.
#[derive(Debug, Error)]
pub enum SchemaError {
    /// The schema JSON is malformed.
    #[error("schema parse error: {0}")]
    ParseError(String),
    /// An unsupported or invalid keyword value was encountered.
    #[error("invalid schema keyword `{keyword}`: {reason}")]
    InvalidKeyword {
        /// The keyword name.
        keyword: String,
        /// Why it is invalid.
        reason: String,
    },
    /// A `$anchor` name appears more than once in the schema document.
    #[error("duplicate $anchor name: `{name}`")]
    DuplicateAnchor {
        /// The anchor name that appeared more than once.
        name: String,
    },
    /// A keyword that is not yet supported was found in the schema.
    #[error("unsupported keyword `{keyword}` in schema (not implemented)")]
    UnsupportedKeyword {
        /// The keyword name.
        keyword: String,
    },
}

/// Options that control how the validator is built and behaves.
#[derive(Debug, Clone, Default)]
pub struct ValidationOptions {
    /// Base URI for the root schema (used for `$id` resolution).
    pub base_uri: String,
    /// Whether format assertions are enabled (vs. annotation-only).
    pub assert_formats: bool,
}

/// The result of validating a single instance against a schema.
#[derive(Debug, Clone)]
pub struct ValidationOutput {
    /// All validation errors, if any.
    pub errors: Vec<ValidationError>,
}

impl ValidationOutput {
    /// Returns `true` when the instance is valid (no errors).
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Merge another output into this one (used for composing applicators).
    pub(crate) fn merge(&mut self, other: Self) {
        self.errors.extend(other.errors);
    }

    pub(crate) const fn ok() -> Self {
        Self { errors: Vec::new() }
    }

    pub(crate) fn fail(error: ValidationError) -> Self {
        Self {
            errors: vec![error],
        }
    }
}

/// A single validation error.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// JSON Pointer path to the failing instance location.
    pub instance_path: String,
    /// JSON Pointer to the failing keyword in the schema.
    pub keyword_path: String,
    /// Human-readable error message.
    pub message: String,
}

impl ValidationError {
    pub(crate) fn new(
        instance_path: impl Into<String>,
        keyword_path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            instance_path: instance_path.into(),
            keyword_path: keyword_path.into(),
            message: message.into(),
        }
    }
}

/// A compiled, ready-to-use JSON Schema validator.
pub struct Validator {
    schema: Value,
    options: ValidationOptions,
    /// Pre-loaded additional schemas keyed by their `$id`.
    registry: HashMap<String, Value>,
    formats: FormatRegistry,
    /// Per-document `$anchor` tables.
    ///
    /// The root document's anchors are stored under the empty string key `""`.
    /// When `ValidationOptions::base_uri` is non-empty the root anchors are
    /// also mirrored under that URI so that self-referencing via the full URI
    /// resolves correctly.  Each schema added via [`Validator::add_schema`]
    /// has its anchors stored under the id passed to that method.
    anchors_by_doc: HashMap<String, HashMap<String, Value>>,
    /// Pre-compiled regexes for every `pattern` and `patternProperties` key
    /// found in the schema tree.  Keyed by the raw pattern string.
    patterns: HashMap<String, Regex>,
}

impl Validator {
    /// Compile a new validator from a JSON Schema value.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the schema is invalid, contains an
    /// invalid regular expression in a `pattern` or `patternProperties` key,
    /// contains duplicate `$anchor` names, or uses an unsupported keyword
    /// such as `$dynamicRef`, `$recursiveRef`, or `dependencies`.
    pub fn new(schema: &Value, options: ValidationOptions) -> Result<Self, SchemaError> {
        let mut formats = FormatRegistry::with_defaults();
        if options.assert_formats {
            formats.assert_all();
        }
        check_for_unsupported_keywords(schema)?;
        let root_anchors = collect_anchors(schema)?;
        let mut anchors_by_doc: HashMap<String, HashMap<String, Value>> = HashMap::new();
        if !options.base_uri.is_empty() {
            anchors_by_doc.insert(options.base_uri.clone(), root_anchors.clone());
        }
        anchors_by_doc.insert(String::new(), root_anchors);
        let mut patterns = HashMap::new();
        collect_patterns_recursive(schema, &mut patterns)?;
        // Insert the root schema under `base_uri` so that absolute self-refs
        // such as `"$ref": "https://example.com/schema.json#anchor"` resolve
        // against the root document rather than failing with "not found".
        let mut registry = HashMap::new();
        if !options.base_uri.is_empty() {
            registry.insert(options.base_uri.clone(), schema.clone());
        }
        Ok(Self {
            schema: schema.clone(),
            options,
            registry,
            formats,
            anchors_by_doc,
            patterns,
        })
    }

    /// Add an additional schema to the validator's internal registry.
    ///
    /// Patterns from the added schema are precompiled and merged into the
    /// validator's pattern cache so they are available during validation.
    ///
    /// `$anchor` names found in the added schema are collected into a
    /// per-document anchor table keyed by `id`.  Duplicate anchor names
    /// within the same document are rejected with
    /// [`SchemaError::DuplicateAnchor`].  The same anchor name may appear
    /// in different documents without conflict; anchors are always looked up
    /// against the document that owns the `$ref`, preventing accidental
    /// cross-document resolution.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the added schema contains an invalid
    /// regular expression, unsupported keywords, or duplicate `$anchor`
    /// names within the same document.
    pub fn add_schema(&mut self, id: impl Into<String>, schema: Value) -> Result<(), SchemaError> {
        let id_str = id.into();
        check_for_unsupported_keywords(&schema)?;
        collect_patterns_recursive(&schema, &mut self.patterns)?;
        let doc_anchors = collect_anchors(&schema)?;
        self.anchors_by_doc.insert(id_str.clone(), doc_anchors);
        self.registry.insert(id_str, schema);
        Ok(())
    }

    /// Validate `instance` against the compiled schema.
    ///
    /// Returns a [`ValidationOutput`] describing all errors (if any).
    #[must_use]
    pub fn validate(&self, instance: &Value) -> ValidationOutput {
        let ctx = ValidationContext {
            registry: &self.registry,
            formats: &self.formats,
            base_uri: &self.options.base_uri,
            root_schema: &self.schema,
            anchors_by_doc: &self.anchors_by_doc,
            patterns: &self.patterns,
            depth: Cell::new(0),
        };
        validate_schema(&self.schema, instance, "", &ctx)
    }

    /// Parse a JSON string and validate it.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::ParseError`] when the JSON is malformed.
    pub fn validate_str(&self, json: &str) -> Result<ValidationOutput, SchemaError> {
        let instance =
            serde_json::from_str(json).map_err(|e| SchemaError::ParseError(e.to_string()))?;
        Ok(self.validate(&instance))
    }
}

// ── Schema-child keyword sets ─────────────────────────────────────────────────
//
// Construction-time walks (anchor collection, unsupported-keyword checks,
// pattern precompilation) must only descend into *schema*-valued keywords.
// Non-schema annotations such as `default`, `const`, `enum`, `examples`,
// `title`, and `description` are plain JSON values; recursing into them would
// falsely register anchors, block unsupported-keyword checks, or reject valid
// literal values that contain regex-like strings.

/// Keywords whose value is a single sub-schema.
const SCHEMA_SINGLE_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "contains",
    "contentSchema",
    "else",
    "if",
    "items",
    "not",
    "propertyNames",
    "then",
    "unevaluatedItems",
    "unevaluatedProperties",
];

/// Keywords whose value is an array of sub-schemas.
const SCHEMA_ARRAY_KEYWORDS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];

/// Keywords whose value is an object mapping names to sub-schemas.
const SCHEMA_MAP_KEYWORDS: &[&str] = &[
    "$defs",
    "definitions",
    "dependentSchemas",
    "patternProperties",
    "properties",
];

/// Call `f` for each immediately reachable child schema of `obj`.
///
/// Only structural keywords are visited.  Non-schema annotations
/// (`default`, `const`, `enum`, `examples`, `title`, `description`, …)
/// are intentionally skipped.
fn recurse_into_schema_children<E, F>(obj: &Map<String, Value>, mut f: F) -> Result<(), E>
where
    F: FnMut(&Value) -> Result<(), E>,
{
    for &key in SCHEMA_SINGLE_KEYWORDS {
        if let Some(v) = obj.get(key) {
            f(v)?;
        }
    }
    for &key in SCHEMA_ARRAY_KEYWORDS {
        if let Some(Value::Array(arr)) = obj.get(key) {
            for item in arr {
                f(item)?;
            }
        }
    }
    for &key in SCHEMA_MAP_KEYWORDS {
        if let Some(Value::Object(map)) = obj.get(key) {
            for v in map.values() {
                f(v)?;
            }
        }
    }
    Ok(())
}

// ── Anchor collection ─────────────────────────────────────────────────────────

/// Walk the schema tree once and collect the `$anchor` registry.
///
/// Returns `Err(SchemaError::DuplicateAnchor)` when the same anchor name
/// appears more than once in the document.
fn collect_anchors(schema: &Value) -> Result<HashMap<String, Value>, SchemaError> {
    let mut anchors = HashMap::new();
    collect_anchors_recursive(schema, &mut anchors)?;
    Ok(anchors)
}

fn collect_anchors_recursive(
    schema: &Value,
    anchors: &mut HashMap<String, Value>,
) -> Result<(), SchemaError> {
    let Value::Object(obj) = schema else {
        return Ok(());
    };
    if let Some(Value::String(name)) = obj.get("$anchor") {
        if anchors.contains_key(name.as_str()) {
            return Err(SchemaError::DuplicateAnchor { name: name.clone() });
        }
        anchors.insert(name.clone(), schema.clone());
    }
    recurse_into_schema_children(obj, |child| collect_anchors_recursive(child, anchors))
}

// ── Unsupported keyword rejection ─────────────────────────────────────────────

/// Keywords rejected at construction because their semantics are not yet
/// implemented (dynamic/recursive refs, legacy `dependencies`).
///
/// Callers receive [`SchemaError::UnsupportedKeyword`] rather than silently
/// wrong validation results.
const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "$dynamicRef",
    "$dynamicAnchor",
    "$recursiveRef",
    "$recursiveAnchor",
    "dependencies",
];

/// Reject schemas that contain keywords not yet supported by this validator.
fn check_for_unsupported_keywords(schema: &Value) -> Result<(), SchemaError> {
    let Value::Object(obj) = schema else {
        return Ok(());
    };
    check_unsupported_in_object(obj)
}

fn check_unsupported_in_object(obj: &Map<String, Value>) -> Result<(), SchemaError> {
    for &keyword in UNSUPPORTED_KEYWORDS {
        if obj.contains_key(keyword) {
            return Err(SchemaError::UnsupportedKeyword {
                keyword: keyword.to_owned(),
            });
        }
    }
    recurse_into_schema_children(obj, check_for_unsupported_keywords)
}

// ── Pattern precompilation ────────────────────────────────────────────────────

/// Walk the schema tree and precompile every regex found in `pattern` and
/// `patternProperties` keys, storing them in `map` keyed by pattern string.
///
/// Returns `Err` immediately when any pattern fails to compile, so the caller
/// can surface a [`SchemaError`] rather than silently ignoring the invalid regex.
fn collect_patterns_recursive(
    schema: &Value,
    map: &mut HashMap<String, Regex>,
) -> Result<(), SchemaError> {
    let Value::Object(obj) = schema else {
        return Ok(());
    };
    collect_patterns_from_object(obj, map)
}

fn collect_patterns_from_object(
    obj: &Map<String, Value>,
    map: &mut HashMap<String, Regex>,
) -> Result<(), SchemaError> {
    register_pattern(obj.get("pattern").and_then(Value::as_str), map)?;
    if let Some(pp) = obj.get("patternProperties").and_then(Value::as_object) {
        for k in pp.keys() {
            register_pattern(Some(k.as_str()), map)?;
        }
    }
    recurse_into_schema_children(obj, |child| collect_patterns_recursive(child, map))
}

fn register_pattern(
    pattern: Option<&str>,
    map: &mut HashMap<String, Regex>,
) -> Result<(), SchemaError> {
    let Some(p) = pattern else { return Ok(()) };
    if map.contains_key(p) {
        return Ok(());
    }
    match Regex::new(p) {
        Ok(re) => {
            map.insert(p.to_owned(), re);
            Ok(())
        }
        Err(e) => Err(SchemaError::InvalidKeyword {
            keyword: "pattern".to_owned(),
            reason: format!("invalid regex `{p}`: {e}"),
        }),
    }
}

/// Return the set of property names explicitly declared in `properties`.
///
/// Shared by the applicator (`additionalProperties`) and unevaluated
/// (`unevaluatedProperties`) vocabularies.  A [`HashSet`] keeps per-key
/// membership checks O(1) when validating objects with many properties.
pub(crate) fn collect_known_property_names(obj: &Map<String, Value>) -> HashSet<&str> {
    obj.get("properties")
        .and_then(Value::as_object)
        .map(|p| p.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// Shared context passed through recursive validation calls.
pub(crate) struct ValidationContext<'a> {
    pub(crate) registry: &'a HashMap<String, Value>,
    pub(crate) formats: &'a FormatRegistry,
    pub(crate) base_uri: &'a str,
    /// The root schema document (used for local `$ref` JSON Pointer resolution).
    pub(crate) root_schema: &'a Value,
    /// Per-document `$anchor` tables.  Root doc is keyed by `""`.  External
    /// docs are keyed by the id passed to [`Validator::add_schema`].
    pub(crate) anchors_by_doc: &'a HashMap<String, HashMap<String, Value>>,
    /// Pre-compiled regexes keyed by pattern string.
    pub(crate) patterns: &'a HashMap<String, Regex>,
    /// Current evaluation nesting depth — prevents stack overflows on cyclic
    /// schemas such as `{"$ref": "#"}`.  Uses interior mutability so the
    /// signature of `validate_schema` stays `&ValidationContext`.
    pub(crate) depth: Cell<u32>,
}

/// Validate `instance` against `schema` at `path`.
pub(crate) fn validate_schema(
    schema: &Value,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
) -> ValidationOutput {
    let depth = ctx.depth.get();
    if depth >= MAX_DEPTH {
        return ValidationOutput::fail(ValidationError::new(
            path,
            path,
            format!("schema evaluation exceeded maximum nesting depth of {MAX_DEPTH}"),
        ));
    }
    ctx.depth.set(depth + 1);
    let out = match schema {
        Value::Bool(true) => ValidationOutput::ok(),
        Value::Bool(false) => ValidationOutput::fail(ValidationError::new(
            path,
            path,
            "schema is `false` - no instance is valid",
        )),
        Value::Object(obj) => validate_object_schema(obj, instance, path, ctx),
        _ => ValidationOutput::fail(ValidationError::new(
            path,
            path,
            "invalid schema: a JSON Schema must be a boolean or an object",
        )),
    };
    ctx.depth.set(depth);
    out
}

fn validate_object_schema(
    obj: &serde_json::Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
) -> ValidationOutput {
    let mut out = ValidationOutput::ok();
    core::apply(obj, instance, path, ctx, &mut out);
    applicator::apply(obj, instance, path, ctx, &mut out);
    validation::apply(obj, instance, path, ctx, &mut out);
    unevaluated::apply(obj, instance, path, ctx, &mut out);
    out
}

/// Parse JSON text into a schema and create a validator.
///
/// # Errors
///
/// Returns [`SchemaError`] when `json` is not valid JSON or the schema is invalid.
pub fn from_str(json: &str) -> Result<Validator, SchemaError> {
    let schema: Value =
        serde_json::from_str(json).map_err(|e| SchemaError::ParseError(e.to_string()))?;
    Validator::new(&schema, ValidationOptions::default())
}

/// Quickly check whether `instance` satisfies `schema`.
#[must_use]
pub fn is_valid(schema: &Value, instance: &Value) -> bool {
    Validator::new(schema, ValidationOptions::default())
        .is_ok_and(|v| v.validate(instance).is_valid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid(schema: &Value, instance: &Value) -> bool {
        Validator::new(schema, ValidationOptions::default())
            .unwrap()
            .validate(instance)
            .is_valid()
    }

    #[test]
    fn boolean_schema_true() {
        assert!(valid(&json!(true), &json!(42)));
        assert!(valid(&json!(true), &json!(null)));
    }

    #[test]
    fn boolean_schema_false() {
        assert!(!valid(&json!(false), &json!(42)));
    }

    #[test]
    fn type_string() {
        let s = json!({"type": "string"});
        assert!(valid(&s, &json!("hello")));
        assert!(!valid(&s, &json!(42)));
    }

    #[test]
    fn type_integer() {
        let s = json!({"type": "integer"});
        assert!(valid(&s, &json!(1)));
        assert!(!valid(&s, &json!(1.5)));
        assert!(!valid(&s, &json!("1")));
    }

    #[test]
    fn type_array() {
        let s = json!({"type": ["string", "null"]});
        assert!(valid(&s, &json!("hi")));
        assert!(valid(&s, &json!(null)));
        assert!(!valid(&s, &json!(42)));
    }

    #[test]
    fn enum_keyword() {
        let s = json!({"enum": ["foo", "bar", 1]});
        assert!(valid(&s, &json!("foo")));
        assert!(valid(&s, &json!(1)));
        assert!(!valid(&s, &json!("baz")));
    }

    #[test]
    fn const_keyword() {
        let s = json!({"const": 42});
        assert!(valid(&s, &json!(42)));
        assert!(!valid(&s, &json!(43)));
    }

    #[test]
    fn string_length() {
        let s = json!({"type": "string", "minLength": 2, "maxLength": 5});
        assert!(valid(&s, &json!("hi")));
        assert!(valid(&s, &json!("hello")));
        assert!(!valid(&s, &json!("h")));
        assert!(!valid(&s, &json!("toolong")));
    }

    #[test]
    fn required_properties() {
        let s = json!({"type": "object", "required": ["name"]});
        assert!(valid(&s, &json!({"name": "Alice"})));
        assert!(!valid(&s, &json!({"age": 30})));
    }

    #[test]
    fn all_of() {
        let s = json!({"allOf": [{"type": "string"}, {"minLength": 3}]});
        assert!(valid(&s, &json!("foo")));
        assert!(!valid(&s, &json!("hi")));
        assert!(!valid(&s, &json!(42)));
    }

    #[test]
    fn any_of() {
        let s = json!({"anyOf": [{"type": "string"}, {"type": "number"}]});
        assert!(valid(&s, &json!("hi")));
        assert!(valid(&s, &json!(42)));
        assert!(!valid(&s, &json!(null)));
    }

    #[test]
    fn one_of() {
        let s = json!({"oneOf": [{"type": "string"}, {"minLength": 3}]});
        assert!(!valid(&s, &json!("foo")));
        assert!(valid(&s, &json!("hi")));
    }

    #[test]
    fn not_keyword() {
        let s = json!({"not": {"type": "string"}});
        assert!(valid(&s, &json!(42)));
        assert!(!valid(&s, &json!("hi")));
    }

    #[test]
    fn properties_keyword() {
        let s = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            }
        });
        assert!(valid(&s, &json!({"name": "Alice", "age": 30})));
        assert!(!valid(&s, &json!({"name": 42})));
    }

    #[test]
    fn items_keyword() {
        let s = json!({"type": "array", "items": {"type": "string"}});
        assert!(valid(&s, &json!(["a", "b"])));
        assert!(!valid(&s, &json!(["a", 1])));
    }

    #[test]
    fn min_max_number() {
        let s = json!({"type": "number", "minimum": 0, "maximum": 100});
        assert!(valid(&s, &json!(50)));
        assert!(!valid(&s, &json!(-1)));
        assert!(!valid(&s, &json!(101)));
    }

    #[test]
    fn is_valid_helper() {
        assert!(is_valid(&json!({"type": "string"}), &json!("hi")));
        assert!(!is_valid(&json!({"type": "string"}), &json!(42)));
    }

    #[test]
    fn property_names_keyword() {
        let s = json!({"propertyNames": {"maxLength": 3}});
        assert!(valid(&s, &json!({"ab": 1, "cd": 2})));
        assert!(!valid(&s, &json!({"toolong": 1})));
    }

    #[test]
    fn dependent_schemas_keyword() {
        let s = json!({
            "dependentSchemas": {
                "credit_card": {
                    "required": ["billing_address"]
                }
            }
        });
        assert!(valid(&s, &json!({"name": "Alice"})));
        assert!(valid(
            &s,
            &json!({"credit_card": "1234", "billing_address": "123 Main"})
        ));
        assert!(!valid(&s, &json!({"credit_card": "1234"})));
    }

    #[test]
    fn ref_to_defs() {
        let schema = json!({
            "$defs": {"Name": {"type": "string"}},
            "properties": {"name": {"$ref": "#/$defs/Name"}}
        });
        assert!(valid(&schema, &json!({"name": "Alice"})));
        assert!(!valid(&schema, &json!({"name": 42})));
    }

    #[test]
    fn dynamic_anchor_and_ref_rejected_at_construction() {
        // $dynamicAnchor and $dynamicRef are not implemented: Validator::new
        // must return Err rather than silently producing wrong results.
        let schema = json!({
            "$defs": {
                "Item": {
                    "$dynamicAnchor": "item",
                    "type": "string"
                }
            },
            "type": "array",
            "items": { "$dynamicRef": "#item" }
        });
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with $dynamicAnchor/$dynamicRef must fail at construction"
        );
    }

    fn assert_no_panic(schema: &Value, instance: &Value) {
        let v = Validator::new(schema, ValidationOptions::default()).unwrap();
        let _ = v.validate(instance);
    }

    #[test]
    fn prop_deeply_nested_array() {
        let schema =
            json!({"type": "array", "items": {"type": "array", "items": {"type": "integer"}}});
        let instances = [
            json!([]),
            json!([[]]),
            json!([[1, 2, 3], [4, 5]]),
            json!([[1, "oops"], []]),
        ];
        for inst in &instances {
            assert_no_panic(&schema, inst);
        }
    }

    #[test]
    fn prop_large_object() {
        let schema = json!({"type": "object", "additionalProperties": {"type": "integer"}});
        let mut obj = serde_json::Map::new();
        for i in 0_i64..50 {
            obj.insert(format!("field{i}"), json!(i));
        }
        assert_no_panic(&schema, &Value::Object(obj.clone()));
        obj.insert("bad".to_owned(), json!("not-an-int"));
        assert_no_panic(&schema, &Value::Object(obj));
    }

    #[test]
    fn prop_empty_string_and_unicode() {
        let schema = json!({"type": "string", "minLength": 0});
        let long_str = "x".repeat(1024);
        let instances = [
            json!(""),
            json!("a"),
            json!("hello, world!"),
            json!("\u{0000}"),
            Value::String(long_str),
        ];
        for inst in &instances {
            assert_no_panic(&schema, inst);
        }
    }

    #[test]
    fn prop_boolean_schema_never_panics() {
        let instances = [
            json!(null),
            json!(true),
            json!(0),
            json!(""),
            json!([]),
            json!({}),
        ];
        for inst in &instances {
            let vt = Validator::new(&json!(true), ValidationOptions::default()).unwrap();
            let vf = Validator::new(&json!(false), ValidationOptions::default()).unwrap();
            let _ = vt.validate(inst);
            let _ = vf.validate(inst);
        }
    }

    #[test]
    fn prop_invalid_json_via_validate_str() {
        let schema = json!({"type": "string"});
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        for s in ["not json at all", "{unclosed", "NaN"] {
            let _ = v.validate_str(s);
        }
    }

    #[test]
    fn unresolved_ref_fails_validation() {
        let schema = json!({"$ref": "https://example.com/missing-schema"});
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        assert!(!v.validate(&json!("anything")).is_valid());
    }

    #[test]
    fn dynamic_ref_rejected_at_construction() {
        // A schema with only $dynamicRef (no $dynamicAnchor) must also fail at
        // construction because $dynamicRef is unsupported.
        let schema = json!({"$dynamicRef": "#missing-anchor"});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with $dynamicRef must fail at construction"
        );
    }

    // ── Recursion / cycle guards ──────────────────────────────────────────────

    #[test]
    fn cyclic_ref_does_not_stack_overflow() {
        // {"$ref": "#"} recurses into the root schema forever without a depth
        // budget.  The validator must return an error instead of overflowing.
        let schema = json!({"$ref": "#"});
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        let out = v.validate(&json!("anything"));
        assert!(
            !out.is_valid(),
            "cyclic $ref should produce a validation error, not succeed"
        );
    }

    #[test]
    fn deep_all_of_does_not_stack_overflow() {
        // Build an allOf chain nested MAX_DEPTH + a few levels beyond the limit.
        let limit = MAX_DEPTH as usize + 5;
        let mut schema = json!({"type": "string"});
        for _ in 0..limit {
            schema = json!({"allOf": [schema]});
        }
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        let out = v.validate(&json!("hello"));
        // Whether valid or error, the important thing is no stack overflow.
        let _ = out;
    }

    #[test]
    fn deep_all_of_within_budget_is_valid() {
        // An allOf chain that stays inside MAX_DEPTH must validate normally.
        let mut schema = json!({"type": "string"});
        for _ in 0..10_usize {
            schema = json!({"allOf": [schema]});
        }
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        assert!(v.validate(&json!("hello")).is_valid());
        assert!(!v.validate(&json!(42)).is_valid());
    }

    #[test]
    fn allof_failure_instance_path_is_instance_not_schema_path() {
        // A required-field failure inside an allOf branch must report the
        // instance path (the object location), not the schema path
        // ("/allOf/0").  This ensures callers can locate the failing value
        // in the instance rather than in the schema.
        let schema = json!({
            "allOf": [
                {"required": ["name"]}
            ]
        });
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        let out = v.validate(&json!({}));
        assert!(!out.is_valid());
        for err in &out.errors {
            assert!(
                !err.instance_path.contains("allOf"),
                "instance_path must not contain 'allOf': got {:?}",
                err.instance_path
            );
        }
    }

    #[test]
    fn allof_nested_field_failure_instance_path_reflects_field() {
        // A failure on a nested field under allOf must report the field's
        // instance path, e.g. "/name", not "/allOf/0/name".
        let schema = json!({
            "type": "object",
            "allOf": [
                {
                    "properties": {
                        "name": {"type": "string"}
                    }
                }
            ]
        });
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        let out = v.validate(&json!({"name": 42}));
        assert!(!out.is_valid());
        let has_name_path = out
            .errors
            .iter()
            .any(|e| e.instance_path == "/name" || e.instance_path.ends_with("/name"));
        assert!(
            has_name_path,
            "expected an error at /name, got: {:#?}",
            out.errors
        );
    }

    // ── Invalid pattern guards ────────────────────────────────────────────────

    #[test]
    fn invalid_pattern_rejected_at_build_time() {
        // An invalid regex must cause Validator::new to return Err.
        let schema = json!({"pattern": "[invalid"});
        let result = Validator::new(&schema, ValidationOptions::default());
        assert!(
            result.is_err(),
            "Validator::new should fail on invalid pattern"
        );
    }

    #[test]
    fn invalid_pattern_in_pattern_properties_rejected_at_build_time() {
        let schema = json!({"patternProperties": {"[invalid": {"type": "string"}}});
        let result = Validator::new(&schema, ValidationOptions::default());
        assert!(
            result.is_err(),
            "Validator::new should fail on invalid patternProperties key"
        );
    }

    #[test]
    fn valid_pattern_still_works() {
        let schema = json!({"type": "string", "pattern": "^[a-z]+$"});
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        assert!(v.validate(&json!("hello")).is_valid());
        assert!(!v.validate(&json!("Hello")).is_valid());
    }

    #[test]
    fn non_schema_number_fails_closed() {
        // A schema that is a JSON number (not bool or object) must reject every
        // instance rather than silently accepting them.
        let schema = serde_json::json!(42);
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        assert!(
            !v.validate(&serde_json::json!("anything")).is_valid(),
            "a numeric schema must not validate any instance"
        );
    }

    #[test]
    fn non_schema_string_fails_closed() {
        let schema = serde_json::json!("not-a-schema");
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        assert!(
            !v.validate(&serde_json::json!(null)).is_valid(),
            "a string schema must not validate any instance"
        );
    }

    #[test]
    fn non_schema_array_fails_closed() {
        let schema = serde_json::json!([1, 2, 3]);
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        assert!(
            !v.validate(&serde_json::json!({})).is_valid(),
            "an array schema must not validate any instance"
        );
    }

    #[test]
    fn add_schema_fails_on_invalid_pattern() {
        let root = json!({"type": "string"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let bad = json!({"pattern": "[bad"});
        assert!(
            v.add_schema("urn:bad", bad).is_err(),
            "add_schema should fail on invalid pattern"
        );
    }

    // ── Schema-walk only descends schema keywords (fix #1) ────────────────────

    #[test]
    fn anchor_in_default_not_registered() {
        // $anchor inside a `default` value is a plain JSON annotation, not a
        // schema sub-keyword.  It must NOT be collected into the anchor table.
        let schema = json!({
            "$defs": {
                "Str": {
                    "type": "string",
                    "default": {"$anchor": "ghost"}
                }
            },
            "type": "object"
        });
        // Construction must succeed (no duplicate-anchor error triggered by `default`).
        let v = Validator::new(&schema, ValidationOptions::default())
            .expect("$anchor inside default must not cause a construction error");
        // A ref that tries to reach the phantom anchor must fail validation
        // (unresolved), proving the anchor was never registered.
        let ref_schema = json!({"$ref": "#ghost"});
        let rv = Validator::new(&ref_schema, ValidationOptions::default()).unwrap();
        assert!(
            !rv.validate(&json!("anything")).is_valid(),
            "$anchor inside `default` must not be registered"
        );
        // Original schema still validates normally.
        assert!(v.validate(&json!({})).is_valid());
    }

    #[test]
    fn dynamic_ref_in_const_does_not_fail_construction() {
        // $dynamicRef inside a `const` value is a literal JSON value, not a
        // schema keyword usage.  Construction must succeed.
        let schema = json!({"const": {"$dynamicRef": "#foo"}});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_ok(),
            "schema with $dynamicRef inside a const value must be accepted at construction"
        );
    }

    #[test]
    fn invalid_pattern_in_const_does_not_fail_construction() {
        // An invalid regex string inside a `const` value is not a `pattern`
        // keyword; construction must succeed.
        let schema = json!({"const": {"pattern": "[invalid"}});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_ok(),
            "invalid pattern string inside const value must not fail construction"
        );
    }

    // ── per-document anchor isolation ─────────────────────────────────────────

    #[test]
    fn external_anchor_resolves_via_external_uri_ref() {
        // An anchor in an external schema must be reachable only through the
        // external document URI (e.g. `"$ref": "urn:ext#myAnchor"`), not via a
        // root-fragment ref (`"$ref": "#myAnchor"`).
        let root = json!({"$ref": "urn:ext#myAnchor"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let external = json!({
            "$defs": {
                "Str": {"$anchor": "myAnchor", "type": "string"}
            }
        });
        v.add_schema("urn:ext", external).unwrap();
        assert!(
            v.validate(&json!("hello")).is_valid(),
            "external anchor reachable via `urn:ext#myAnchor`"
        );
        assert!(
            !v.validate(&json!(42)).is_valid(),
            "anchor schema constraints must be applied"
        );
    }

    #[test]
    fn foreign_anchor_not_reachable_via_root_ref() {
        // `"$ref": "#name"` looks up anchors in the root document only.
        // An anchor defined solely in an external schema must NOT resolve.
        let root = json!({"$ref": "#myAnchor"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let external = json!({
            "$defs": {
                "Str": {"$anchor": "myAnchor", "type": "string"}
            }
        });
        v.add_schema("urn:ext", external).unwrap();
        // The ref is unresolved, so validation must fail.
        assert!(
            !v.validate(&json!("hello")).is_valid(),
            "root #anchor ref must not reach anchor defined only in external schema"
        );
    }

    #[test]
    fn same_anchor_name_in_different_docs_is_allowed() {
        // The same anchor name may appear in the root and in an external
        // document without conflict; each is scoped to its own document.
        let root = json!({"$anchor": "shared", "type": "string"});
        let mut v = Validator::new(&root, ValidationOptions::default()).unwrap();
        let ext = json!({"$anchor": "shared", "type": "integer"});
        assert!(
            v.add_schema("urn:ext", ext).is_ok(),
            "same anchor name in different documents must not be rejected"
        );
    }

    // ── $anchor collision ─────────────────────────────────────────────────────

    #[test]
    fn duplicate_anchor_rejected_at_construction() {
        // Two schemas with the same $anchor name must cause Validator::new to
        // return SchemaError::DuplicateAnchor.
        let schema = json!({
            "$defs": {
                "A": {"$anchor": "shared", "type": "string"},
                "B": {"$anchor": "shared", "type": "integer"}
            }
        });
        let result = Validator::new(&schema, ValidationOptions::default());
        assert!(
            result.is_err(),
            "duplicate $anchor names must be rejected at construction"
        );
        let err = result.err().expect("already checked is_err");
        match err {
            SchemaError::DuplicateAnchor { name } => assert_eq!(name, "shared"),
            other => panic!("expected DuplicateAnchor, got {other}"),
        }
    }

    #[test]
    fn unique_anchors_accepted() {
        let schema = json!({
            "$defs": {
                "A": {"$anchor": "alpha", "type": "string"},
                "B": {"$anchor": "beta", "type": "integer"}
            }
        });
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_ok(),
            "unique $anchor names must be accepted"
        );
    }

    // ── $dynamicRef / $dynamicAnchor unsupported ──────────────────────────────

    #[test]
    fn dynamic_anchor_alone_rejected_at_construction() {
        let schema = json!({"$dynamicAnchor": "root", "type": "object"});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with $dynamicAnchor must fail at construction"
        );
    }

    #[test]
    fn dynamic_ref_nested_rejected_at_construction() {
        // $dynamicRef nested inside $defs must still be caught.
        let schema = json!({
            "$defs": {
                "Leaf": {"$dynamicRef": "#root"}
            },
            "type": "object"
        });
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "nested $dynamicRef must be rejected at construction"
        );
    }

    // ── $recursiveRef / $recursiveAnchor unsupported ──────────────────────────

    #[test]
    fn recursive_ref_rejected_at_construction() {
        // $recursiveRef carries runtime anchor-resolution semantics not yet
        // implemented; the validator must refuse rather than silently misbehave.
        let schema = json!({"$recursiveRef": "#"});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with $recursiveRef must fail at construction"
        );
    }

    #[test]
    fn recursive_anchor_rejected_at_construction() {
        let schema = json!({"$recursiveAnchor": true, "type": "object"});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with $recursiveAnchor must fail at construction"
        );
    }

    #[test]
    fn recursive_ref_nested_rejected_at_construction() {
        // $recursiveRef nested inside allOf must still be caught.
        let schema = json!({
            "allOf": [
                {"$recursiveRef": "#"}
            ]
        });
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "nested $recursiveRef must be rejected at construction"
        );
    }

    // ── `dependencies` (legacy Draft 4/7) unsupported ────────────────────────

    #[test]
    fn dependencies_rejected_at_construction() {
        // The legacy `dependencies` keyword conflates two distinct semantics
        // that the validator does not yet implement; reject explicitly.
        let schema = json!({"dependencies": {"foo": ["bar"]}});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema with `dependencies` must fail at construction"
        );
    }

    #[test]
    fn dependencies_schema_form_rejected_at_construction() {
        // `dependencies` with a sub-schema value (not an array) must also be
        // rejected.
        let schema = json!({"dependencies": {"foo": {"required": ["bar"]}}});
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "schema-form `dependencies` must fail at construction"
        );
    }

    #[test]
    fn dependencies_nested_rejected_at_construction() {
        // `dependencies` nested inside a sub-schema must still be caught.
        let schema = json!({
            "properties": {
                "inner": {"dependencies": {"a": ["b"]}}
            }
        });
        assert!(
            Validator::new(&schema, ValidationOptions::default()).is_err(),
            "nested `dependencies` must be rejected at construction"
        );
    }
}
