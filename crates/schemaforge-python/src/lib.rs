//! Python bindings for Schemaforge.
//!
//! This crate exposes a pure-Rust API suitable for wrapping with PyO3.
//! Actual PyO3 FFI is guarded behind the `pyo3` feature flag (which requires
//! `unsafe_code` and is documented in ADR-0003).  Without the flag this crate
//! compiles as a safe Rust library providing the same logic.
//!
//! Validation uses [`schemaforge_bindings::CompiledSchema`] which constructs a
//! [`schemaforge_jsonschema::Validator`] directly — no IR compilation pass.
//!
//! # Public API
//!
//! ```rust
//! use schemaforge_python::{compile_schema, validate_json};
//!
//! // One-shot validation returning errors as strings
//! validate_json(r#"{"type":"string"}"#, r#""hello""#).unwrap();
//!
//! // Compile once, validate many times
//! let cs = compile_schema(r#"{"type":"number"}"#).unwrap();
//! assert!(cs.validate_json("42").unwrap().is_empty());
//! ```

pub use schemaforge_bindings::{BindingError as PyBindingError, CompiledSchema};

/// Compile a JSON Schema string into a [`CompiledSchema`] handle.
///
/// This is the preferred entry point when validating the same schema against
/// multiple instances.
///
/// # Errors
///
/// Returns [`PyBindingError`] when the schema string is not valid JSON or the
/// validator cannot be constructed from the schema.
pub fn compile_schema(schema_json: &str) -> Result<CompiledSchema, PyBindingError> {
    CompiledSchema::from_json(schema_json)
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
    schemaforge_bindings::validate_json(schema_json, instance_json)
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
}
