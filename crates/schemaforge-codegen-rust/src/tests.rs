use schemaforge_compiler::Compiler;
use schemaforge_ir::{SchemaIr, SchemaNode, TypeSet};

use crate::names::{to_pascal_case, to_snake_case};
use crate::{CodegenError, CodegenOptions, generate};

fn type_val(s: &str) -> serde_json::Value {
    serde_json::json!(s)
}

fn simple_ir(type_json: &serde_json::Value) -> SchemaIr {
    let root = SchemaNode {
        types: TypeSet::from_json(type_json),
        ..SchemaNode::default()
    };
    SchemaIr::new(
        root,
        "https://json-schema.org/draft/2020-12/schema",
        "abc123",
        "test://",
    )
}

#[test]
fn generate_string_alias() {
    let ir = simple_ir(&type_val("string"));
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(code.contains("pub type Root = String;"));
}

#[test]
fn generate_integer_alias() {
    let ir = simple_ir(&type_val("integer"));
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(code.contains("pub type Root = i64;"));
}

#[test]
fn generate_number_alias_produces_f64() {
    // {"type":"number"} must generate f64, not i64, even though TypeSet
    // sets integer=true as a superset flag when number is present.
    let ir = simple_ir(&type_val("number"));
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("pub type Root = f64;"),
        "expected f64 for number schema, got:\n{code}"
    );
    assert!(
        !code.contains("pub type Root = i64;"),
        "i64 must not appear for number schema:\n{code}"
    );
}

#[test]
fn field_name_collision_foo_bar_vs_foo_underscore_bar() {
    // "foo-bar" and "foo_bar" both sanitize to "foo_bar".
    // The second one must be renamed to "foo_bar_1" to avoid a duplicate.
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("foo-bar".to_owned(), prop.clone());
    props.insert("foo_bar".to_owned(), prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("pub foo_bar:"),
        "expected foo_bar field in:\n{code}"
    );
    assert!(
        code.contains("pub foo_bar_1:"),
        "expected collision-renamed foo_bar_1 field in:\n{code}"
    );
}

#[test]
fn generate_struct_with_properties() {
    let name_prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("name".to_owned(), name_prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        object: schemaforge_ir::ObjectConstraints {
            required: vec!["name".to_owned()],
            ..Default::default()
        },
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(code.contains("pub struct Root"));
    assert!(code.contains("pub name: String"));
}

#[test]
fn snake_case_conversion() {
    assert_eq!(to_snake_case("camelCase"), "camel_case");
    assert_eq!(to_snake_case("snake_case"), "snake_case");
    assert_eq!(to_snake_case("kebab-case"), "kebab_case");
}

#[test]
fn pascal_case_conversion() {
    assert_eq!(to_pascal_case("foo_bar"), "FooBar");
    assert_eq!(to_pascal_case("hello-world"), "HelloWorld");
}

#[test]
fn dynamic_schema_emits_validator() {
    // An unconstrained schema (TypeSet::any, no properties) → Dynamic.
    let root = SchemaNode::default();
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("pub fn validate_root_json"),
        "expected validate_root_json in:\n{code}"
    );
    assert!(code.contains("pub type Root = serde_json::Value;"));
    assert!(code.contains("RootValidationError"));
}

#[test]
fn dynamic_schema_validator_is_callable() {
    let root = SchemaNode::default();
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(code.contains("serde_json::from_slice::<serde_json::Value>(input)"));
}

#[test]
fn nominal_struct_has_required_bitset_comment() {
    let name_prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("id".to_owned(), name_prop.clone());
    props.insert("label".to_owned(), name_prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        object: schemaforge_ir::ObjectConstraints {
            required: vec!["id".to_owned()],
            ..Default::default()
        },
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("Required-field bitset:"),
        "expected bitset comment in:\n{code}"
    );
    assert!(code.contains("required: id"));
}

#[test]
fn nominal_struct_has_dispatch_comment() {
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("id".to_owned(), prop.clone());
    props.insert("name".to_owned(), prop.clone());
    props.insert("value".to_owned(), prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("Property dispatch:"),
        "expected dispatch comment in:\n{code}"
    );
}

#[test]
fn max_bytes_exceeded_returns_error() {
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let opts = CodegenOptions {
        max_bytes: Some(1),
        ..CodegenOptions::default()
    };
    let result = generate(&ir, &opts);
    assert!(
        matches!(result, Err(CodegenError::SizeExceeded { .. })),
        "expected SizeExceeded but got: {result:?}"
    );
}

