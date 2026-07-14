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

use std::fmt::Write as _;

use indexmap::IndexMap;
use schemaforge_analysis::{DispatchStrategy, Representability, analyse, explain_schema};
use schemaforge_ir::{SchemaIr, SchemaNode};
use thiserror::Error;

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
    emit_header(&mut buf, options)?;
    emit_defs(&ir.root, options, &mut buf)?;
    generate_node(&ir.root, &inferred, "Root", options, &mut buf)?;
    check_size_limit(&buf, options)?;
    Ok(buf)
}

// ── Header ──────────────────────────────────────────────────────────────────

fn emit_header(buf: &mut String, options: &CodegenOptions) -> Result<(), CodegenError> {
    if let Some(doc) = &options.module_doc {
        for line in doc.lines() {
            writeln!(buf, "//! {line}")?;
        }
        buf.push('\n');
    }
    buf.push_str("#![allow(clippy::all)]\n\n");
    buf.push_str("use serde::{Deserialize, Serialize};\n\n");
    Ok(())
}

// ── Size limit ───────────────────────────────────────────────────────────────

const fn check_size_limit(buf: &str, options: &CodegenOptions) -> Result<(), CodegenError> {
    let Some(limit) = options.max_bytes else {
        return Ok(());
    };
    let actual = buf.len();
    if actual > limit {
        return Err(CodegenError::SizeExceeded { actual, limit });
    }
    Ok(())
}

// ── $defs ────────────────────────────────────────────────────────────────────

/// Emit named types for all `$defs` / `definitions` entries in `node`.
///
/// Def names are converted to `PascalCase` to form Rust type identifiers.
/// This ensures that `$ref`-based types (already inlined by the IR resolver)
/// still appear as named types in the output, making the generated code more
/// readable.
fn emit_defs(
    node: &SchemaNode,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    for (idx, (def_name, def_node)) in node.defs.iter().enumerate() {
        let type_name = sanitize_type_name(def_name, idx);
        let inferred = analyse(def_node).map_err(|e| CodegenError::Unsupported {
            path: format!("$defs/{def_name}"),
            reason: e.to_string(),
        })?;
        generate_node(def_node, &inferred, &type_name, options, buf)?;
    }
    Ok(())
}

// ── Node dispatch ────────────────────────────────────────────────────────────

fn generate_node(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    if inferred.never {
        return emit_never_type(name, buf);
    }
    // Call explain_schema once to get both Representability and DispatchStrategy,
    // avoiding a separate classify call followed by another explain_schema inside
    // emit_dispatch_comment.
    let report = explain_schema(node);
    match report.representation {
        Representability::Unsupported => emit_never_type(name, buf),
        Representability::Nominal => {
            emit_nominal(node, inferred, name, options, report.dispatch_strategy, buf)
        }
        Representability::Structural => emit_structural(node, inferred, name, options, buf),
        Representability::Dynamic => emit_dynamic(name, buf),
    }
}

// ── Nominal (concrete named types) ──────────────────────────────────────────

fn emit_nominal(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    options: &CodegenOptions,
    dispatch: DispatchStrategy,
    buf: &mut String,
) -> Result<(), CodegenError> {
    if inferred.types.object || !node.properties.is_empty() {
        emit_struct(node, inferred, name, options, dispatch, buf)
    } else {
        emit_type_alias(inferred, name, buf)
    }
}

fn emit_struct(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    options: &CodegenOptions,
    dispatch: DispatchStrategy,
    buf: &mut String,
) -> Result<(), CodegenError> {
    let all_props: Vec<&str> = inferred.property_types.keys().map(String::as_str).collect();
    emit_dispatch_comment(node, dispatch, buf)?;
    emit_required_bitset_comment(&inferred.required_properties, &all_props, buf)?;
    emit_doc_comment(node, buf)?;
    emit_derives(options, buf)?;
    writeln!(buf, "pub struct {name} {{")?;
    emit_struct_fields(
        &inferred.property_types,
        &inferred.required_properties,
        options,
        buf,
    )?;
    buf.push_str("}\n\n");
    emit_nested_structs(node, name, options, buf)
}

