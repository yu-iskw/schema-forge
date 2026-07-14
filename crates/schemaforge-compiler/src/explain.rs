//! Schema explanation helpers: representation strategy and codegen decisions.

use schemaforge_analysis::{ExplainReport, Representability, explain_schema};
use schemaforge_ir::{SchemaIr, SchemaNode, TypeSet};
use serde::{Deserialize, Serialize};

/// Explanation of a schema's representation strategy and codegen decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainResult {
    /// Detected JSON Schema dialect URI.
    pub dialect_uri: String,
    /// Human-readable description of the type strategy.
    pub type_strategy: String,
    /// Whether the root schema allows `null`.
    pub nullable: bool,
    /// Number of named properties on the root object schema.
    pub property_count: usize,
    /// Total number of combinator sub-schemas (`allOf` + `anyOf` + `oneOf`).
    pub combinator_count: usize,
    /// Code-generation hints derived from the IR and analysis.
    pub codegen_hints: Vec<String>,
}

/// Explain the representation strategy for a compiled [`SchemaIr`].
///
/// Uses [`explain_schema`] as the single analysis pass; does not run the
/// full `analyse` inference separately.
#[must_use]
pub fn explain_ir(ir: &SchemaIr) -> ExplainResult {
    let root = &ir.root;
    let report = explain_schema(root);
    ExplainResult {
        dialect_uri: ir.dialect_uri.clone(),
        type_strategy: describe_type_strategy(root.types),
        nullable: root.types.null,
        property_count: root.properties.len(),
        combinator_count: count_combinators(root),
        codegen_hints: make_codegen_hints(root, &report),
    }
}

const fn count_combinators(node: &SchemaNode) -> usize {
    node.all_of.len() + node.any_of.len() + node.one_of.len()
}

fn describe_type_strategy(types: TypeSet) -> String {
    if types == TypeSet::any() {
        return "any".to_owned();
    }
    if types.is_empty() {
        return "never".to_owned();
    }
    build_type_name(types)
}

fn build_type_name(types: TypeSet) -> String {
    types.type_names().join("|")
}

fn make_codegen_hints(node: &SchemaNode, report: &ExplainReport) -> Vec<String> {
    let mut hints = base_codegen_hints(node);
    append_representation_hints(node, report.representation, &mut hints);
    hints.extend(report.fallback_reasons.iter().cloned());
    hints
}

fn base_codegen_hints(node: &SchemaNode) -> Vec<String> {
    let mut hints = Vec::new();
    if node.types.object && !node.properties.is_empty() {
        hints.push("generates struct".to_owned());
    }
    if node.enum_values.as_ref().is_some_and(|v| !v.is_empty()) {
        hints.push("generates enum".to_owned());
    }
    if node.types.array {
        hints.push("generates Vec<T>".to_owned());
    }
    hints
}

/// Append high-level representability hints derived from the [`ExplainReport`].
///
/// These replace the separate `analyse` pass by reading the coarser
/// [`Representability`] classification and the node's type flags directly.
fn append_representation_hints(
    node: &SchemaNode,
    representation: Representability,
    hints: &mut Vec<String>,
) {
    if node.types.null {
        hints.push("fields wrapped in Option<T>".to_owned());
    }
    if representation == Representability::Unsupported {
        hints.push("schema is never (always fails)".to_owned());
    }
    // Fully unconstrained dynamic schema (no union branches, no properties).
    if representation == Representability::Dynamic
        && node.any_of.is_empty()
        && node.one_of.is_empty()
        && node.properties.is_empty()
    {
        hints.push("schema accepts any value".to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_ir::{SchemaNode, TypeSet};

    #[test]
    fn explain_any_schema() {
        let ir = crate::make_test_ir(SchemaNode::any());
        let r = explain_ir(&ir);
        assert_eq!(r.type_strategy, "any");
        assert!(
            r.codegen_hints
                .contains(&"schema accepts any value".to_owned())
        );
    }

    #[test]
    fn explain_never_schema() {
        let ir = crate::make_test_ir(SchemaNode::boolean_schema(false));
        let r = explain_ir(&ir);
        assert_eq!(r.type_strategy, "never");
    }

    #[test]
    fn explain_object_schema() {
        let mut node = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            ..SchemaNode::default()
        };
        node.properties.insert(
            "id".to_owned(),
            SchemaNode {
                types: TypeSet::from_json(&serde_json::json!("string")),
                ..SchemaNode::default()
            },
        );
        let ir = crate::make_test_ir(node);
        let r = explain_ir(&ir);
        assert_eq!(r.type_strategy, "object");
        assert!(r.codegen_hints.contains(&"generates struct".to_owned()));
    }
}
