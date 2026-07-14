//! Rust code generation from the Schemaforge IR.
//!
//! Translates a [`SchemaNode`] tree into Rust `struct` and `enum` definitions
//! with [`serde`] derive attributes.
//!
//! # Key entry points
//!
//! - [`generate`] — produce Rust source from a [`SchemaIr`].
//! - [`CodegenOptions`] — tune output (derives, optional wrapping, size limit).
//! - [`CodegenError`] — errors including [`CodegenError::SizeExceeded`].
//!
//! # Classification integration
//!
//! This crate delegates representability classification and dispatch-strategy
//! selection to [`schemaforge_analysis::explain_schema`] so that both decisions
//! are made in a single pass per node, sharing a consistent view of how each
//! schema node should be mapped to Rust.

use std::collections::HashSet;

use schemaforge_analysis::analyse;
use schemaforge_ir::SchemaIr;
use thiserror::Error;

mod emit;
mod names;
mod types;

#[cfg(test)]
mod tests;

/// Error returned during Rust code generation.
#[derive(Debug, Error)]
pub enum CodegenError {
    /// The IR contains a node that cannot be represented in Rust.
    #[error("unsupported IR node at `{path}`: {reason}")]
    Unsupported {
        /// Path to the unsupported node.
        path: String,
        /// Explanation.
        reason: String,
    },
    /// Generated output exceeds the configured byte limit.
    #[error("generated output ({actual} bytes) exceeds the configured limit ({limit} bytes)")]
    SizeExceeded {
        /// Actual number of bytes generated.
        actual: usize,
        /// Configured limit.
        limit: usize,
    },
    /// Formatting error (infallible in practice).
    #[error("format error: {0}")]
    Fmt(#[from] std::fmt::Error),
}

/// Options controlling code generation.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Derive additional traits beyond `Debug`, `Clone`, `serde::Serialize`,
    /// and `serde::Deserialize`.
    pub extra_derives: Vec<String>,
    /// When `true`, wrap all optional fields in `Option<T>`.
    pub wrap_optional: bool,
    /// Module-level doc comment prepended to the output.
    pub module_doc: Option<String>,
    /// Maximum number of bytes the generated output may contain.
    ///
    /// When `Some(n)`, [`generate`] returns [`CodegenError::SizeExceeded`] if
    /// the output exceeds `n` bytes.  `None` disables the check.
    pub max_bytes: Option<usize>,
}

/// Default output size cap: 5 MiB.  Schemas that produce more generated code
/// than this are almost certainly pathological; callers may override via
/// [`CodegenOptions::max_bytes`].
pub const DEFAULT_MAX_BYTES: usize = 5_000_000;

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            extra_derives: Vec::new(),
            wrap_optional: true,
            module_doc: None,
            max_bytes: Some(DEFAULT_MAX_BYTES),
        }
    }
}

/// Generate Rust source code from a [`SchemaIr`].
///
/// # Errors
///
/// Returns [`CodegenError`] when the IR cannot be represented in Rust or when
/// the output exceeds `options.max_bytes`.
pub fn generate(ir: &SchemaIr, options: &CodegenOptions) -> Result<String, CodegenError> {
    let inferred = analyse(&ir.root).map_err(|e| CodegenError::Unsupported {
        path: String::new(),
        reason: e.to_string(),
    })?;
    let mut buf = String::new();
    // Seed the allocator with "Root" so that any $def named "Root" is
    // automatically disambiguated and does not collide with the root struct.
    let mut alloc: HashSet<String> = HashSet::from(["Root".to_owned()]);
    emit::emit_header(&mut buf, options)?;
    emit::emit_defs(&ir.root, options, &mut buf, &mut alloc)?;
    emit::generate_node(&ir.root, &inferred, "Root", options, &mut buf, &mut alloc)?;
    emit::check_size_limit(&buf, options)?;
    Ok(buf)
}
