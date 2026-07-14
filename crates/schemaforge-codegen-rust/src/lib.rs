//! Rust code generation from the Schemaforge IR.
//!
//! Translates a [`SchemaNode`] tree into Rust `struct` and `enum` definitions
//! with [`serde`] derive attributes.

use std::fmt::Write as _;

use indexmap::IndexMap;
use schemaforge_analysis::InferredNode;
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
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            extra_derives: Vec::new(),
            wrap_optional: true,
            module_doc: None,
        }
    }
}

/// Generate Rust source code from a [`SchemaIr`].
///
/// # Errors
///
/// Returns [`CodegenError`] when the IR cannot be represented in Rust.
pub fn generate(ir: &SchemaIr, options: &CodegenOptions) -> Result<String, CodegenError> {
    let inferred =
        schemaforge_analysis::analyse(&ir.root).map_err(|e| CodegenError::Unsupported {
            path: String::new(),
            reason: e.to_string(),
        })?;
    let mut buf = String::new();
    emit_header(&mut buf, options)?;
    generate_node(&ir.root, &inferred, "Root", options, &mut buf)?;
    Ok(buf)
}

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

fn generate_node(
    node: &SchemaNode,
    inferred: &InferredNode,
    name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    if inferred.never {
        emit_never_type(name, buf)?;
        return Ok(());
    }
    if inferred.types.object || !node.properties.is_empty() {
        emit_struct(node, inferred, name, options, buf)?;
    } else if !node.any_of.is_empty() || !node.one_of.is_empty() {
        emit_enum(node, name, options, buf)?;
    } else {
        emit_type_alias(inferred, name, options, buf)?;
    }
    Ok(())
}

fn emit_struct(
    node: &SchemaNode,
    inferred: &InferredNode,
    name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
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
    emit_nested_structs(node, inferred, name, options, buf)
}

fn emit_struct_fields(
    props: &IndexMap<String, InferredNode>,
    required: &[String],
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    for (key, prop_inferred) in props {
        let field_name = to_snake_case(key);
        let rust_type = inferred_to_rust_type(prop_inferred);
        let optional = options.wrap_optional && !required.contains(key);
        if field_name != *key {
            writeln!(buf, "    #[serde(rename = \"{key}\")]")?;
        }
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
    _inferred: &InferredNode,
    parent_name: &str,
    options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    for (key, prop) in &node.properties {
        let prop_name = format!("{parent_name}{}", to_pascal_case(key));
        if !prop.properties.is_empty() {
            let prop_inferred =
                schemaforge_analysis::analyse(prop).map_err(|e| CodegenError::Unsupported {
                    path: key.clone(),
                    reason: e.to_string(),
                })?;
            generate_node(prop, &prop_inferred, &prop_name, options, buf)?;
        }
    }
    Ok(())
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

fn emit_never_type(name: &str, buf: &mut String) -> Result<(), CodegenError> {
    writeln!(
        buf,
        "/// Type that can never be instantiated (schema is `false`)."
    )?;
    writeln!(buf, "pub enum {name} {{}}")?;
    buf.push('\n');
    Ok(())
}

fn emit_type_alias(
    inferred: &InferredNode,
    name: &str,
    _options: &CodegenOptions,
    buf: &mut String,
) -> Result<(), CodegenError> {
    let rust_type = inferred_to_rust_type(inferred);
    writeln!(buf, "pub type {name} = {rust_type};")?;
    buf.push('\n');
    Ok(())
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

const fn inferred_to_rust_type(inferred: &InferredNode) -> &'static str {
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
}
