//! Node.js bindings for Schemaforge.
//!
//! This crate exposes a pure-Rust API suitable for wrapping with napi-rs.
//! Actual napi-rs FFI is guarded behind the `napi` feature flag (which
//! requires `unsafe_code` and is documented in ADR-0003). Without the flag
//! this crate compiles as a safe Rust library.
//!
//! # Public API
//!
//! ```rust
//! use schemaforge_node::{compile_schema, validate_json};
//!
//! // One-shot validation returning errors as strings
//! validate_json(r#"{"type":"string"}"#, r#""hello""#).unwrap();
//!
//! // Compile once, validate many times
//! let cs = compile_schema(r#"{"type":"number"}"#).unwrap();
//! assert!(cs.validate_json("42").unwrap().is_empty());
//! ```

use schemaforge_compiler::{CompileError, Compiler, CompilerOptions};
use schemaforge_ir::SchemaIr;
use schemaforge_jsonschema::{ValidationOptions, Validator};
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
    /// Returns [`NodeBindingError`] when the schema is invalid JSON or fails
    /// compilation.
    pub fn from_json(schema_json: &str) -> Result<Self, NodeBindingError> {
        let mut compiler = Compiler::with_options(CompilerOptions::default());
        let ir = compiler.compile_json("node://schema", schema_json)?;
        let schema_val: Value = serde_json::from_str(schema_json)
            .map_err(|e| NodeBindingError::JsonParse(e.to_string()))?;
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
    /// Returns [`NodeBindingError::JsonParse`] when `instance_json` is not
    /// valid JSON.
    pub fn validate_json(&self, instance_json: &str) -> Result<Vec<String>, NodeBindingError> {
        let instance: Value = serde_json::from_str(instance_json)
            .map_err(|e| NodeBindingError::JsonParse(e.to_string()))?;
        let output = self.validator.validate(&instance);
        if output.is_valid() {
            Ok(vec![])
        } else {
            Ok(output.errors.iter().map(|e| e.message.clone()).collect())
        }
    }

    /// Access the compiled IR.
    #[must_use]
    pub const fn ir(&self) -> &SchemaIr {
        &self.ir
    }
}

/// Compile a JSON Schema string into a [`JsCompiledSchema`] handle.
///
/// This is the preferred entry point when validating the same schema against
/// multiple instances.
///
/// # Errors
///
/// Returns [`NodeBindingError`] when the schema string is not valid JSON or
/// fails the Schemaforge compilation pipeline.
pub fn compile_schema(schema_json: &str) -> Result<JsCompiledSchema, NodeBindingError> {
    JsCompiledSchema::from_json(schema_json)
}

/// Validate a JSON instance against a JSON Schema (both as strings).
///
/// Returns `Ok(())` when the instance is valid.  Returns
/// `Err(errors)` where `errors` is a `Vec<String>` of human-readable
/// validation error messages when the instance is invalid.
///
/// # Errors
///
/// Returns `Err` with validation error messages when the instance does not
/// conform to the schema, or when either argument cannot be parsed as JSON.
pub fn validate_json(schema_json: &str, instance_json: &str) -> Result<(), Vec<String>> {
    let cs = JsCompiledSchema::from_json(schema_json).map_err(|e| vec![e.to_string()])?;
    let errors = cs
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
        assert!(validate_json(r#"{"type":"number"}"#, "3.14").is_ok());
    }

    #[test]
    fn validate_json_err_on_invalid_instance() {
        let errors = validate_json(r#"{"type":"number"}"#, r#""text""#).unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn validate_json_err_reports_messages() {
        let errors = validate_json(r#"{"type":"string","minLength":5}"#, r#""hi""#).unwrap_err();
        assert!(!errors.is_empty());
    }

    #[test]
    fn compile_schema_and_reuse() {
        let cs = compile_schema(r#"{"type":"object"}"#).unwrap();
        assert!(cs.validate_json("{}").unwrap().is_empty());
        assert!(!cs.validate_json("42").unwrap().is_empty());
    }

    #[test]
    fn compiled_schema_ir_accessible() {
        let cs = compile_schema(r#"{"type":"object"}"#).unwrap();
        assert!(cs.ir().root.types.object);
    }

    #[test]
    fn invalid_schema_json_error() {
        assert!(compile_schema("{broken").is_err());
    }
}
