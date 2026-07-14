//! Type inference and constraint analysis over the Schemaforge IR.
//!
//! Performs a bottom-up pass over a [`SchemaNode`] tree, narrowing type sets
//! based on combinators and extracting simplified constraint summaries for use
//! by code generators and documentation builders.
//!
//! # Key entry points
//!
//! - [`analyse`] — full inference pass, returns an [`InferredNode`].
//! - [`classify`] — classify how well a node maps to a static Rust type.
//! - [`explain_schema`] — combined classification + dispatch + cost report.
//! - [`estimated_generated_bytes`] — quick byte-cost estimate.

pub mod classify;
pub mod cost;
pub mod explain;

pub use classify::{Representability, classify};
pub use cost::estimated_generated_bytes;
pub use explain::{DispatchStrategy, ExplainReport, explain_schema};

use indexmap::IndexMap;
use schemaforge_ir::{SchemaNode, TypeSet};
use thiserror::Error;

/// Error returned during analysis.
#[derive(Debug, Error)]
pub enum AnalysisError {
    /// The schema contains a contradiction (e.g. `allOf` with conflicting types).
    #[error("schema contradiction at `{path}`: {reason}")]
    Contradiction {
        /// JSON Pointer path to the problematic node.
        path: String,
        /// Explanation of the contradiction.
        reason: String,
    },
}

/// Summary of inferred types and constraints for a single schema node.
#[derive(Debug, Clone)]
pub struct InferredNode {
    /// Narrowed type set after analysing combinators.
    pub types: TypeSet,
    /// Whether this node can be `null`.
    pub nullable: bool,
    /// Whether this node is always invalid (bottom type).
    pub never: bool,
    /// Whether this node accepts any value (top type).
    pub any: bool,
    /// Human-readable description of required properties (for objects).
    pub required_properties: Vec<String>,
    /// Inferred types for named properties.
    pub property_types: IndexMap<String, InferredNode>,
    /// Inferred type for array items.
    pub item_type: Option<Box<InferredNode>>,
}

impl InferredNode {
    /// Create a node that accepts no value (bottom type).
    fn never_node() -> Self {
        Self {
            types: TypeSet::none(),
            nullable: false,
            never: true,
            any: false,
            required_properties: Vec::new(),
            property_types: IndexMap::new(),
            item_type: None,
        }
    }
}

/// Analyse a [`SchemaNode`] and return an [`InferredNode`].
///
/// # Errors
///
/// Returns [`AnalysisError`] when the schema contains a detectable
/// contradiction.
pub fn analyse(node: &SchemaNode) -> Result<InferredNode, AnalysisError> {
    analyse_at(node, "")
}

fn analyse_at(node: &SchemaNode, path: &str) -> Result<InferredNode, AnalysisError> {
    if node.is_never() {
        return Ok(InferredNode::never_node());
    }
    let mut types = node.types;
    types = narrow_with_enum(node, types);
    types = narrow_with_const(node, types);
    types = intersect_all_of(node, path, types)?;
    types = narrow_with_union(node, path, types)?;

    let nullable = types.null;
    let any = types == TypeSet::any();
    let never = types.is_empty();

    let property_types = analyse_properties(node, path)?;
    let item_type = analyse_items(node, path)?;

    Ok(InferredNode {
        types,
        nullable,
        never,
        any,
        required_properties: node.object.required.clone(),
        property_types,
        item_type,
    })
}

fn narrow_with_enum(node: &SchemaNode, mut types: TypeSet) -> TypeSet {
    if node.enum_values.is_empty() {
        return types;
    }
    let mut narrowed = TypeSet::none();
    for v in &node.enum_values {
        if v.is_null() && types.null {
            narrowed.null = true;
        } else if v.is_boolean() && types.boolean {
            narrowed.boolean = true;
        } else if v.is_i64() && types.integer {
            narrowed.integer = true;
        } else if v.is_f64() && types.number {
            narrowed.number = true;
        } else if v.is_string() && types.string {
            narrowed.string = true;
        } else if v.is_array() && types.array {
            narrowed.array = true;
        } else if v.is_object() && types.object {
            narrowed.object = true;
        }
    }
    types = narrowed;
    types
}

