//! Representability classification for schema nodes.
//!
//! Each schema node is classified into one of four representability tiers
//! that downstream code generators use to decide how to emit Rust types.

use schemaforge_ir::{SchemaNode, TypeSet};

/// How well a schema node can be represented as a static Rust type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Representability {
    /// A concrete named type can be generated: struct, enum of scalars, or
    /// scalar type alias.
    Nominal,
    /// The schema has well-known structure but uses open patterns or unions of
    /// uniform types (e.g. arrays, `additionalProperties`, homogeneous
    /// `anyOf`).
    Structural,
    /// The schema requires runtime dispatch — heterogeneous `anyOf`/`oneOf`,
    /// or a fully unconstrained `{}` schema.
    Dynamic,
    /// The schema is unsatisfiable (`false` boolean schema, empty type set).
    /// No valid value exists; an uninhabited type is emitted.
    Unsupported,
}

/// Classify how representable `node` is as a static Rust type.
#[must_use]
pub fn classify(node: &SchemaNode) -> Representability {
    if node.is_never() {
        return Representability::Unsupported;
    }
    if has_union(node) {
        return classify_union(node);
    }
    if is_fully_unconstrained(node) {
        return Representability::Dynamic;
    }
    classify_typed(node)
}

/// Return all union variant schemas from the node (anyOf takes precedence).
#[must_use]
pub fn pick_variants(node: &SchemaNode) -> &[SchemaNode] {
    if node.any_of.is_empty() {
        &node.one_of
    } else {
        &node.any_of
    }
}

const fn has_union(node: &SchemaNode) -> bool {
    !node.any_of.is_empty() || !node.one_of.is_empty()
}

fn is_fully_unconstrained(node: &SchemaNode) -> bool {
    // A node is unconstrained only when every constraint field is absent.
    // Note: any_of and one_of are already handled by has_union() before this
    // function is reached, so they do not need an explicit check here.
    node.types == TypeSet::any()
        && node.properties.is_empty()
        && node.all_of.is_empty()
        && node.not.is_none()
        && node.items.is_none()
        && node.prefix_items.is_empty()
        && node.defs.is_empty()
        && node.enum_values.is_none()
        && node.const_value.is_none()
}

fn classify_union(node: &SchemaNode) -> Representability {
    // Object with explicit named properties + anyOf/oneOf constraints is still Nominal.
    if node.types.object && !node.properties.is_empty() {
        return Representability::Nominal;
    }
    let variants = pick_variants(node);
    if variants_share_single_type(variants) {
        Representability::Structural
    } else {
        Representability::Dynamic
    }
}

fn classify_typed(node: &SchemaNode) -> Representability {
    if node.types.object || !node.properties.is_empty() {
        return classify_object(node);
    }
    if is_scalar_type(node.types) {
        return Representability::Nominal;
    }
    if node.types.array {
        return Representability::Structural;
    }
    Representability::Dynamic
}

fn classify_object(node: &SchemaNode) -> Representability {
    if node.properties.is_empty() {
        // Object type with no explicit named properties (open schema /
        // additionalProperties only) → Structural.
        Representability::Structural
    } else {
        Representability::Nominal
    }
}

/// True when all variants share one identical `TypeSet` and that set contains
/// exactly one JSON type.
fn variants_share_single_type(variants: &[SchemaNode]) -> bool {
    let Some(first) = variants.first() else {
        return false;
    };
    let ft = first.types;
    variants.iter().all(|v| v.types == ft && is_single_type(ft))
}

fn is_single_type(ts: TypeSet) -> bool {
    // `number` implies `integer` in the IR (TypeSet::apply_str sets both when
    // the keyword is "number"), so treat number||integer as a single type slot.
    let count = u8::from(ts.null)
        + u8::from(ts.boolean)
        + u8::from(ts.number || ts.integer)
        + u8::from(ts.string)
        + u8::from(ts.array)
        + u8::from(ts.object);
    count == 1
}

