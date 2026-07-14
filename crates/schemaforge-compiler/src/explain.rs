//! Schema explanation helpers: representation strategy and codegen decisions.

use schemaforge_analysis::InferredNode;
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
    let inferred = schemaforge_analysis::analyse(root).ok();
    ExplainResult {
        dialect_uri: ir.dialect_uri.clone(),
        type_strategy: describe_type_strategy(root.types),
        nullable: root.types.null,
        property_count: root.properties.len(),
        combinator_count: count_combinators(root),
        codegen_hints: make_codegen_hints(root, inferred.as_ref()),
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
    let mut parts = Vec::new();
    if types.object {
        parts.push("object");
    }
    if types.array {
        parts.push("array");
    }
    if types.string {
        parts.push("string");
    }
    if types.number {
        parts.push("number");
    }
    if types.boolean {
        parts.push("boolean");
    }
    if types.null {
        parts.push("null");
    }
    parts.join("|")
}

fn make_codegen_hints(node: &SchemaNode, inferred: Option<&InferredNode>) -> Vec<String> {
    let mut hints = base_codegen_hints(node);
    if let Some(inf) = inferred {
        append_analysis_hints(inf, &mut hints);
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
    use schemaforge_ir::{SchemaIr, SchemaNode, TypeSet};

    fn make_ir(root: SchemaNode) -> SchemaIr {
        SchemaIr::new(
            root,
            "https://json-schema.org/draft/2020-12/schema",
            "digest",
            "test://s",
        )
    }

    #[test]
    fn explain_any_schema() {
        let ir = make_ir(SchemaNode::any());
        let r = explain_ir(&ir);
        assert_eq!(r.type_strategy, "any");
        assert!(
            r.codegen_hints
                .contains(&"schema accepts any value".to_owned())
        );
    }

    #[test]
    fn explain_never_schema() {
        let ir = make_ir(SchemaNode::boolean_schema(false));
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
        let ir = make_ir(node);
        let r = explain_ir(&ir);
        assert_eq!(r.type_strategy, "object");
        assert!(r.codegen_hints.contains(&"generates struct".to_owned()));
    }
}
