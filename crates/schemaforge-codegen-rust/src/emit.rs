//! Rust source emission: all `emit_*` helpers and the central `generate_node`
//! dispatch function.

use std::collections::HashSet;
use std::fmt::Write as _;

use indexmap::IndexMap;
use schemaforge_analysis::{
    DispatchStrategy, Representability, analyse, explain_schema, pick_variants,
};
use schemaforge_ir::SchemaNode;

use crate::names::{
    escape_for_serde_rename, sanitize_field_name, sanitize_for_comment, sanitize_type_name,
    to_snake_case, unique_name,
};
use crate::types::inferred_to_rust_type;
use crate::{CodegenError, CodegenOptions};

// ── Header ──────────────────────────────────────────────────────────────────

pub(crate) fn emit_header(buf: &mut String, options: &CodegenOptions) -> Result<(), CodegenError> {
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

pub(crate) const fn check_size_limit(
    buf: &str,
    options: &CodegenOptions,
) -> Result<(), CodegenError> {
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
pub(crate) fn emit_defs(
    node: &SchemaNode,
    options: &CodegenOptions,
    buf: &mut String,
    alloc: &mut HashSet<String>,
) -> Result<(), CodegenError> {
    for (idx, (def_name, def_node)) in node.defs.iter().enumerate() {
        let raw_name = sanitize_type_name(def_name, idx);
        let type_name = unique_name(&raw_name, alloc);
        let inferred = analyse(def_node).map_err(|e| CodegenError::Unsupported {
            path: format!("$defs/{def_name}"),
            reason: e.to_string(),
        })?;
        generate_node(def_node, &inferred, &type_name, options, buf, alloc)?;
    }
    Ok(())
}

// ── Node dispatch ────────────────────────────────────────────────────────────

pub(crate) fn generate_node(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    options: &CodegenOptions,
    buf: &mut String,
    alloc: &mut HashSet<String>,
) -> Result<(), CodegenError> {
    if inferred.never {
        return emit_never_type(name, buf);
    }
    let report = explain_schema(node);
    match report.representation {
        Representability::Unsupported => emit_never_type(name, buf),
        Representability::Nominal => {
            if inferred.types.object || !node.properties.is_empty() {
                emit_struct(
                    node,
                    inferred,
                    name,
                    report.dispatch_strategy,
                    options,
                    buf,
                    alloc,
                )
            } else {
                emit_type_alias(inferred, name, buf)
            }
        }
        Representability::Structural => emit_structural(node, inferred, name, options, buf),
        Representability::Dynamic => emit_dynamic(name, buf),
    }
}

// ── Nominal (concrete named types) ──────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn emit_struct(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    name: &str,
    dispatch: DispatchStrategy,
    options: &CodegenOptions,
    buf: &mut String,
    alloc: &mut HashSet<String>,
) -> Result<(), CodegenError> {
    // Pre-compute unique type names for all nested-object properties using the
    // shared allocator.  Doing this before emitting the struct body means field
    // declarations can reference the concrete named type rather than falling
    // back to serde_json::Value.
    let nested_type_names = compute_nested_type_names(node, name, alloc);

    let all_props: Vec<&str> = inferred.property_types.keys().map(String::as_str).collect();
    emit_dispatch_comment(node, dispatch, buf)?;
    emit_required_bitset_comment(&inferred.required_properties, &all_props, buf)?;
    emit_doc_comment(node, buf)?;
    emit_derives(options, buf)?;
    writeln!(buf, "pub struct {name} {{")?;
    emit_struct_fields(
        &inferred.property_types,
        &inferred.required_properties,
        &nested_type_names,
        options,
        buf,
    )?;
    buf.push_str("}\n\n");
    emit_nested_struct_defs(node, inferred, &nested_type_names, options, buf, alloc)
}

/// Pre-compute unique Rust type names for all nested-object properties of
/// `node`.  Names are allocated through the shared `alloc` so they cannot
/// collide with names produced by `$defs` or sibling nested structs.
fn compute_nested_type_names(
    node: &SchemaNode,
    parent_name: &str,
    alloc: &mut HashSet<String>,
) -> IndexMap<String, String> {
    let mut map = IndexMap::new();
    for (idx, (key, prop)) in node.properties.iter().enumerate() {
        if !prop.properties.is_empty() {
            let suffix = sanitize_type_name(key, idx);
            let raw_name = format!("{parent_name}{suffix}");
            let type_name = unique_name(&raw_name, alloc);
            map.insert(key.clone(), type_name);
        }
    }
    map
}

fn emit_struct_fields(
    props: &IndexMap<String, schemaforge_analysis::InferredNode>,
    required: &[String],
    nested_type_names: &IndexMap<String, String>,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    let mut used_names: HashSet<String> = HashSet::new();
    for (idx, (key, prop_inferred)) in props.iter().enumerate() {
        let raw_name = sanitize_field_name(key, idx);
        let field_name = unique_name(&raw_name, &mut used_names);
        // Use the pre-computed nested type name when the property is itself a
        // named struct; otherwise fall back to the primitive-type mapping.
        let rust_type = nested_type_names.get(key).map_or_else(
            || inferred_to_rust_type(prop_inferred).to_owned(),
            Clone::clone,
        );
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

/// Emit Rust struct definitions for all nested-object properties of `node`.
///
/// Reuses the `InferredNode` values already computed by `analyse` for the
/// parent, avoiding a second analysis pass over the same property schemas.
fn emit_nested_struct_defs(
    node: &SchemaNode,
    inferred: &schemaforge_analysis::InferredNode,
    nested_type_names: &IndexMap<String, String>,
    options: &CodegenOptions,
    buf: &mut String,
    alloc: &mut HashSet<String>,
) -> Result<(), CodegenError> {
    for (key, prop) in &node.properties {
        let Some(type_name) = nested_type_names.get(key) else {
            continue;
        };
        // Reuse the cached InferredNode from the parent to avoid re-analysing
        // the same property schema a second time.
        let prop_inferred = match inferred.property_types.get(key) {
            Some(cached) => cached.clone(),
            None => analyse(prop).map_err(|e| CodegenError::Unsupported {
                path: key.clone(),
                reason: e.to_string(),
            })?,
        };
        generate_node(prop, &prop_inferred, type_name, options, buf, alloc)?;
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
    let variants = pick_variants(node);
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
