//! Unit tests for the Schemaforge compiler.

use super::*;

#[test]
fn compile_simple_json_schema() {
    let mut c = Compiler::new();
    let ir = c
        .compile_json("test://schema.json", r#"{"type":"string","minLength":1}"#)
        .unwrap();
    assert!(ir.root.types.string);
    assert!(!ir.root.types.number);
    assert_eq!(ir.root.string.min_length, Some(1));
}

#[test]
fn compile_boolean_schema_true() {
    let mut c = Compiler::new();
    let ir = c.compile_json("test://bool.json", "true").unwrap();
    assert!(ir.root.types.string);
}

#[test]
fn compile_boolean_schema_false() {
    let mut c = Compiler::new();
    let ir = c.compile_json("test://bool.json", "false").unwrap();
    assert!(ir.root.is_never());
}

#[test]
fn compile_object_with_properties() {
    let mut c = Compiler::new();
    let src = r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string"},
            "age":  {"type": "integer"}
        },
        "required": ["name"]
    }"#;
    let ir = c.compile_json("test://obj.json", src).unwrap();
    assert!(ir.root.types.object);
    assert!(ir.root.properties.contains_key("name"));
    assert!(ir.root.object.required.contains(&"name".to_owned()));
}

#[test]
fn compile_invalid_json_returns_error() {
    let mut c = Compiler::new();
    let result = c.compile_json("test://bad.json", "{invalid}");
    assert!(result.is_err());
}

#[test]
fn source_digest_is_hex() {
    let mut c = Compiler::new();
    let ir = c.compile_json("test://digest.json", "{}").unwrap();
    assert!(ir.source_digest.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(ir.source_digest.len(), 64);
}

#[test]
fn compile_local_ref_to_defs() {
    let mut c = Compiler::new();
    let src = r##"{
        "type": "object",
        "properties": {
            "name": {"$ref": "#/$defs/StringField"}
        },
        "$defs": {
            "StringField": {"type": "string", "minLength": 1}
        }
    }"##;
    let ir = c.compile_json("test://ref.json", src).unwrap();
    let name_prop = ir.root.properties.get("name").expect("property exists");
    assert!(name_prop.types.string, "ref lowered to string type");
    assert_eq!(name_prop.string.min_length, Some(1));
}

#[test]
fn compile_root_ref_cycle_is_error() {
    let mut c = Compiler::new();
    // A `$defs` entry that `$ref`s back to the document root creates an
    // unbounded recursive schema.  The compiler must reject it with a
    // `CyclicRef` error rather than silently returning an open schema.
    let src = r##"{"$defs": {"Self": {"$ref": "#"}}}"##;
    let result = c.compile_json("test://root-ref.json", src);
    assert!(
        matches!(result, Err(CompileError::CyclicRef { .. })),
        "expected CyclicRef error but got: {result:?}"
    );
}

#[test]
fn compile_non_schema_number_returns_error() {
    let mut c = Compiler::new();
    let result = c.compile_json("test://bad.json", "42");
    assert!(
        matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
        "expected InvalidSchemaKind for number schema, got: {result:?}"
    );
}

#[test]
fn compile_non_schema_string_returns_error() {
    let mut c = Compiler::new();
    let result = c.compile_json("test://bad.json", r#""just a string""#);
    assert!(
        matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
        "expected InvalidSchemaKind for string schema, got: {result:?}"
    );
}

#[test]
fn compile_non_schema_array_returns_error() {
    let mut c = Compiler::new();
    let result = c.compile_json("test://bad.json", "[]");
    assert!(
        matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
        "expected InvalidSchemaKind for array schema, got: {result:?}"
    );
}

#[test]
fn compile_non_schema_null_returns_error() {
    let mut c = Compiler::new();
    let result = c.compile_json("test://bad.json", "null");
    assert!(
        matches!(result, Err(CompileError::InvalidSchemaKind { .. })),
        "expected InvalidSchemaKind for null schema, got: {result:?}"
    );
}

#[test]
fn compile_non_cyclic_ref_succeeds() {
    let mut c = Compiler::new();
    // A ref that points to a definition that does not refer back should
    // still compile successfully.
    let src = r##"{
        "type": "object",
        "properties": {
            "name": {"$ref": "#/$defs/StringField"}
        },
        "$defs": {
            "StringField": {"type": "string"}
        }
    }"##;
    let ir = c.compile_json("test://non-cyclic.json", src).unwrap();
    assert!(ir.root.properties.contains_key("name"));
}

// ── $ref + siblings (Draft 2020-12) ──────────────────────────────────────

