//! Schema inspection helpers: dialect, node count, and capabilities.

use schemaforge_ir::{SchemaIr, SchemaNode};
use serde::{Deserialize, Serialize};

/// Summary produced by inspecting a compiled [`SchemaIr`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectResult {
    /// Detected JSON Schema dialect URI.
    pub dialect_uri: String,
    /// Source URI (file path or remote URL).
    pub source_uri: String,
    /// SHA-256 hex digest of the source bytes.
    pub source_digest: String,
    /// Total number of schema nodes (root + all sub-schemas).
    pub node_count: usize,
    /// High-level capabilities detected in the root schema.
    pub capabilities: Vec<String>,
}

/// Inspect a compiled [`SchemaIr`] and return a high-level summary.
#[must_use]
pub fn inspect_ir(ir: &SchemaIr) -> InspectResult {
    InspectResult {
        dialect_uri: ir.dialect_uri.clone(),
        source_uri: ir.source_uri.clone(),
        source_digest: ir.source_digest.clone(),
        node_count: count_nodes(&ir.root),
        capabilities: detect_capabilities(&ir.root),
    }
}

fn count_nodes(node: &SchemaNode) -> usize {
    let mut n = 1_usize;
    for prop in node.properties.values() {
        n += count_nodes(prop);
    }
    if let Some(items) = &node.items {
        n += count_nodes(items);
    }
    for sub in node.all_of.iter().chain(&node.any_of).chain(&node.one_of) {
        n += count_nodes(sub);
    }
    for def in node.defs.values() {
        n += count_nodes(def);
    }
    n
}

fn detect_capabilities(node: &SchemaNode) -> Vec<String> {
    let mut caps = collect_type_caps(node);
    caps.extend(collect_combinator_caps(node));
    caps
}

fn collect_type_caps(node: &SchemaNode) -> Vec<String> {
    let mut caps = Vec::new();
    if node.types.object {
        caps.push("object".to_owned());
    }
    if node.types.array {
        caps.push("array".to_owned());
    }
    if node.types.string {
        caps.push("string".to_owned());
    }
    if node.types.number {
        caps.push("number".to_owned());
    }
    if node.types.boolean {
        caps.push("boolean".to_owned());
    }
    if node.types.null {
        caps.push("nullable".to_owned());
    }
    caps
}

fn collect_combinator_caps(node: &SchemaNode) -> Vec<String> {
    let mut caps = Vec::new();
    if !node.all_of.is_empty() {
        caps.push("allOf".to_owned());
    }
    if !node.any_of.is_empty() {
        caps.push("anyOf".to_owned());
    }
    if !node.one_of.is_empty() {
        caps.push("oneOf".to_owned());
    }
    if node.not.is_some() {
        caps.push("not".to_owned());
    }
    caps
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_ir::{SchemaIr, SchemaNode, TypeSet};

    fn make_ir(root: SchemaNode) -> SchemaIr {
        SchemaIr::new(
            root,
            "https://json-schema.org/draft/2020-12/schema",
            "abc",
            "test://s",
        )
    }

    #[test]
    fn inspect_any_schema() {
        let ir = make_ir(SchemaNode::any());
        let r = inspect_ir(&ir);
        assert_eq!(r.node_count, 1);
        assert!(r.capabilities.contains(&"string".to_owned()));
    }

    #[test]
    fn inspect_object_schema() {
        use schemaforge_ir::ObjectConstraints;
        let mut node = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            ..SchemaNode::default()
        };
        node.object = ObjectConstraints {
            required: vec!["name".to_owned()],
            ..Default::default()
        };
        node.properties.insert(
            "name".to_owned(),
            SchemaNode {
                types: TypeSet::from_json(&serde_json::json!("string")),
                ..SchemaNode::default()
            },
        );
        let ir = make_ir(node);
        let r = inspect_ir(&ir);
        assert_eq!(r.node_count, 2);
        assert!(r.capabilities.contains(&"object".to_owned()));
    }
}
