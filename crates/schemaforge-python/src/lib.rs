//! Python bindings for Schemaforge.
//!
//! This crate exposes a pure-Rust API suitable for wrapping with PyO3.
//! Actual PyO3 FFI is guarded behind the `pyo3` feature flag (which requires
//! `unsafe_code` and is documented in ADR-0003).  Without the flag this crate
//! compiles as a safe Rust library providing the same logic.

use schemaforge_compiler::{CompileError, Compiler, CompilerOptions};
use schemaforge_ir::SchemaIr;
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
    /// Validation setup failed.
    #[error("schema error: {0}")]
    Schema(#[from] schemaforge_jsonschema::SchemaError),
}

/// A compiled schema handle suitable for repeated validation.
pub struct CompiledSchema {
    ir: SchemaIr,
    validator: Validator,
}

impl CompiledSchema {
    /// Compile a JSON Schema from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns [`PyBindingError`] when the schema is invalid.
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
    /// Returns `true` when valid.
    ///
    /// # Errors
    ///
    /// Returns [`PyBindingError::JsonParse`] when `instance_json` is not valid
    /// JSON.
    pub fn validate_json(&self, instance_json: &str) -> Result<bool, PyBindingError> {
        let instance: Value = serde_json::from_str(instance_json)
            .map_err(|e| PyBindingError::JsonParse(e.to_string()))?;
        Ok(self.validator.validate(&instance).is_valid())
    }

    /// Access the compiled IR.
    #[must_use]
    pub const fn ir(&self) -> &SchemaIr {
        &self.ir
    }
}

/// Validate a JSON instance against a JSON Schema (both as strings).
///
/// # Errors
///
/// Returns [`PyBindingError`] on parse or schema errors.
pub fn validate(schema_json: &str, instance_json: &str) -> Result<bool, PyBindingError> {
    let compiled = CompiledSchema::from_json(schema_json)?;
    compiled.validate_json(instance_json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_string_schema() {
        assert!(validate(r#"{"type":"string"}"#, r#""hello""#).unwrap());
        assert!(!validate(r#"{"type":"string"}"#, "42").unwrap());
    }

    #[test]
    fn compiled_schema_reuse() {
        let cs = CompiledSchema::from_json(r#"{"type":"number","minimum":0}"#).unwrap();
        assert!(cs.validate_json("5").unwrap());
        assert!(!cs.validate_json("-1").unwrap());
        assert!(!cs.validate_json(r#""text""#).unwrap());
    }

    #[test]
    fn invalid_schema_json_error() {
        let result = CompiledSchema::from_json("{broken");
        assert!(result.is_err());
    }
}