#[test]
fn compile_ref_with_type_sibling_applies_both() {
    // A schema with $ref AND a sibling `type` keyword should produce an
    // allOf that enforces both the referenced schema and the type assertion.
    let mut c = Compiler::new();
    let src = r##"{
        "$defs": {
            "Base": {"minLength": 1}
        },
        "$ref": "#/$defs/Base",
        "type": "string"
    }"##;
    let ir = c.compile_json("test://ref-siblings.json", src).unwrap();
    // The root node must carry an allOf with exactly two members.
    assert_eq!(
        ir.root.all_of.len(),
        2,
        "expected allOf([ref_node, sibling_node]), got {:?}",
        ir.root.all_of.len()
    );
    // One member should carry the minLength from the $ref target.
    let has_min_length = ir
        .root
        .all_of
        .iter()
        .any(|n| n.string.min_length == Some(1));
    assert!(
        has_min_length,
        "allOf should contain the $ref target with minLength"
    );
    // One member should carry the type constraint from the sibling.
    let has_type = ir
        .root
        .all_of
        .iter()
        .any(|n| n.types.string && !n.types.number);
    assert!(
        has_type,
        "allOf should contain the sibling with type:string"
    );
}

#[test]
fn compile_ref_without_siblings_is_shortcut() {
    // A $ref with only metadata-style siblings must still take the
    // short-circuit path (no allOf wrapper).
    let mut c = Compiler::new();
    let src = r##"{
        "$defs": {"S": {"type": "integer"}},
        "$ref": "#/$defs/S",
        "$comment": "just a comment"
    }"##;
    let ir = c.compile_json("test://ref-no-siblings.json", src).unwrap();
    assert!(
        ir.root.all_of.is_empty(),
        "pure $ref with only metadata siblings must not produce allOf"
    );
    assert!(
        ir.root.types.integer,
        "root must be lowered from the $ref target"
    );
}

// ── exclusiveMinimum / exclusiveMaximum boolean ───────────────────────────

#[test]
fn compile_exclusive_minimum_bool_true_converts() {
    // Draft 4 / OAS 3.0 style: exclusiveMinimum: true with minimum: X
    // must compile to an exclusive bound at X.
    let mut c = Compiler::new();
    let src = r#"{"type":"number","minimum":5.0,"exclusiveMinimum":true}"#;
    let ir = c.compile_json("test://excl-min.json", src).unwrap();
    let bound = ir
        .root
        .numeric
        .minimum
        .as_ref()
        .expect("minimum bound present");
    assert!(bound.exclusive, "bound must be exclusive");
    assert!(
        (bound.value - 5.0).abs() < f64::EPSILON,
        "bound value must equal minimum"
    );
}

#[test]
fn compile_exclusive_minimum_bool_false_is_inclusive() {
    // exclusiveMinimum: false means minimum is non-exclusive; the keyword
    // is dropped but minimum itself is kept as an inclusive bound.
    let mut c = Compiler::new();
    let src = r#"{"type":"number","minimum":3.0,"exclusiveMinimum":false}"#;
    let ir = c.compile_json("test://excl-min-false.json", src).unwrap();
    let bound = ir
        .root
        .numeric
        .minimum
        .as_ref()
        .expect("minimum bound present");
    assert!(!bound.exclusive, "bound must be non-exclusive");
    assert!((bound.value - 3.0).abs() < f64::EPSILON);
}

#[test]
fn compile_exclusive_maximum_bool_true_converts() {
    // Same conversion for the upper bound.
    let mut c = Compiler::new();
    let src = r#"{"type":"number","maximum":10.0,"exclusiveMaximum":true}"#;
    let ir = c.compile_json("test://excl-max.json", src).unwrap();
    let bound = ir
        .root
        .numeric
        .maximum
        .as_ref()
        .expect("maximum bound present");
    assert!(bound.exclusive, "upper bound must be exclusive");
    assert!((bound.value - 10.0).abs() < f64::EPSILON);
}

#[test]
fn compile_exclusive_minimum_numeric_passthrough() {
    // Draft 2020-12 style numeric exclusiveMinimum must pass through unchanged.
    let mut c = Compiler::new();
    let src = r#"{"type":"number","exclusiveMinimum":7.5}"#;
    let ir = c.compile_json("test://excl-min-num.json", src).unwrap();
    let bound = ir
        .root
        .numeric
        .minimum
        .as_ref()
        .expect("minimum bound present");
    assert!(bound.exclusive);
    assert!((bound.value - 7.5).abs() < f64::EPSILON);
}

