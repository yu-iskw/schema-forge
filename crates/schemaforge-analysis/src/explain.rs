//! Structured explanation reports for schema nodes.
//!
//! [`explain_schema`] combines classification, dispatch-strategy suggestion,
//! and cost-model estimation into a single [`ExplainReport`] that callers
//! (compilers, documentation builders, CLIs) can consume without re-running
//! each analysis pass independently.

use schemaforge_ir::SchemaNode;

use crate::classify::{Representability, classify, pick_variants};
use crate::cost::estimated_generated_bytes;

/// Suggested property-dispatch strategy for a schema node.
///
/// Code generators use this hint to decide which lookup algorithm to emit
/// for object-property parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchStrategy {
    /// All property keys have distinct byte-lengths; a length-indexed jump
    /// table is faster than a linear string comparison.
    LengthFirst,
    /// Property keys share lengths; fall back to linear exact-match comparison.
    ExactMatch,
    /// The schema is a union type; variant discrimination is needed before
    /// individual property dispatch.
    TaggedUnion,
    /// The schema is fully dynamic; no static dispatch is possible.
    RuntimeAny,
}

/// Structured analysis report for a single schema node.
#[derive(Debug, Clone)]
pub struct ExplainReport {
    /// How representable the schema is as a static Rust type.
    pub representation: Representability,
    /// Suggested property-dispatch algorithm.
    pub dispatch_strategy: DispatchStrategy,
    /// Human-readable reasons why the schema could not achieve `Nominal`
    /// representation (empty when `representation == Nominal`).
    pub fallback_reasons: Vec<String>,
    /// Estimated number of bytes the code generator would emit for this node.
    pub estimated_bytes: usize,
}

/// Analyse `node` and return a structured [`ExplainReport`].
///
/// This function is the single entry-point that integrates classification,
/// dispatch-strategy selection, fallback-reason collection, and cost
/// estimation.  Callers should prefer this over calling the individual
/// sub-functions to avoid re-running the classification pass.
#[must_use]
pub fn explain_schema(node: &SchemaNode) -> ExplainReport {
    let representation = classify(node);
    let dispatch_strategy = suggest_dispatch(node, representation);
    let fallback_reasons = collect_fallback_reasons(node, representation);
    let estimated_bytes = estimated_generated_bytes(node);
    ExplainReport {
        representation,
        dispatch_strategy,
        fallback_reasons,
        estimated_bytes,
    }
}

/// Suggest a property-dispatch strategy for `node` given its `representation`.
///
/// Prefer this (with [`classify`]) over [`explain_schema`] when the caller only
/// needs classification and dispatch — explain also builds fallback reasons and
/// walks the tree for a byte-cost estimate.
#[must_use]
pub fn suggest_dispatch(node: &SchemaNode, rep: Representability) -> DispatchStrategy {
    match rep {
        Representability::Dynamic | Representability::Unsupported => DispatchStrategy::RuntimeAny,
        Representability::Structural if !node.any_of.is_empty() || !node.one_of.is_empty() => {
            DispatchStrategy::TaggedUnion
        }
        Representability::Nominal | Representability::Structural => suggest_object_dispatch(node),
    }
}

fn suggest_object_dispatch(node: &SchemaNode) -> DispatchStrategy {
    let names: Vec<&str> = node.properties.keys().map(String::as_str).collect();
    if names.len() > 1 && all_distinct_lengths(&names) {
        DispatchStrategy::LengthFirst
    } else {
        DispatchStrategy::ExactMatch
    }
}

fn all_distinct_lengths(names: &[&str]) -> bool {
    let mut seen = std::collections::HashSet::new();
    names.iter().all(|n| seen.insert(n.len()))
}

fn collect_fallback_reasons(node: &SchemaNode, rep: Representability) -> Vec<String> {
    let mut reasons = Vec::new();
    push_union_reasons(node, &mut reasons);
    push_unconstrained_reason(node, rep, &mut reasons);
    push_not_reason(node, &mut reasons);
    reasons
}