#[test]
fn max_bytes_within_limit_succeeds() {
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let opts = CodegenOptions {
        max_bytes: Some(usize::MAX),
        ..CodegenOptions::default()
    };
    assert!(generate(&ir, &opts).is_ok());
}

#[test]
fn defs_types_are_emitted() {
    let string_def = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut defs = indexmap::IndexMap::new();
    defs.insert("my-id".to_owned(), string_def);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        defs,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    // "my-id" → PascalCase → "MyId"
    assert!(
        code.contains("pub type MyId"),
        "expected MyId type in:\n{code}"
    );
}

#[test]
fn generate_never_schema() {
    let root = SchemaNode::boolean_schema(false);
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(code.contains("pub enum Root {}"));
}

#[test]
fn malicious_key_with_quotes_and_newlines_does_not_inject() {
    // A property key containing `"` and a literal newline must not break
    // out of the `#[serde(rename = "...")]` attribute and must not appear
    // as raw Rust code in the generated output.
    let malicious_key = "field\"\n; pub fn evil() {} //".to_owned();
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert(malicious_key, prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    // If the newline were NOT escaped, the key would break out of the
    // attribute string and `pub fn evil()` would appear as a standalone
    // definition on its own source line.  Check that no line starts with
    // `pub fn evil` (after trimming leading whitespace) to detect injection.
    let injected = code
        .lines()
        .any(|l| l.trim_start().starts_with("pub fn evil"));
    assert!(!injected, "injection detected in generated code:\n{code}");
    // The serde rename attribute must be present with the double-quote escaped as `\"`.
    assert!(
        code.contains(r#"serde(rename = "field\""#),
        "expected escaped serde rename attribute in:\n{code}"
    );
}

#[test]
fn doc_comment_title_with_newline_does_not_inject_code() {
    // A title containing a literal newline must NOT produce a multi-line
    // comment that causes the second line to appear as raw Rust source.
    // Use a struct (object with properties) so that emit_doc_comment is
    // actually called.
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("name".to_owned(), prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        title: Some("Evil\n; pub fn injected() {} //".to_owned()),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    let injected = code
        .lines()
        .any(|l| l.trim_start().starts_with("pub fn injected"));
    assert!(!injected, "newline in title injected code:\n{code}");
    // The title must still appear in the comment, but with newlines replaced.
    assert!(
        code.contains("Evil"),
        "title content must be present:\n{code}"
    );
}

#[test]
fn field_name_type_keyword_gets_raw_identifier() {
    // A JSON property named "type" must produce `r#type` as the field name
    // so the generated struct compiles as valid Rust.
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("type".to_owned(), prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("r#type"),
        "expected raw identifier r#type in:\n{code}"
    );
}

#[test]
fn field_name_self_keyword_gets_raw_identifier() {
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert("self".to_owned(), prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("r#self"),
        "expected raw identifier r#self in:\n{code}"
    );
}

#[test]
fn reserved_keywords_produce_valid_field_names() {
    // Check a sample of reserved keywords to ensure they are all wrapped.
    let keywords = ["ref", "match", "crate", "async", "fn", "let", "pub"];
    for kw in keywords {
        let prop = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("string")),
            ..SchemaNode::default()
        };
        let mut props = indexmap::IndexMap::new();
        props.insert(kw.to_owned(), prop);
        let root = SchemaNode {
            types: TypeSet::from_json(&serde_json::json!("object")),
            properties: props,
            ..SchemaNode::default()
        };
        let ir = SchemaIr::new(root, "", "abc", "test://");
        let code = generate(&ir, &CodegenOptions::default()).unwrap();
        assert!(
            code.contains(&format!("r#{kw}")),
            "expected r#{kw} for keyword property `{kw}` in:\n{code}"
        );
    }
}

#[test]
fn key_with_only_special_chars_gets_fallback_field_name() {
    // A key consisting entirely of characters outside [A-Za-z0-9_] must
    // produce a valid fallback identifier (`field_0`) rather than an empty
    // or invalid Rust identifier.
    let special_key = "!!!".to_owned();
    let prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut props = indexmap::IndexMap::new();
    props.insert(special_key, prop);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("pub field_0"),
        "expected fallback field name `field_0` in:\n{code}"
    );
}

