//! Python bindings for Schemaforge.
//!
//! This crate exposes a pure-Rust API suitable for wrapping with PyO3.
//! Actual PyO3 FFI is guarded behind the `pyo3` feature flag. Without the
//! flag this crate compiles as a safe Rust library providing the same logic.

use schemaforge_compiler::{CompileError, Compiler, CompilerOptions};
use schemaforge_ir::{ObjectAttribute, SchemaIr};
use schemaforge_jsonschema::{ValidationOptions, Validator};
use serde_json::Value;
use thiserror::Error;

/// Error type for the Python binding layer.
#[derive(Debug, Error)]
pub enum PyBindingError {
    /// Compilation failed.
    #[error("compile error: {0}")]
    Compile(#[from] CompileError),
    /// JSON parsing failed.
    #[error("JSON parse error: {0}")]
    JsonParse(String),
    /// JSON serialization failed.
    #[error("JSON serialization error: {0}")]
    JsonSerialize(String),
    /// Validation setup failed.
    #[error("schema error: {0}")]
    Schema(#[from] schemaforge_jsonschema::SchemaError),
}

/// A compiled schema handle suitable for repeated validation and introspection.
pub struct CompiledSchema {
    ir: SchemaIr,
    validator: Validator,
}

impl CompiledSchema {
    /// Compile a JSON Schema from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PyBindingError`] when the schema is invalid JSON or fails
    /// compilation.
    pub fn from_json(schema_json: &str) -> Result<Self, PyBindingError> {
        let mut compiler = Compiler::with_options(CompilerOptions::default());
        let ir = compiler.compile_json("py://schema", schema_json)?;
        let schema_val: Value = serde_json::from_str(schema_json)
            .map_err(|e| PyBindingError::JsonParse(e.to_string()))?;
        let validator = Validator::new(&schema_val, ValidationOptions::default())?;
        Ok(Self { ir, validator })
    }

    /// Validate a JSON instance string against the compiled schema.
    ///
    /// Returns an empty `Vec` when the instance is valid, or a `Vec` of error
    /// message strings when it is invalid.
    ///
    /// # Errors
    ///
    /// Returns [`PyBindingError::JsonParse`] when `instance_json` is not valid
    /// JSON.
    pub fn validate_json(&self, instance_json: &str) -> Result<Vec<String>, PyBindingError> {
        let instance: Value = serde_json::from_str(instance_json)
            .map_err(|e| PyBindingError::JsonParse(e.to_string()))?;
        let output = self.validator.validate(&instance);
        if output.is_valid() {
            Ok(vec![])
        } else {
            Ok(output.errors.iter().map(|e| e.message.clone()).collect())
        }
    }

    /// Return descriptors for the root schema's declared JSON object fields.
    ///
    /// Each descriptor includes the source property name, requiredness,
    /// accepted JSON types, title, description, format, nested attributes, and
    /// the complete compiled child schema. Source declaration order is kept.
    #[must_use]
    pub fn object_attributes(&self) -> Vec<ObjectAttribute> {
        self.ir.object_attributes()
    }

    /// Return a single root object attribute by its JSON property name.
    #[must_use]
    pub fn object_attribute(&self, name: &str) -> Option<ObjectAttribute> {
        self.ir.object_attribute(name)
    }

    /// Return object attribute descriptors encoded as JSON.
    ///
    /// This method is intended as a simple PyO3 boundary: the native module can
    /// either convert the returned JSON to Python dictionaries or expose the
    /// string directly.
    ///
    /// # Errors
    ///
    /// Returns [`PyBindingError::JsonSerialize`] if serialization fails.
    pub fn object_attributes_json(&self) -> Result<String, PyBindingError> {
        serde_json::to_string(&self.object_attributes())
            .map_err(|e| PyBindingError::JsonSerialize(e.to_string()))
    }

    /// Access the compiled IR.
    #[must_use]
    pub const fn ir(&self) -> &SchemaIr {
        &self.ir
    }
}

/// Compile a JSON Schema string into a [`CompiledSchema`] handle.
///
/// # Errors
///
/// Returns [`PyBindingError`] when the schema string is not valid JSON or
/// fails the Schemaforge compilation pipeline.
pub fn compile_schema(schema_json: &str) -> Result<CompiledSchema, PyBindingError> {
    CompiledSchema::from_json(schema_json)
}

/// Validate a JSON instance against a JSON Schema (both as strings).
///
/// # Errors
///
/// Returns validation messages when the instance does not conform to the
/// schema, or when either argument cannot be parsed as JSON.
pub fn validate_json(schema_json: &str, instance_json: &str) -> Result<(), Vec<String>> {
    let compiled = CompiledSchema::from_json(schema_json).map_err(|e| vec![e.to_string()])?;
    let errors = compiled
        .validate_json(instance_json)
        .map_err(|e| vec![e.to_string()])?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_json_ok_on_valid_instance() {
        assert!(validate_json(r#"{"type":"string"}"#, r#""hello""#).is_ok());
    }

    #[test]
    fn validate_json_err_on_invalid_instance() {
        let errors = validate_json(r#"{"type":"string"}"#, "42").unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn validate_json_err_on_bad_instance_json() {
        let errors = validate_json(r#"{"type":"string"}"#, "{broken").unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn compile_schema_and_reuse() {
        let cs = compile_schema(r#"{"type":"number","minimum":0}"#).unwrap();
        assert!(cs.validate_json("5").unwrap().is_empty());
        assert!(!cs.validate_json("-1").unwrap().is_empty());
        assert!(!cs.validate_json(r#""text""#).unwrap().is_empty());
    }

    #[test]
    fn invalid_schema_json_error() {
        assert!(compile_schema("{broken").is_err());
    }

    #[test]
    fn compiled_schema_ir_accessible() {
        let cs = compile_schema(r#"{"type":"object"}"#).unwrap();
        let _ = cs.ir();
    }

    #[test]
    fn compiled_schema_exposes_object_attributes() {
        let cs = compile_schema(
            r#"{
                "type":"object",
                "required":["id"],
                "properties":{
                    "id":{"type":"string","format":"uuid"},
                    "profile":{
                        "type":"object",
                        "properties":{"displayName":{"type":"string"}}
                    }
                }
            }"#,
        )
        .unwrap();

        let attributes = cs.object_attributes();
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name, "id");
        assert!(attributes[0].required);
        assert_eq!(attributes[0].format.as_deref(), Some("uuid"));
        assert_eq!(attributes[1].attributes[0].name, "displayName");
        assert!(cs.object_attribute("missing").is_none());

        let json = cs.object_attributes_json().unwrap();
        assert!(json.contains("displayName"));
    }
}