/// True when `ts` contains only scalar types (string, integer, number, boolean)
/// and none of object or array.
const fn is_scalar_type(ts: TypeSet) -> bool {
    (ts.string || ts.integer || ts.number || ts.boolean) && !ts.object && !ts.array
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_ir::ObjectConstraints;
    use serde_json::json;

    fn string_node() -> SchemaNode {
        SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        }
    }

    fn object_node_with_props() -> SchemaNode {
        let mut props = indexmap::IndexMap::new();
        props.insert("id".to_owned(), string_node());
        SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            properties: props,
            ..SchemaNode::default()
        }
    }

    #[test]
    fn nominal_scalar_string() {
        assert_eq!(classify(&string_node()), Representability::Nominal);
    }

    #[test]
    fn nominal_scalar_integer() {
        let node = SchemaNode {
            types: TypeSet::from_json(&json!("integer")),
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Nominal);
    }

    #[test]
    fn nominal_object_with_properties() {
        assert_eq!(
            classify(&object_node_with_props()),
            Representability::Nominal
        );
    }

    #[test]
    fn structural_array() {
        let node = SchemaNode {
            types: TypeSet::from_json(&json!("array")),
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Structural);
    }

    #[test]
    fn structural_open_object() {
        let node = SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Structural);
    }

    #[test]
    fn structural_homogeneous_any_of() {
        let s1 = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let s2 = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            any_of: vec![s1, s2],
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Structural);
    }

    #[test]
    fn structural_homogeneous_any_of_number_schemas() {
        // "type": "number" sets both number=true and integer=true in the IR.
        // Two number-schema variants must be treated as homogeneous (single type)
        // and produce Structural, not Dynamic.
        let n1 = SchemaNode {
            types: TypeSet::from_json(&json!("number")),
            ..SchemaNode::default()
        };
        let n2 = SchemaNode {
            types: TypeSet::from_json(&json!("number")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            any_of: vec![n1, n2],
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Structural);
    }

    #[test]
    fn structural_homogeneous_any_of_integer_schemas() {
        // Pure integer-only variants (integer=true, number=false) also count as
        // a single numeric type, so a homogeneous anyOf is Structural.
        let i1 = SchemaNode {
            types: TypeSet::from_json(&json!("integer")),
            ..SchemaNode::default()
        };
        let i2 = SchemaNode {
            types: TypeSet::from_json(&json!("integer")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            any_of: vec![i1, i2],
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Structural);
    }

    #[test]
    fn dynamic_mixed_any_of() {
        let s = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let o = SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            any_of: vec![s, o],
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Dynamic);
    }

    #[test]
    fn dynamic_unconstrained() {
        let node = SchemaNode::default();
        assert_eq!(classify(&node), Representability::Dynamic);
    }

    #[test]
    fn unsupported_never() {
        let node = SchemaNode::boolean_schema(false);
        assert_eq!(classify(&node), Representability::Unsupported);
    }

    #[test]
    fn unsupported_empty_enum() {
        // An explicitly empty enum (Some([])) means the schema is never
        // satisfiable — classify must return Unsupported (via is_never).
        let node = SchemaNode {
            enum_values: Some(vec![]),
            ..SchemaNode::default()
        };
        assert_eq!(classify(&node), Representability::Unsupported);
    }

    #[test]
    fn dynamic_schema_with_all_of_is_not_unconstrained() {
        // A node produced by $ref + constraint siblings carries a non-empty
        // all_of vec.  is_fully_unconstrained must return false so it is NOT
        // misclassified as Dynamic (which would produce serde_json::Value).
        let sub = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            all_of: vec![sub],
            ..SchemaNode::default()
        };
        // Must NOT be Dynamic — the all_of constrains the schema.
        assert_ne!(classify(&node), Representability::Dynamic);
    }

    #[test]
    fn dynamic_schema_with_not_is_not_unconstrained() {
        let node = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            not: Some(Box::new(SchemaNode::default())),
            ..SchemaNode::default()
        };
        assert_ne!(classify(&node), Representability::Dynamic);
    }

    #[test]
    fn dynamic_schema_with_enum_values_is_not_unconstrained() {
        // A node with Some([...]) enum_values is constrained.
        let node = SchemaNode {
            enum_values: Some(vec![serde_json::json!("a")]),
            ..SchemaNode::default()
        };
        assert_ne!(classify(&node), Representability::Dynamic);
    }

    #[test]
    fn dynamic_schema_with_const_value_is_not_unconstrained() {
        let node = SchemaNode {
            const_value: Some(serde_json::json!(42)),
            ..SchemaNode::default()
        };
        assert_ne!(classify(&node), Representability::Dynamic);
    }

    #[test]
    fn nominal_object_with_anyof_constraints() {
        let mut props = indexmap::IndexMap::new();
        props.insert("name".to_owned(), string_node());
        let req1 = SchemaNode {
            object: ObjectConstraints {
                required: vec!["name".to_owned()],
                ..Default::default()
            },
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            properties: props,
            any_of: vec![req1],
            ..SchemaNode::default()
        };
        // Object with explicit properties + anyOf constraints is still Nominal.
        assert_eq!(classify(&node), Representability::Nominal);
    }
}
