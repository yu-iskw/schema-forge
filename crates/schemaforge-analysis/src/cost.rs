//! Cost-model estimates for generated Rust code size.
//!
//! Provides [`estimated_generated_bytes`], a quick heuristic that predicts how
//! many bytes a code generator would emit for a given schema node.  The values
//! are intentionally approximate; they are useful for capacity checks and for
//! surfacing large schemas in explain reports, not for precise accounting.

use schemaforge_ir::SchemaNode;

use crate::classify::pick_variants;

/// Base byte cost for a generated scalar type alias (`pub type X = T;`).
const SCALAR_BYTES: usize = 40;
/// Base byte cost for an uninhabited (`never`) type.
const NEVER_BYTES: usize = 60;
/// Fixed overhead for a generated struct (derives, braces, etc.).
const STRUCT_BASE_BYTES: usize = 80;
/// Bytes per struct field (attribute, visibility, name, type, comma).
const FIELD_BYTES: usize = 50;
/// Fixed overhead for a generated enum.
const UNION_BASE_BYTES: usize = 80;
/// Bytes per enum variant line.
const VARIANT_BYTES: usize = 30;

/// Estimate how many bytes a Rust code generator would emit for `node`.
///
/// The estimate is recursive; nested structs and enum variants contribute to
/// the total.  The result is a rough upper bound — not an exact byte count.
///
/// Union combinators (`anyOf` / `oneOf`) are checked before the object branch
/// because a schema can have `types` that includes `object` while also
/// expressing a union — the union structure dominates the generated output.
#[must_use]
pub fn estimated_generated_bytes(node: &SchemaNode) -> usize {
    if node.is_never() {
        return NEVER_BYTES;
    }
    if !node.any_of.is_empty() || !node.one_of.is_empty() {
        return estimate_union(node);
    }
    if node.types.object || !node.properties.is_empty() {
        return estimate_struct(node);
    }
    SCALAR_BYTES
}

fn estimate_struct(node: &SchemaNode) -> usize {
    let field_cost = node.properties.len() * FIELD_BYTES;
    let nested: usize = node
        .properties
        .values()
        .map(estimated_generated_bytes)
        .sum();
    STRUCT_BASE_BYTES + field_cost + nested
}

fn estimate_union(node: &SchemaNode) -> usize {
    let variants = pick_variants(node);
    let variant_cost: usize = variants.iter().map(estimated_generated_bytes).sum();
    UNION_BASE_BYTES + variants.len() * VARIANT_BYTES + variant_cost
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_estimate_nonzero() {
        let node = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        assert!(estimated_generated_bytes(&node) > 0);
    }

    #[test]
    fn struct_estimate_grows_with_fields() {
        let mut props = indexmap::IndexMap::new();
        props.insert("a".to_owned(), SchemaNode::default());
        props.insert("b".to_owned(), SchemaNode::default());
        let node = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let bytes = estimated_generated_bytes(&node);
        assert!(bytes > STRUCT_BASE_BYTES);
    }

    #[test]
    fn never_estimate() {
        assert_eq!(
            estimated_generated_bytes(&SchemaNode::boolean_schema(false)),
            NEVER_BYTES
        );
    }

    #[test]
    fn union_estimate_grows_with_variants() {
        let s1 = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let s2 = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            any_of: vec![s1, s2],
            ..SchemaNode::default()
        };
        let bytes = estimated_generated_bytes(&node);
        assert!(bytes >= UNION_BASE_BYTES + 2 * VARIANT_BYTES);
    }
}