// ── Unsupported keyword fail-closed ───────────────────────────────────────

fn assert_unsupported(keyword: &str, schema_json: &str) {
    let mut c = Compiler::new();
    let result = c.compile_json("test://unsupported.json", schema_json);
    assert!(
        matches!(result, Err(CompileError::UnsupportedKeyword { keyword: ref k }) if k == keyword),
        "expected UnsupportedKeyword({keyword}) for schema {schema_json}, got: {result:?}"
    );
}

#[test]
fn compile_unevaluated_properties_is_unsupported() {
    assert_unsupported(
        "unevaluatedProperties",
        r#"{"type":"object","unevaluatedProperties":false}"#,
    );
}

#[test]
fn compile_unevaluated_items_is_unsupported() {
    assert_unsupported(
        "unevaluatedItems",
        r#"{"type":"array","unevaluatedItems":false}"#,
    );
}

#[test]
fn compile_pattern_properties_is_unsupported() {
    assert_unsupported(
        "patternProperties",
        r#"{"type":"object","patternProperties":{"^x-":{"type":"string"}}}"#,
    );
}

#[test]
fn compile_if_is_unsupported() {
    assert_unsupported("if", r#"{"if":{"type":"string"},"then":{"minLength":1}}"#);
}

#[test]
fn compile_dependent_schemas_is_unsupported() {
    assert_unsupported(
        "dependentSchemas",
        r#"{"type":"object","dependentSchemas":{"a":{"required":["b"]}}}"#,
    );
}

#[test]
fn compile_property_names_is_unsupported() {
    assert_unsupported(
        "propertyNames",
        r#"{"type":"object","propertyNames":{"maxLength":5}}"#,
    );
}

#[test]
fn compile_contains_is_unsupported() {
    assert_unsupported(
        "contains",
        r#"{"type":"array","contains":{"type":"integer"}}"#,
    );
}

#[test]
fn compile_dynamic_ref_is_unsupported() {
    assert_unsupported(
        "$dynamicRef",
        r##"{"$dynamicRef":"#items","type":"array"}"##,
    );
}

#[test]
fn compile_dynamic_anchor_is_unsupported() {
    assert_unsupported(
        "$dynamicAnchor",
        r#"{"$dynamicAnchor":"items","type":"array"}"#,
    );
}

#[test]
fn compile_dynamic_anchor_with_ref_is_unsupported() {
    assert_unsupported(
        "$dynamicAnchor",
        r##"{"$defs":{"S":{"type":"string"}},"$ref":"#/$defs/S","$dynamicAnchor":"items"}"##,
    );
}

#[test]
fn compile_dependent_required_is_unsupported() {
    assert_unsupported(
        "dependentRequired",
        r#"{"type":"object","dependentRequired":{"credit_card":["billing_address"]}}"#,
    );
}

#[test]
fn compile_content_schema_is_unsupported() {
    assert_unsupported(
        "contentSchema",
        r#"{"type":"string","contentSchema":{"type":"object"}}"#,
    );
}

#[test]
fn compile_content_media_type_is_unsupported() {
    assert_unsupported(
        "contentMediaType",
        r#"{"type":"string","contentMediaType":"application/json"}"#,
    );
}

#[test]
fn compile_content_encoding_is_unsupported() {
    assert_unsupported(
        "contentEncoding",
        r#"{"type":"string","contentEncoding":"base64"}"#,
    );
}

#[test]
fn compile_recursive_ref_is_unsupported() {
    assert_unsupported(
        "$recursiveRef",
        r##"{"$recursiveRef":"#","type":"object"}"##,
    );
}

#[test]
fn compile_recursive_anchor_is_unsupported() {
    assert_unsupported(
        "$recursiveAnchor",
        r#"{"$recursiveAnchor":true,"type":"object"}"#,
    );
}

#[test]
fn compile_dependencies_is_unsupported() {
    assert_unsupported(
        "dependencies",
        r#"{"type":"object","dependencies":{"credit_card":["billing_address"]}}"#,
    );
}

// ── $ref + annotation-only siblings (description/title) ──────────────────

#[test]
fn compile_ref_with_description_alone_still_shortcuts() {
    // A $ref with only a `description` sibling must still take the
    // short-circuit path and NOT produce an allOf wrapper, because
    // `description` is a pure annotation with no assertion semantics.
    let mut c = Compiler::new();
    let src = r##"{
        "$defs": {"S": {"type": "string"}},
        "$ref": "#/$defs/S",
        "description": "A short description"
    }"##;
    let ir = c.compile_json("test://ref-desc.json", src).unwrap();
    assert!(
        ir.root.all_of.is_empty(),
        "pure $ref with only description must not produce allOf"
    );
    assert!(
        ir.root.types.string,
        "root must be lowered from the $ref target (string)"
    );
}