fn push_union_reasons(node: &SchemaNode, reasons: &mut Vec<String>) {
    if !node.any_of.is_empty() {
        let n = node.any_of.len();
        reasons.push(format!("anyOf introduces a {n}-branch union"));
    }
    if !node.one_of.is_empty() {
        let n = node.one_of.len();
        reasons.push(format!("oneOf introduces a {n}-branch union"));
    }
    let variants = pick_variants(node);
    push_mixed_type_reason(variants, reasons);
}

fn push_mixed_type_reason(variants: &[SchemaNode], reasons: &mut Vec<String>) {
    if variants.len() < 2 {
        return;
    }
    let first = &variants[0];
    let mixed = variants.iter().any(|v| v.types != first.types);
    if mixed {
        reasons
            .push("union branches have heterogeneous types — runtime dispatch required".to_owned());
    }
}

fn push_unconstrained_reason(node: &SchemaNode, rep: Representability, reasons: &mut Vec<String>) {
    if rep == Representability::Dynamic
        && node.any_of.is_empty()
        && node.one_of.is_empty()
        && node.properties.is_empty()
    {
        reasons.push("schema has no type constraint — mapped to serde_json::Value".to_owned());
    }
}

fn push_not_reason(node: &SchemaNode, reasons: &mut Vec<String>) {
    if node.not.is_some() {
        reasons.push(
            "not keyword cannot be checked statically — runtime evaluation required".to_owned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explain_nominal_scalar() {
        let node = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let report = explain_schema(&node);
        assert_eq!(report.representation, Representability::Nominal);
        assert!(report.fallback_reasons.is_empty());
        assert!(report.estimated_bytes > 0);
    }

    #[test]
    fn explain_dynamic_unconstrained() {
        let report = explain_schema(&SchemaNode::default());
        assert_eq!(report.representation, Representability::Dynamic);
        assert_eq!(report.dispatch_strategy, DispatchStrategy::RuntimeAny);
        assert!(!report.fallback_reasons.is_empty());
    }

    #[test]
    fn explain_unsupported_never() {
        let report = explain_schema(&SchemaNode::boolean_schema(false));
        assert_eq!(report.representation, Representability::Unsupported);
        assert_eq!(report.dispatch_strategy, DispatchStrategy::RuntimeAny);
    }

    #[test]
    fn dispatch_length_first_when_distinct_key_lengths() {
        let mut props = indexmap::IndexMap::new();
        props.insert("id".to_owned(), SchemaNode::default());
        props.insert("name".to_owned(), SchemaNode::default());
        props.insert("value".to_owned(), SchemaNode::default());
        let node = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let report = explain_schema(&node);
        // "id"(2), "name"(4), "value"(5) — all distinct lengths.
        assert_eq!(report.dispatch_strategy, DispatchStrategy::LengthFirst);
    }

    #[test]
    fn dispatch_exact_match_when_duplicate_key_lengths() {
        let mut props = indexmap::IndexMap::new();
        props.insert("foo".to_owned(), SchemaNode::default());
        props.insert("bar".to_owned(), SchemaNode::default()); // same length as "foo"
        let node = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let report = explain_schema(&node);
        assert_eq!(report.dispatch_strategy, DispatchStrategy::ExactMatch);
    }

    #[test]
    fn explain_mixed_any_of_has_fallback_reason() {
        let s = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("string")),
            ..SchemaNode::default()
        };
        let o = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("object")),
            ..SchemaNode::default()
        };
        let node = SchemaNode {
            any_of: vec![s, o],
            ..SchemaNode::default()
        };
        let report = explain_schema(&node);
        assert_eq!(report.representation, Representability::Dynamic);
        assert!(
            report
                .fallback_reasons
                .iter()
                .any(|r| r.contains("heterogeneous"))
        );
    }

    #[test]
    fn not_keyword_adds_fallback_reason() {
        let node = SchemaNode {
            types: schemaforge_ir::TypeSet::from_json(&json!("string")),
            not: Some(Box::new(SchemaNode::default())),
            ..SchemaNode::default()
        };
        let report = explain_schema(&node);
        assert!(report.fallback_reasons.iter().any(|r| r.contains("not")));
    }
}
