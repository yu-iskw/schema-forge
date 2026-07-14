//! Node.js bindings for Schemaforge.
//!
//! This crate exposes a pure-Rust API suitable for wrapping with napi-rs.
//! Actual napi-rs FFI is guarded behind the `napi` feature flag (which
//! requires `unsafe_code` and is documented in ADR-0003). Without the flag
//! this crate compiles as a safe Rust library.

use schemaforge_compiler::{CompileError, Compiler, CompilerOptions};
use schemaforge_ir::SchemaIr;
use schemaforge_jsonschema::{ValidationOptions, ValidationOutput, Validator};
use serde_json::Value;
use thiserror::Error;

/// Error type for the Node.js binding layer.
#[derive(Debug, Error)]
pub enum NodeBindingError {
    /// Compilation failed.
    #[error("compile error: {0}")]
    Compile(#[from] CompileError),
    /// JSON parsing failed.
    #[error("JSON parse error: {0}")]
    JsonParse(String),
    /// Validation setup failed.
    #[error("schema error: {0}")]
    Schema(#[from] schemaforge_jsonschema::SchemaError),
}

/// A compiled schema handle for use from Node.js.
pub struct JsCompiledSchema {
    ir: SchemaIr,
    validator: Validator,
}

impl JsCompiledSchema {
    /// Compile a JSON Schema from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`NodeBindingError`] when the schema is invalid.
    pub fn from_json(schema_json: &str) -> Result<Self, NodeBindingError> {
        let mut compiler = Compiler::with_options(CompilerOptions::default());
        let ir = compiler.compile_json("node://schema", schema_json)?;
        let schema_val: Value = serde_json::from_str(schema_json)
            .map_err(|e| NodeBindingError::JsonParse(e.to_string()))?;
        let validator = Validator::new(&schema_val, ValidationOptions::default())?;
        Ok(Self { ir, validator })
    }

    /// Validate a JSON instance string.
    ///
    /// Returns a [`ValidationOutput`] with the result and any errors.
    ///
    /// # Errors
    ///
    /// Returns [`NodeBindingError::JsonParse`] when `instance_json` is not
    /// valid JSON.
    pub fn validate_json(&self, instance_json: &str) -> Result<ValidationOutput, NodeBindingError> {
        let instance: Value = serde_json::from_str(instance_json)
            .map_err(|e| NodeBindingError::JsonParse(e.to_string()))?;
        Ok(self.validator.validate(&instance))
    }

    /// Access the compiled IR.
    #[must_use]
    pub const fn ir(&self) -> &SchemaIr {
        &self.ir
    }
}

/// Validate JSON instance against a JSON Schema, returning `true` when valid.
///
/// # Errors
///
/// Returns [`NodeBindingError`] on parse or schema errors.
pub fn validate(schema_json: &str, instance_json: &str) -> Result<bool, NodeBindingError> {
    let cs = JsCompiledSchema::from_json(schema_json)?;
    Ok(cs.validate_json(instance_json)?.is_valid())
}

/// Validate and return all errors as JSON strings.
///
/// # Errors
///
/// Returns [`NodeBindingError`] on parse or schema errors.
pub fn validate_with_errors(
    schema_json: &str,
    instance_json: &str,
) -> Result<Vec<String>, NodeBindingError> {
    let cs = JsCompiledSchema::from_json(schema_json)?;
    let output = cs.validate_json(instance_json)?;
    Ok(output.errors.iter().map(|e| e.message.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_number_schema() {
        assert!(validate(r#"{"type":"number"}"#, "3.14").unwrap());
        assert!(!validate(r#"{"type":"number"}"#, r#""text""#).unwrap());
    }

    #[test]
    fn validate_with_errors_reports_messages() {
        let errors = validate_with_errors(r#"{"type":"string","minLength":5}"#, r#""hi""#).unwrap();
        assert!(!errors.is_empty());
    }

    #[test]
    fn compiled_schema_ir_accessible() {
        let cs = JsCompiledSchema::from_json(r#"{"type":"object"}"#).unwrap();
        assert!(cs.ir().root.types.object);
    }
}