#[test]
fn compile_ref_with_title_alone_still_shortcuts() {
    // Same shortcut behaviour for `title` annotation.
    let mut c = Compiler::new();
    let src = r##"{
        "$defs": {"S": {"type": "integer"}},
        "$ref": "#/$defs/S",
        "title": "An integer field"
    }"##;
    let ir = c.compile_json("test://ref-title.json", src).unwrap();
    assert!(
        ir.root.all_of.is_empty(),
        "pure $ref with only title must not produce allOf"
    );
    assert!(
        ir.root.types.integer,
        "root must be lowered from the $ref target (integer)"
    );
}

// ── Empty enum ────────────────────────────────────────────────────────────

#[test]
fn compile_empty_enum_preserves_enum_present_flag() {
    // `{"enum": []}` must compile to a node where enum_values is Some([]),
    // not None — so that downstream code can distinguish "enum absent"
    // from "enum present but empty".
    let mut c = Compiler::new();
    let ir = c
        .compile_json("test://empty-enum.json", r#"{"enum":[]}"#)
        .unwrap();
    assert!(
        ir.root.enum_values.as_ref().is_some_and(Vec::is_empty),
        "empty enum must be Some([]), got {:?}",
        ir.root.enum_values
    );
}

#[test]
fn compile_empty_enum_is_never() {
    // `{"enum": []}` is a schema that can never be satisfied.
    let mut c = Compiler::new();
    let ir = c
        .compile_json("test://empty-enum.json", r#"{"enum":[]}"#)
        .unwrap();
    assert!(
        ir.root.is_never(),
        "empty enum schema must be classified as never"
    );
}

#[test]
fn compile_deep_acyclic_ref_chain_exceeds_depth_limit() {
    let mut c = Compiler::new();
    // Build a linear chain of MAX_DEPTH+2 defs (each refs the next, none
    // cyclic) to guarantee the depth bound fires rather than a stack
    // overflow.
    let chain_len = LowerCtx::MAX_DEPTH + 2;
    let mut defs = serde_json::Map::new();
    for i in 0..chain_len {
        let key = format!("def{i}");
        let val = if i + 1 < chain_len {
            serde_json::json!({ "$ref": format!("#/$defs/def{}", i + 1) })
        } else {
            serde_json::json!({ "type": "string" })
        };
        defs.insert(key, val);
    }
    let schema = serde_json::json!({ "$ref": "#/$defs/def0", "$defs": defs });
    let src = serde_json::to_string(&schema).unwrap();
    let result = c.compile_json("test://deep-chain.json", &src);
    assert!(
        matches!(result, Err(CompileError::DepthExceeded { .. })),
        "expected DepthExceeded for long acyclic $ref chain, got: {result:?}"
    );
}

// ── Node count limit ──────────────────────────────────────────────────────

#[test]
fn compile_node_limit_exceeded_error() {
    // Build a schema with more than DEFAULT_MAX_NODE_COUNT nodes by deeply
    // nesting allOf arrays.  Each element is a schema node.
    let mut c = Compiler::new();
    // Build a flat allOf with slightly more than the limit to trigger the error.
    let limit = LowerCtx::DEFAULT_MAX_NODE_COUNT + 10;
    let members: Vec<serde_json::Value> = (0..limit)
        .map(|_| serde_json::json!({"type": "string"}))
        .collect();
    let schema = serde_json::json!({"allOf": members});
    let src = serde_json::to_string(&schema).unwrap();
    let result = c.compile_json("test://too-many-nodes.json", &src);
    assert!(
        matches!(result, Err(CompileError::NodeLimitExceeded { .. })),
        "expected NodeLimitExceeded for oversized schema, got: {result:?}"
    );
}

#[test]
fn compile_schema_within_node_limit_succeeds() {
    // A modest schema must compile successfully and not hit the node limit.
    let mut c = Compiler::new();
    let members: Vec<serde_json::Value> = (0..10)
        .map(|_| serde_json::json!({"type": "string"}))
        .collect();
    let schema = serde_json::json!({"allOf": members});
    let src = serde_json::to_string(&schema).unwrap();
    assert!(
        c.compile_json("test://small-schema.json", &src).is_ok(),
        "small schema must compile without hitting node limit"
    );
}