fn emit_struct_fields(
    props: &IndexMap<String, schemaforge_analysis::InferredNode>,
    required: &[String],
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    for (idx, (key, prop_inferred)) in props.iter().enumerate() {
        let field_name = sanitize_field_name(key, idx);
        let rust_type = inferred_to_rust_type(prop_inferred);
        let optional = options.wrap_optional && !required.contains(key);
        // Always emit a properly escaped serde rename attribute so that the
        // original JSON key is never interpolated raw into the Rust source.
        let escaped_key = escape_for_serde_rename(key);
        writeln!(buf, "    #[serde(rename = \"{escaped_key}\")]")?;
        if optional {
            writeln!(buf, "    pub {field_name}: Option<{rust_type}>,")?;
        } else {
            writeln!(buf, "    pub {field_name}: {rust_type},")?;
        }
    }
    Ok(())
}

fn emit_nested_structs(
    node: &SchemaNode,
    parent_name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    for (idx, (key, prop)) in node.properties.iter().enumerate() {
        if prop.properties.is_empty() {
            continue;
        }
        let suffix = sanitize_type_name(key, idx);
        let prop_name = format!("{parent_name}{suffix}");
        let prop_inferred = analyse(prop).map_err(|e| CodegenError::Unsupported {
            path: key.clone(),
            reason: e.to_string(),
        })?;
        generate_node(prop, &prop_inferred, &prop_name, options, buf)?;
    }
    Ok(())
}

// ── Structural (arrays, open objects, homogeneous unions) ────────────────────

fn emit_structural(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    if !node.any_of.is_empty() || !node.one_of.is_empty() {
        emit_enum(node, name, options, buf)
    } else {
        emit_type_alias(inferred, name, buf)
    }
}

fn emit_enum(
    node: &SchemaNode,
    name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    emit_doc_comment(node, buf)?;
    emit_derives(options, buf)?;
    buf.push_str("#[serde(untagged)]\n");
    writeln!(buf, "pub enum {name} {{")?;
    let variants = if node.one_of.is_empty() {
        &node.any_of
    } else {
        &node.one_of
    };
    for (i, _) in variants.iter().enumerate() {
        writeln!(buf, "    Variant{i}(serde_json::Value),")?;
    }
    buf.push_str("}\n\n");
    Ok(())
}

// ── Dynamic (fully runtime, validate-only) ───────────────────────────────────

/// Emit a `serde_json::Value` type alias and a `validate_<name>_json` stub for
/// a schema that cannot be represented statically.
fn emit_dynamic(name: &str, buf: &mut String) -> Result<(), CodegenError> {
    writeln!(buf, "/// Dynamic schema: accepts any JSON value.")?;
    writeln!(buf, "pub type {name} = serde_json::Value;")?;
    buf.push('\n');
    emit_validate_json_stub(name, buf)
}

fn emit_validate_json_stub(name: &str, buf: &mut String) -> Result<(), CodegenError> {
    let snake = to_snake_case(name);
    let error_type = format!("{name}ValidationError");
    writeln!(
        buf,
        "/// Validates raw JSON bytes against the `{name}` dynamic schema."
    )?;
    writeln!(buf, "///")?;
    writeln!(buf, "/// # Errors")?;
    writeln!(buf, "///")?;
    writeln!(buf, "/// Returns `Err` when the input is not valid JSON.")?;
    writeln!(
        buf,
        "pub fn validate_{snake}_json(input: &[u8]) -> Result<(), {error_type}> {{"
    )?;
    writeln!(
        buf,
        "    serde_json::from_slice::<serde_json::Value>(input)"
    )?;
    writeln!(buf, "        .map(|_| ())")?;
    writeln!(buf, "        .map_err(|e| {error_type}(e.to_string()))")?;
    writeln!(buf, "}}")?;
    buf.push('\n');
    writeln!(buf, "/// Validation error for the `{name}` dynamic schema.")?;
    writeln!(buf, "#[derive(Debug)]")?;
    writeln!(buf, "pub struct {error_type}(pub String);")?;
    buf.push('\n');
    writeln!(buf, "impl std::fmt::Display for {error_type} {{")?;
    writeln!(
        buf,
        "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{"
    )?;
    writeln!(buf, "        write!(f, \"validation error: {{}}\", self.0)")?;
    writeln!(buf, "    }}")?;
    writeln!(buf, "}}")?;
    buf.push('\n');
    writeln!(buf, "impl std::error::Error for {error_type} {{}}")?;
    buf.push('\n');
    Ok(())
}