fn narrow_with_const(node: &SchemaNode, mut types: TypeSet) -> TypeSet {
    let Some(ref cv) = node.const_value else {
        return types;
    };
    let mut narrowed = TypeSet::none();
    narrowed.null = cv.is_null() && types.null;
    narrowed.boolean = cv.is_boolean() && types.boolean;
    narrowed.integer = cv.is_i64() && types.integer;
    narrowed.number = cv.is_f64() && types.number;
    narrowed.string = cv.is_string() && types.string;
    narrowed.array = cv.is_array() && types.array;
    narrowed.object = cv.is_object() && types.object;
    types = narrowed;
    types
}

fn intersect_all_of(
    node: &SchemaNode,
    path: &str,
    mut types: TypeSet,
) -> Result<TypeSet, AnalysisError> {
    for (i, sub) in node.all_of.iter().enumerate() {
        let sub_path = format!("{path}/allOf/{i}");
        let sub_inferred = analyse_at(sub, &sub_path)?;
        types = intersect_type_sets(types, sub_inferred.types);
        if types.is_empty() {
            return Err(AnalysisError::Contradiction {
                path: sub_path,
                reason: "allOf subschemas produce an empty type intersection".to_owned(),
            });
        }
    }
    Ok(types)
}

/// Narrow `types` by unioning the type sets reachable via `anyOf` and `oneOf`.
///
/// For type-inference purposes both keywords behave identically: any type
/// reachable through any branch is potentially valid, so we take the union of
/// all branch type sets and then intersect with the parent's declared types.
fn narrow_with_union(
    node: &SchemaNode,
    path: &str,
    types: TypeSet,
) -> Result<TypeSet, AnalysisError> {
    let has_any_of = !node.any_of.is_empty();
    let has_one_of = !node.one_of.is_empty();
    if !has_any_of && !has_one_of {
        return Ok(types);
    }
    let combined = combine_variant_types(node, path)?;
    Ok(intersect_type_sets(types, combined))
}

fn combine_variant_types(node: &SchemaNode, path: &str) -> Result<TypeSet, AnalysisError> {
    let mut result = TypeSet::none();
    result = union_with_variants(result, &node.any_of, path, "anyOf")?;
    result = union_with_variants(result, &node.one_of, path, "oneOf")?;
    Ok(result)
}

fn union_with_variants(
    mut acc: TypeSet,
    variants: &[SchemaNode],
    path: &str,
    prefix: &str,
) -> Result<TypeSet, AnalysisError> {
    for (i, sub) in variants.iter().enumerate() {
        let sub_path = format!("{path}/{prefix}/{i}");
        let sub_inferred = analyse_at(sub, &sub_path)?;
        acc = union_type_sets(acc, sub_inferred.types);
    }
    Ok(acc)
}

pub(crate) const fn intersect_type_sets(a: TypeSet, b: TypeSet) -> TypeSet {
    TypeSet {
        null: a.null && b.null,
        boolean: a.boolean && b.boolean,
        integer: a.integer && b.integer,
        number: a.number && b.number,
        string: a.string && b.string,
        array: a.array && b.array,
        object: a.object && b.object,
    }
}

pub(crate) const fn union_type_sets(a: TypeSet, b: TypeSet) -> TypeSet {
    TypeSet {
        null: a.null || b.null,
        boolean: a.boolean || b.boolean,
        integer: a.integer || b.integer,
        number: a.number || b.number,
        string: a.string || b.string,
        array: a.array || b.array,
        object: a.object || b.object,
    }
}

fn analyse_properties(
    node: &SchemaNode,
    path: &str,
) -> Result<IndexMap<String, InferredNode>, AnalysisError> {
    let mut map = IndexMap::new();
    for (key, prop) in &node.properties {
        let prop_path = format!("{path}/properties/{key}");
        map.insert(key.clone(), analyse_at(prop, &prop_path)?);
    }
    Ok(map)
}

