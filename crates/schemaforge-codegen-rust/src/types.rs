//! Mapping from the analysis `InferredNode` to Rust primitive type names.

/// Map an [`schemaforge_analysis::InferredNode`] to the Rust type name that
/// best represents it.
///
/// Returns a `'static` string because every possible result is one of a small
/// set of well-known Rust type names.
pub(crate) const fn inferred_to_rust_type(
    inferred: &schemaforge_analysis::InferredNode,
) -> &'static str {
    if inferred.any || inferred.never {
        return "serde_json::Value";
    }
    let t = &inferred.types;
    // A single-type schema contains exactly one of these flags plus nothing else.
    let no_other = !t.string && !t.boolean && !t.array && !t.object && !t.null;
    if t.string && !t.number && !t.boolean && !t.array && !t.object && !t.null {
        "String"
    } else if t.number && no_other {
        // `{"type":"number"}` sets number=true AND integer=true in TypeSet
        // because number is a superset of integer. Check number before integer
        // so the presence of `number` always wins and yields f64.
        "f64"
    } else if t.integer && !t.number && no_other {
        // integer-only (number is false): i64.
        "i64"
    } else if t.boolean && !t.string && !t.number && !t.array && !t.object && !t.null {
        "bool"
    } else {
        "serde_json::Value"
    }
}
