//! Shared validation helpers for Schemaforge Python and Node.js bindings.
//!
//! Both FFI crates use [`Validator`] directly — no IR compilation pass — so
//! the same JSON string is never parsed twice in the validation hot-path.
//!
//! # Public API
//!
//! ```rust
//! use schemaforge_bindings::{CompiledSchema, validate_json};
//!
//! // One-shot validation
//! validate_json(r#"{"type":"string"}"#, r#""hello""#).unwrap();
//!
//! // Compile once, validate many times
//! let cs = CompiledSchema::from_json(r#"{"type":"number"}"#).unwrap();
//! assert!(cs.validate_json("42").unwrap().is_empty());
//! ```

use schemaforge_jsonschema::{SchemaError, ValidationOptions, Validator};
use serde_json::Value;
use thiserror::Error;

/// Error returned by binding operations.
#[derive(Debug, Error)]
pub enum BindingError {
    /// JSON parsing failed.
    #[error("JSON parse error: {0}")]
    JsonParse(String),
    /// Schema construction failed.
    #[error("schema error: {0}")]
    Schema(#[from] SchemaError),
}

/// A compiled schema handle for repeated validation.
///
/// Built from a JSON Schema string via [`CompiledSchema::from_json`].  Only a
/// [`Validator`] is constructed — no IR compilation pass — so the schema JSON
/// is parsed exactly once.
pub struct CompiledSchema {
    validator: Validator,
}

impl CompiledSchema {
    /// Build a [`CompiledSchema`] from a JSON Schema string.
    ///
    /// # Errors
    ///
    /// Returns [`BindingError`] when `schema_json` is not valid JSON or the
    /// validator cannot be constructed from the schema.
    pub fn from_json(schema_json: &str) -> Result<Self, BindingError> {
        let schema_val: Value = serde_json::from_str(schema_json)
            .map_err(|e| BindingError::JsonParse(e.to_string()))?;
        let validator = Validator::new(&schema_val, ValidationOptions::default())?;
        Ok(Self { validator })
    }

    /// Validate a JSON instance string against the compiled schema.
    ///
    /// Returns an empty `Vec` when the instance is valid, or a non-empty `Vec`
    /// of human-readable error messages when it is invalid.
    ///
    /// # Errors
    ///
    /// Returns [`BindingError::JsonParse`] when `instance_json` is not valid
    /// JSON.
    pub fn validate_json(&self, instance_json: &str) -> Result<Vec<String>, BindingError> {
        let instance: Value = serde_json::from_str(instance_json)
            .map_err(|e| BindingError::JsonParse(e.to_string()))?;
        let output = self.validator.validate(&instance);
        if output.is_valid() {
            Ok(vec![])
        } else {
            Ok(output.errors.iter().map(|e| e.message.clone()).collect())
        }
    }
}

/// Validate a JSON instance against a JSON Schema (both as strings).
///
/// Returns `Ok(())` when the instance is valid.  Returns `Err(errors)` where
/// `errors` is a `Vec<String>` of human-readable messages when invalid.
///
/// # Errors
///
/// Returns `Err` with error messages when the instance does not conform to the
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
        let cs = CompiledSchema::from_json(r#"{"type":"number","minimum":0}"#).unwrap();
        assert!(cs.validate_json("5").unwrap().is_empty());
        assert!(!cs.validate_json("-1").unwrap().is_empty());
        assert!(!cs.validate_json(r#""text""#).unwrap().is_empty());
    }

    #[test]
    fn invalid_schema_json_error() {
        assert!(CompiledSchema::from_json("{broken").is_err());
    }

    #[test]
    fn compile_object_schema() {
        let cs = CompiledSchema::from_json(r#"{"type":"object"}"#).unwrap();
        assert!(cs.validate_json("{}").unwrap().is_empty());
        assert!(!cs.validate_json("42").unwrap().is_empty());
    }
}
