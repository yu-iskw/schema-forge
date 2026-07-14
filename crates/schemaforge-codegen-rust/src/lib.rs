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
    let mut used_names: HashSet<String> = HashSet::new();
    for (idx, (def_name, def_node)) in node.defs.iter().enumerate() {
        let raw_name = sanitize_type_name(def_name, idx);
        let type_name = unique_name(&raw_name, &mut used_names);
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
    let mut used_names: HashSet<String> = HashSet::new();
    for (idx, (key, prop_inferred)) in props.iter().enumerate() {
        let raw_name = sanitize_field_name(key, idx);
        let field_name = unique_name(&raw_name, &mut used_names);
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
    let field_list: Vec<String> = required.iter().map(|s| sanitize_for_comment(s)).collect();
    let field_list = field_list.join(", ");
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
        writeln!(buf, "/// {}", sanitize_for_comment(title))?;
    }
    if let Some(desc) = &node.description {
        for line in desc.lines() {
            writeln!(buf, "/// {}", sanitize_for_comment(line))?;
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
    // A single-type schema contains exactly one of these flags plus nothing else.
    let no_other = !t.string && !t.boolean && !t.array && !t.object && !t.null;
    if t.string && !t.number && !t.boolean && !t.array && !t.object && !t.null {
        "String"
    } else if t.number && no_other {
        // `{"type":"number"}` sets number=true AND integer=true in TypeSet
        // because number is a superset of integer. Check number before integer
        // so the presence of `number` always wins and yields f64.
        "f64"
    } else if t.integer && !t.number && no_other {
        // integer-only (number is false): i64.
        "i64"
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

// ── Collision-free name allocation ───────────────────────────────────────────

/// Return `proposed` if it is not yet in `used`, otherwise append `_1`, `_2`,
/// … until a free name is found.  The chosen name is inserted into `used`.
fn unique_name(proposed: &str, used: &mut HashSet<String>) -> String {
    if used.insert(proposed.to_owned()) {
        return proposed.to_owned();
    }
    let mut i = 1usize;
    loop {
        let candidate = format!("{proposed}_{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        i += 1;
    }
}

// ── Identifier sanitization and string escaping ──────────────────────────────

/// Replace newlines, carriage-returns, and other ASCII control characters in
/// `s` with a single space so the result is safe to embed in a Rust line
/// comment (`//` or `///`) without prematurely terminating the comment or
/// injecting code on the next source line.
fn sanitize_for_comment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c.is_ascii_control() {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Return `true` when `s` is a Rust strict keyword or reserved word that
/// cannot appear as a bare identifier in source.
fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "union"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

/// Escape a string for safe embedding inside a Rust string literal used in
/// `#[serde(rename = "...")]`.
///
/// Escapes `\`, `"`, newline, carriage-return, and tab so the resulting bytes
/// cannot break out of the attribute string literal.  Every other Unicode
/// scalar is passed through unchanged because Rust string literals accept
/// arbitrary Unicode.
fn escape_for_serde_rename(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
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
/// If the resulting identifier is a Rust keyword, it is wrapped as a raw
/// identifier (`r#keyword`) so it remains a valid field name.
fn sanitize_field_name(key: &str, idx: usize) -> String {
    let snake = to_snake_case(key);
    let safe = sanitize_identifier_chars(&snake);
    if safe.is_empty() {
        format!("field_{idx}")
    } else if is_rust_keyword(&safe) {
        format!("r#{safe}")
    } else {
        safe
    }
}

/// Derive a safe `PascalCase` Rust type identifier from a schema key.
///
/// Falls back to `Type{idx}` when no safe characters survive sanitization.
/// If the resulting identifier is a Rust keyword, a `_` suffix is appended
/// (raw-identifier syntax is not conventional for type names).
fn sanitize_type_name(key: &str, idx: usize) -> String {
    let pascal = to_pascal_case(key);
    let safe = sanitize_identifier_chars(&pascal);
    if safe.is_empty() {
        format!("Type{idx}")
    } else if is_rust_keyword(&safe) {
        format!("{safe}_")
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
    fn generate_number_alias_produces_f64() {
        // {"type":"number"} must generate f64, not i64, even though TypeSet
        // sets integer=true as a superset flag when number is present.
        let ir = simple_ir(&type_val("number"));
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("pub type Root = f64;"),
            "expected f64 for number schema, got:\n{code}"
        );
        assert!(
            !code.contains("pub type Root = i64;"),
            "i64 must not appear for number schema:\n{code}"
        );
    }

    #[test]
    fn field_name_collision_foo_bar_vs_foo_underscore_bar() {
        // "foo-bar" and "foo_bar" both sanitize to "foo_bar".
        // The second one must be renamed to "foo_bar_1" to avoid a duplicate.
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("foo-bar".to_owned(), prop.clone());
        props.insert("foo_bar".to_owned(), prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("pub foo_bar:"),
            "expected foo_bar field in:\n{code}"
        );
        assert!(
            code.contains("pub foo_bar_1:"),
            "expected collision-renamed foo_bar_1 field in:\n{code}"
        );
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
        // The serde rename attribute must be present with the double-quote escaped as `\"`.
        assert!(
            code.contains(r#"serde(rename = "field\""#),
            "expected escaped serde rename attribute in:\n{code}"
        );
    }

    #[test]
    fn doc_comment_title_with_newline_does_not_inject_code() {
        // A title containing a literal newline must NOT produce a multi-line
        // comment that causes the second line to appear as raw Rust source.
        // Use a struct (object with properties) so that emit_doc_comment is
        // actually called.
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("name".to_owned(), prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            title: Some("Evil\n; pub fn injected() {} //".to_owned()),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        let injected = code
            .lines()
            .any(|l| l.trim_start().starts_with("pub fn injected"));
        assert!(!injected, "newline in title injected code:\n{code}");
        // The title must still appear in the comment, but with newlines replaced.
        assert!(
            code.contains("Evil"),
            "title content must be present:\n{code}"
        );
    }

    #[test]
    fn field_name_type_keyword_gets_raw_identifier() {
        // A JSON property named "type" must produce `r#type` as the field name
        // so the generated struct compiles as valid Rust.
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("type".to_owned(), prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("r#type"),
            "expected raw identifier r#type in:\n{code}"
        );
    }

    #[test]
    fn field_name_self_keyword_gets_raw_identifier() {
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert("self".to_owned(), prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains("r#self"),
            "expected raw identifier r#self in:\n{code}"
        );
    }

    #[test]
    fn reserved_keywords_produce_valid_field_names() {
        // Check a sample of reserved keywords to ensure they are all wrapped.
        let keywords = ["ref", "match", "crate", "async", "fn", "let", "pub"];
        for kw in keywords {
            let prop = SchemaNode {
                types: TypeSet::from_json(&serde_json::json!("string")),
                ..SchemaNode::default()
            };
            let mut props = indexmap::IndexMap::new();
            props.insert(kw.to_owned(), prop);
            let root = SchemaNode {
                types: TypeSet::from_json(&serde_json::json!("object")),
                properties: props,
                ..SchemaNode::default()
            };
            let ir = SchemaIr::new(root, "", "abc", "test://");
            let code = generate(&ir, &CodegenOptions::default()).unwrap();
            assert!(
                code.contains(&format!("r#{kw}")),
                "expected r#{kw} for keyword property `{kw}` in:\n{code}"
            );
        }
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
