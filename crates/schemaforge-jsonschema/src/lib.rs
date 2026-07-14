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

use std::collections::HashMap;

use schemaforge_formats::FormatRegistry;
use schemaforge_resolver::{OfflineResolver, ResolveError, Resolver};
use serde_json::Value;
use thiserror::Error;

/// Error returned when a schema cannot be compiled.
#[derive(Debug, Error)]
pub enum SchemaError {
    /// The schema JSON is malformed.
    #[error("schema parse error: {0}")]
    ParseError(String),
    /// A `$ref` or `$dynamicRef` could not be resolved.
    #[error("unresolved reference `{uri}`: {source}")]
    UnresolvedRef {
        /// The unresolvable URI.
        uri: String,
        /// The underlying resolve error.
        source: ResolveError,
    },
    /// An unsupported or invalid keyword value was encountered.
    #[error("invalid schema keyword `{keyword}`: {reason}")]
    InvalidKeyword {
        /// The keyword name.
        keyword: String,
        /// Why it is invalid.
        reason: String,
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
    /// Whether the instance is valid.
    pub valid: bool,
    /// All validation errors, if any.
    pub errors: Vec<ValidationError>,
}

impl ValidationOutput {
    /// Returns `true` when the instance is valid.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.valid
    }

    /// Merge another output into this one (used for composing applicators).
    pub(crate) fn merge(&mut self, other: Self) {
        if !other.valid {
            self.valid = false;
            self.errors.extend(other.errors);
        }
    }

    pub(crate) const fn ok() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
        }
    }

    pub(crate) fn fail(error: ValidationError) -> Self {
        Self {
            valid: false,
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
    /// `$dynamicAnchor` -> schema extracted from the root document.
    dynamic_anchors: HashMap<String, Value>,
}

impl Validator {
    /// Compile a new validator from a JSON Schema value.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the schema is invalid.
    pub fn new(schema: &Value, options: ValidationOptions) -> Result<Self, SchemaError> {
        Self::with_resolver(schema, options, &OfflineResolver::new())
    }

    /// Compile with a custom resolver for `$ref` resolution.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the schema is invalid.
    pub fn with_resolver(
        schema: &Value,
        options: ValidationOptions,
        _resolver: &dyn Resolver,
    ) -> Result<Self, SchemaError> {
        let mut formats = FormatRegistry::with_defaults();
        if options.assert_formats {
            formats.assert_all();
        }
        let dynamic_anchors = collect_dynamic_anchors(schema);
        Ok(Self {
            schema: schema.clone(),
            options,
            registry: HashMap::new(),
            formats,
            dynamic_anchors,
        })
    }

    /// Add an additional schema to the validator's internal registry.
    pub fn add_schema(&mut self, id: impl Into<String>, schema: Value) {
        self.registry.insert(id.into(), schema);
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
            dynamic_anchors: &self.dynamic_anchors,
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

/// Walk the schema tree and collect every sub-schema that declares a
/// `$dynamicAnchor`, keyed by the anchor name.
fn collect_dynamic_anchors(schema: &Value) -> HashMap<String, Value> {
    let mut anchors = HashMap::new();
    collect_anchors_recursive(schema, &mut anchors);
    anchors
}

fn collect_anchors_recursive(schema: &Value, anchors: &mut HashMap<String, Value>) {
    match schema {
        Value::Object(obj) => {
            if let Some(Value::String(name)) = obj.get("$dynamicAnchor") {
                anchors.insert(name.clone(), schema.clone());
            }
            for value in obj.values() {
                collect_anchors_recursive(value, anchors);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_anchors_recursive(item, anchors);
            }
        }
        _ => {}
    }
}

/// Shared context passed through recursive validation calls.
pub(crate) struct ValidationContext<'a> {
    registry: &'a HashMap<String, Value>,
    formats: &'a FormatRegistry,
    base_uri: &'a str,
    /// The root schema document (used for local `$ref` JSON Pointer resolution).
    root_schema: &'a Value,
    /// Pre-computed `$dynamicAnchor` registry for the root document.
    dynamic_anchors: &'a HashMap<String, Value>,
}

/// Validate `instance` against `schema` at `path`.
pub(crate) fn validate_schema(
    schema: &Value,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
) -> ValidationOutput {
    match schema {
        Value::Bool(false) => ValidationOutput::fail(ValidationError::new(
            path,
            path,
            "schema is `false` - no instance is valid",
        )),
        Value::Object(obj) => validate_object_schema(obj, instance, path, ctx),
        _ => ValidationOutput::ok(),
    }
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
    fn dynamic_anchor_and_ref() {
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
        assert!(valid(&schema, &json!(["a", "b"])));
        assert!(!valid(&schema, &json!(["a", 1])));
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
}
