//! Schema explanation helpers: representation strategy and codegen decisions.

use schemaforge_analysis::{InferredNode, explain_schema};
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
#[must_use]
pub fn explain_ir(ir: &SchemaIr) -> ExplainResult {
    let root = &ir.root;
    let analysis_report = explain_schema(root);
    let (inferred, analysis_error) = match schemaforge_analysis::analyse(root) {
        Ok(inf) => (Some(inf), None),
        Err(e) => (None, Some(format!("analysis error: {e}"))),
    };
    ExplainResult {
        dialect_uri: ir.dialect_uri.clone(),
        type_strategy: describe_type_strategy(root.types),
        nullable: root.types.null,
        property_count: root.properties.len(),
        combinator_count: count_combinators(root),
        codegen_hints: make_codegen_hints(
            root,
            inferred.as_ref(),
            analysis_report.fallback_reasons,
            analysis_error,
        ),
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

fn make_codegen_hints(
    node: &SchemaNode,
    inferred: Option<&InferredNode>,
    fallback_reasons: Vec<String>,
    analysis_error: Option<String>,
) -> Vec<String> {
    let mut hints = base_codegen_hints(node);
    if let Some(inf) = inferred {
        append_analysis_hints(inf, &mut hints);
    }
    hints.extend(fallback_reasons);
    if let Some(err) = analysis_error {
        hints.push(err);
    }
    hints
}

fn base_codegen_hints(node: &SchemaNode) -> Vec<String> {
    let mut hints = Vec::new();
    if node.types.object && !node.properties.is_empty() {
        hints.push("generates struct".to_owned());
    }
    if !node.enum_values.is_empty() {
        hints.push("generates enum".to_owned());
    }
    if node.types.array {
        hints.push("generates Vec<T>".to_owned());
    }
    hints
}

fn append_analysis_hints(inf: &InferredNode, hints: &mut Vec<String>) {
    if inf.nullable {
        hints.push("fields wrapped in Option<T>".to_owned());
    }
    if inf.never {
        hints.push("schema is never (always fails)".to_owned());
    }
    if inf.any {
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