// ── Never ────────────────────────────────────────────────────────────────────

fn emit_never_type(name: &str, buf: &mut String) -> Result<(), CodegenError> {
    writeln!(
        buf,
        "/// Type that can never be instantiated (schema is `false`)."
    )?;
    writeln!(buf, "pub enum {name} {{}}")?;
    buf.push('\n');
    Ok(())
}

// ── Type alias ───────────────────────────────────────────────────────────────

fn emit_type_alias(
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    buf: &mut String,
) -> Result<(), CodegenError> {
    let rust_type = inferred_to_rust_type(inferred);
    writeln!(buf, "pub type {name} = {rust_type};")?;
    buf.push('\n');
    Ok(())
}

// ── Annotations / comments ───────────────────────────────────────────────────

fn emit_dispatch_comment(
    node: &SchemaNode,
    dispatch: DispatchStrategy,
    buf: &mut String,
) -> Result<(), CodegenError> {
    if node.properties.is_empty() {
        return Ok(());
    }
    let label = dispatch_label(dispatch);
    writeln!(buf, "// Property dispatch: {label}")?;
    Ok(())
}

const fn dispatch_label(strategy: DispatchStrategy) -> &'static str {
    match strategy {
        DispatchStrategy::LengthFirst => {
            "length-first (all property keys have distinct byte lengths)"
        }
        DispatchStrategy::ExactMatch => "exact-match (linear key comparison)",
        DispatchStrategy::TaggedUnion => "tagged-union (variant discrimination)",
        DispatchStrategy::RuntimeAny => "runtime-any (fully dynamic)",
    }
}

fn emit_required_bitset_comment(
    required: &[String],
    all_props: &[&str],
    buf: &mut String,
) -> Result<(), CodegenError> {
    if required.is_empty() {
        return Ok(());
    }
    let bitset = compute_required_bitset(required, all_props);
    let field_list = required.join(", ");
    writeln!(
        buf,
        "// Required-field bitset: {bitset:#066b} (required: {field_list})"
    )?;
    Ok(())
}

fn compute_required_bitset(required: &[String], all_props: &[&str]) -> u64 {
    let mut bitset: u64 = 0;
    for (i, prop) in all_props.iter().enumerate().take(64) {
        if required.iter().any(|r| r == *prop) {
            bitset |= 1u64 << i;
        }
    }
    bitset
}

fn emit_doc_comment(node: &SchemaNode, buf: &mut String) -> Result<(), CodegenError> {
    if let Some(title) = &node.title {
        writeln!(buf, "/// {title}")?;
    }
    if let Some(desc) = &node.description {
        for line in desc.lines() {
            writeln!(buf, "/// {line}")?;
        }
    }
    Ok(())
}

fn emit_derives(options: &CodegenOptions, buf: &mut String) -> Result<(), CodegenError> {
    let mut derives = vec!["Debug", "Clone", "Serialize", "Deserialize"];
    let extra: Vec<&str> = options.extra_derives.iter().map(String::as_str).collect();
    derives.extend(extra);
    writeln!(buf, "#[derive({})]", derives.join(", "))?;
    Ok(())
}

// ── Type mapping ─────────────────────────────────────────────────────────────

const fn inferred_to_rust_type(inferred: &schemaforge_analysis::InferredNode) -> &'static str {
    if inferred.any || inferred.never {
        return "serde_json::Value";
    }
    let t = &inferred.types;
    if t.string && !t.number && !t.boolean && !t.array && !t.object && !t.null {
        "String"
    } else if t.integer && !t.string && !t.boolean && !t.array && !t.object && !t.null {
        "i64"
    } else if t.number && !t.string && !t.boolean && !t.array && !t.object && !t.null {
        "f64"
    } else if t.boolean && !t.string && !t.number && !t.array && !t.object && !t.null {
        "bool"
    } else {
        "serde_json::Value"
    }
}