#[test]
fn defs_named_root_gets_unique_name() {
    // A $def keyed "Root" must be renamed (e.g. "Root_1") because "Root"
    // is pre-seeded in the shared allocator for the root struct.
    let string_def = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut defs = indexmap::IndexMap::new();
    defs.insert("Root".to_owned(), string_def);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        defs,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    // The def "Root" must be renamed because "Root" is taken by the root struct.
    assert!(
        code.contains("pub type Root_1 = String;"),
        "expected Root_1 for def named Root in:\n{code}"
    );
    // Exactly one definition named "Root" must exist — the root struct itself.
    let root_count = code
        .lines()
        .filter(|l| {
            l.trim_start().starts_with("pub struct Root")
                || l.trim_start().starts_with("pub type Root ")
                || l.trim_start().starts_with("pub enum Root ")
        })
        .count();
    assert_eq!(
        root_count, 1,
        "expected exactly one top-level Root definition in:\n{code}"
    );
}

#[test]
fn nested_structs_with_colliding_names_get_unique_type_names() {
    // "foo-bar" and "foo_bar" both sanitize to the same PascalCase suffix "FooBar".
    // The shared allocator must ensure the two nested struct names are unique.
    let inner_prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut inner_props = indexmap::IndexMap::new();
    inner_props.insert("x".to_owned(), inner_prop);
    let nested = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: inner_props,
        ..SchemaNode::default()
    };
    let mut root_props = indexmap::IndexMap::new();
    root_props.insert("foo-bar".to_owned(), nested.clone());
    root_props.insert("foo_bar".to_owned(), nested);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: root_props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("RootFooBar"),
        "expected RootFooBar nested type in:\n{code}"
    );
    assert!(
        code.contains("RootFooBar_1"),
        "expected RootFooBar_1 nested type for colliding foo_bar in:\n{code}"
    );
    // Count struct definitions: Root, RootFooBar, RootFooBar_1 = 3.
    let struct_count = code.matches("pub struct ").count();
    assert_eq!(
        struct_count, 3,
        "expected exactly 3 struct definitions, got {struct_count} in:\n{code}"
    );
}

// ── $ref + constraint codegen ─────────────────────────────────────────────────

#[test]
fn ref_with_min_length_sibling_generates_string_not_value() {
    // A schema of the form {$ref: ..., minLength: 1} produces an allOf node
    // after compilation.  The codegen must resolve that to `String` (based on
    // the inferred type) rather than falling through to `serde_json::Value`.
    let mut c = Compiler::new();
    let src = r##"{
        "$defs": {"S": {"type": "string"}},
        "$ref": "#/$defs/S",
        "minLength": 1
    }"##;
    let ir = c.compile_json("test://ref-minlength.json", src).unwrap();
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("pub type Root = String;"),
        "expected String alias for $ref+minLength, got:\n{code}"
    );
    assert!(
        !code.contains("serde_json::Value"),
        "$ref+minLength must not emit serde_json::Value:\n{code}"
    );
}

#[test]
fn empty_enum_codegen_generates_never_type() {
    // An empty `"enum": []` schema is never satisfiable; codegen must emit an
    // uninhabited enum type, not a `serde_json::Value` alias.
    let mut c = Compiler::new();
    let ir = c
        .compile_json("test://empty-enum.json", r#"{"enum":[]}"#)
        .unwrap();
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    assert!(
        code.contains("pub enum Root {}"),
        "expected uninhabited enum for empty enum schema, got:\n{code}"
    );
    assert!(
        !code.contains("serde_json::Value"),
        "empty enum must not emit serde_json::Value:\n{code}"
    );
}

#[test]
fn nested_struct_field_uses_concrete_type_not_value() {
    // When a property has nested properties, its field type must be the
    // generated named struct, not serde_json::Value.
    let inner_prop = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("string")),
        ..SchemaNode::default()
    };
    let mut inner_props = indexmap::IndexMap::new();
    inner_props.insert("name".to_owned(), inner_prop);
    let nested = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: inner_props,
        ..SchemaNode::default()
    };
    let mut root_props = indexmap::IndexMap::new();
    root_props.insert("child".to_owned(), nested);
    let root = SchemaNode {
        types: TypeSet::from_json(&serde_json::json!("object")),
        properties: root_props,
        ..SchemaNode::default()
    };
    let ir = SchemaIr::new(root, "", "abc", "test://");
    let code = generate(&ir, &CodegenOptions::default()).unwrap();
    // The `child` field must reference the concrete struct name, not Value.
    assert!(
        code.contains("RootChild"),
        "expected RootChild nested struct in:\n{code}"
    );
    assert!(
        !code.contains("child: Option<serde_json::Value>")
            && !code.contains("child: serde_json::Value"),
        "child field must not use serde_json::Value when a concrete struct exists:\n{code}"
    );
}