fn analyse_items(
    node: &SchemaNode,
    path: &str,
) -> Result<Option<Box<InferredNode>>, AnalysisError> {
    let Some(ref items) = node.items else {
        return Ok(None);
    };
    let items_path = format!("{path}/items");
    Ok(Some(Box::new(analyse_at(items, &items_path)?)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_ir::{SchemaNode, TypeSet};
    use serde_json::json;

    #[test]
    fn analyse_any_schema() {
        let node = SchemaNode::any();
        let inferred = analyse(&node).unwrap();
        assert!(inferred.any);
        assert!(!inferred.never);
    }

    #[test]
    fn analyse_never_schema() {
        let node = SchemaNode::boolean_schema(false);
        let inferred = analyse(&node).unwrap();
        assert!(inferred.never);
    }

    #[test]
    fn analyse_string_type() {
        let node = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let inferred = analyse(&node).unwrap();
        assert!(inferred.types.string);
        assert!(!inferred.types.number);
    }

    #[test]
    fn analyse_required_properties() {
        let node = SchemaNode {
            types: TypeSet::from_json(&json!("object")),
            object: schemaforge_ir::ObjectConstraints {
                required: vec!["name".to_owned()],
                ..Default::default()
            },
            ..SchemaNode::default()
        };
        let inferred = analyse(&node).unwrap();
        assert!(inferred.required_properties.contains(&"name".to_owned()));
    }

    #[test]
    fn analyse_all_of_intersection() {
        let sub1 = SchemaNode {
            types: TypeSet::from_json(&json!(["string", "number"])),
            ..SchemaNode::default()
        };
        let sub2 = SchemaNode {
            types: TypeSet::from_json(&json!(["string", "null"])),
            ..SchemaNode::default()
        };
        let root = SchemaNode {
            all_of: vec![sub1, sub2],
            ..SchemaNode::default()
        };
        let inferred = analyse(&root).unwrap();
        assert!(inferred.types.string);
        assert!(!inferred.types.number);
        assert!(!inferred.types.null);
    }

    #[test]
    fn analyse_any_of_narrows_types() {
        let sub1 = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let sub2 = SchemaNode {
            types: TypeSet::from_json(&json!("number")),
            ..SchemaNode::default()
        };
        let root = SchemaNode {
            any_of: vec![sub1, sub2],
            ..SchemaNode::any()
        };
        let inferred = analyse(&root).unwrap();
        assert!(inferred.types.string);
        assert!(inferred.types.number || inferred.types.integer);
        assert!(!inferred.types.boolean);
        assert!(!inferred.types.object);
    }

    #[test]
    fn analyse_one_of_narrows_types() {
        let sub1 = SchemaNode {
            types: TypeSet::from_json(&json!("boolean")),
            ..SchemaNode::default()
        };
        let sub2 = SchemaNode {
            types: TypeSet::from_json(&json!("null")),
            ..SchemaNode::default()
        };
        let root = SchemaNode {
            one_of: vec![sub1, sub2],
            ..SchemaNode::any()
        };
        let inferred = analyse(&root).unwrap();
        assert!(inferred.types.boolean);
        assert!(inferred.types.null);
        assert!(!inferred.types.string);
    }

    #[test]
    fn analyse_any_of_intersects_with_explicit_type() {
        let sub1 = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let sub2 = SchemaNode {
            types: TypeSet::from_json(&json!("number")),
            ..SchemaNode::default()
        };
        // Parent declares type: string — anyOf with number should be excluded.
        let root = SchemaNode {
            types: TypeSet::from_json(&json!("string")),
            any_of: vec![sub1, sub2],
            ..SchemaNode::default()
        };
        let inferred = analyse(&root).unwrap();
        assert!(inferred.types.string);
        assert!(!inferred.types.number);
    }
}