// ── Case conversion ──────────────────────────────────────────────────────────

/// Convert `camelCase` or mixed-case to `snake_case`.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.extend(ch.to_lowercase());
    }
    result.replace(['-', ' '], "_")
}

/// Convert to `PascalCase`.
fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|p| !p.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

// ── Identifier sanitization and string escaping ──────────────────────────────

/// Escape a string for safe embedding inside a Rust string literal used in
/// `#[serde(rename = "...")]`.
///
/// Every ASCII character that is not an alphanumeric character, `-`, or `_`
/// is encoded as `\u{NNNN}`.  This prevents both premature string termination
/// (`"`) and accidental inclusion of recognisable Rust code fragments in the
/// raw generated source text (e.g. `pub fn`, `; evil()`, `\n` newlines).
///
/// Non-ASCII Unicode scalars pass through unchanged because they cannot form
/// ASCII keyword sequences and are valid in Rust string literals.
fn escape_for_serde_rename(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        if ch.is_ascii() && !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
            write!(out, "\\u{{{:04x}}}", ch as u32).unwrap_or(());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Strip every character that is not `[A-Za-z0-9_]` from a string that is
/// intended to become a Rust identifier.
///
/// If the first remaining character is an ASCII digit, a leading `_` is added
/// so the result is a valid identifier.  Returns an empty string when no safe
/// characters remain (callers must supply a fallback).
fn sanitize_identifier_chars(s: &str) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if filtered.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{filtered}")
    } else {
        filtered
    }
}

/// Derive a safe `snake_case` Rust field identifier from a JSON property key.
///
/// Falls back to `field_{idx}` when no safe characters survive sanitization.
fn sanitize_field_name(key: &str, idx: usize) -> String {
    let snake = to_snake_case(key);
    let safe = sanitize_identifier_chars(&snake);
    if safe.is_empty() {
        format!("field_{idx}")
    } else {
        safe
    }
}

/// Derive a safe `PascalCase` Rust type identifier from a schema key.
///
/// Falls back to `Type{idx}` when no safe characters survive sanitization.
fn sanitize_type_name(key: &str, idx: usize) -> String {
    let pascal = to_pascal_case(key);
    let safe = sanitize_identifier_chars(&pascal);
    if safe.is_empty() {
        format!("Type{idx}")
    } else {
        safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_ir::{SchemaIr, SchemaNode, TypeSet};

    fn type_val(s: &str) -> serde_json::Value {
        serde_json::json!(s)
    }

    fn simple_ir(type_json: &serde_json::Value) -> SchemaIr {
        let root = SchemaNode {
            types: TypeSet::from_json(type_json),
            ..SchemaNode::default()
        };
        SchemaIr::new(
            root,
            "https://json-schema.org/draft/2020-12/schema",
            "abc123",
            "test://",
        )
    }

    #[test]
    fn generate_string_alias() {
        let ir = simple_ir(&type_val("string"));
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(code.contains("pub type Root = String;"));
    }

    #[test]
    fn generate_integer_alias() {
        let ir = simple_ir(&type_val("integer"));
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(code.contains("pub type Root = i64;"));
    }

    #[test]
    fn generate_struct_with_properties() {
        let name_prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("name".to_owned(), name_prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            object: schemaforge_ir::ObjectConstraints {
                required: vec!["name".to_owned()],
                ..Default::default()
            },
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(code.contains("pub struct Root"));
        assert!(code.contains("pub name: String"));
    }

    #[test]
    fn snake_case_conversion() {
        assert_eq!(to_snake_case("camelCase"), "camel_case");
        assert_eq!(to_snake_case("snake_case"), "snake_case");
        assert_eq!(to_snake_case("kebab-case"), "kebab_case");
    }

    #[test]
    fn pascal_case_conversion() {
        assert_eq!(to_pascal_case("foo_bar"), "FooBar");
        assert_eq!(to_pascal_case("hello-world"), "HelloWorld");
    }

    #[test]
    fn dynamic_schema_emits_validator() {
        // An unconstrained schema (TypeSet::any, no properties) → Dynamic.
        let root = SchemaNode::default();
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("pub fn validate_root_json"),
            "expected validate_root_json in:\n{code}"
        );
        assert!(code.contains("pub type Root = serde_json::Value;"));
        assert!(code.contains("RootValidationError"));
    }

    #[test]
    fn dynamic_schema_validator_is_callable() {
        let root = SchemaNode::default();
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(code.contains("serde_json::from_slice::<serde_json::Value>(input)"));
    }

    #[test]
    fn nominal_struct_has_required_bitset_comment() {
        let name_prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("id".to_owned(), name_prop.clone());
        props.insert("label".to_owned(), name_prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            object: schemaforge_ir::ObjectConstraints {
                required: vec!["id".to_owned()],
                ..Default::default()
            },
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("Required-field bitset:"),
            "expected bitset comment in:\n{code}"
        );
        assert!(code.contains("required: id"));
    }

    #[test]
    fn nominal_struct_has_dispatch_comment() {
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("id".to_owned(), prop.clone());
        props.insert("name".to_owned(), prop.clone());
        props.insert("value".to_owned(), prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("Property dispatch:"),
            "expected dispatch comment in:\n{code}"
        );
    }

    #[test]
    fn max_bytes_exceeded_returns_error() {
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let opts = CodegenOptions {
            max_bytes: Some(1),
            ..CodegenOptions::default()
        };
        let result = generate(&ir, &opts);
        assert!(
            matches!(result, Err(CodegenError::SizeExceeded { .. })),
            "expected SizeExceeded but got: {result:?}"
        );
    }

    #[test]
    fn max_bytes_within_limit_succeeds() {
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let opts = CodegenOptions {
            max_bytes: Some(usize::MAX),
            ..CodegenOptions::default()
        };
        assert!(generate(&ir, &opts).is_ok());
    }

    #[test]
    fn defs_types_are_emitted() {
        let string_def = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut defs = indexmap::IndexMap::new();
        defs.insert("my-id".to_owned(), string_def);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            defs,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        // "my-id" → PascalCase → "MyId"
        assert!(
            code.contains("pub type MyId"),
            "expected MyId type in:\n{code}"
        );
    }

    #[test]
    fn generate_never_schema() {
        let root = SchemaNode::boolean_schema(false);
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(code.contains("pub enum Root {}"));
    }

    #[test]
    fn malicious_key_with_quotes_and_newlines_does_not_inject() {
        // A property key containing `"` and a literal newline must not break
        // out of the `#[serde(rename = "...")]` attribute and must not appear
        // as raw Rust code in the generated output.
        let malicious_key = "field\"\n; pub fn evil() {} //".to_owned();
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert(malicious_key, prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        // If the newline were NOT escaped, the key would break out of the
        // attribute string and `pub fn evil()` would appear as a standalone
        // definition on its own source line.  Check that no line starts with
        // `pub fn evil` (after trimming leading whitespace) to detect injection.
        let injected = code
            .lines()
            .any(|l| l.trim_start().starts_with("pub fn evil"));
        assert!(!injected, "injection detected in generated code:\n{code}");
        // The serde rename attribute must be present with the quote unicode-escaped.
        assert!(
            code.contains(r#"serde(rename = "field\u{0022}"#),
            "expected escaped serde rename attribute in:\n{code}"
        );
    }

    #[test]
    fn key_with_only_special_chars_gets_fallback_field_name() {
        // A key consisting entirely of characters outside [A-Za-z0-9_] must
        // produce a valid fallback identifier (`field_0`) rather than an empty
        // or invalid Rust identifier.
        let special_key = "!!!".to_owned();
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert(special_key, prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("pub field_0"),
            "expected fallback field name `field_0` in:\n{code}"
        );
    }
}
