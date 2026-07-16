//! Criterion benchmarks for `schemaforge-jsonschema`.
//!
//! See `benchmarks/README.md` at the workspace root for methodology notes.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use schemaforge_jsonschema::{ValidationOptions, Validator};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Schema fixtures used across benchmarks
// ---------------------------------------------------------------------------

fn simple_type_schema() -> Value {
    json!({ "type": "string" })
}

fn object_schema() -> Value {
    json!({
        "type": "object",
        "required": ["id", "name"],
        "properties": {
            "id":    { "type": "integer" },
            "name":  { "type": "string" },
            "email": { "type": "string" },
            "active": { "type": "boolean" }
        },
        "additionalProperties": false
    })
}

fn allof_schema() -> Value {
    json!({
        "allOf": [
            { "type": "object", "required": ["id"] },
            { "properties": { "id": { "type": "integer" } } }
        ]
    })
}

fn nested_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "required": ["id", "name"],
                "properties": {
                    "id":   { "type": "integer" },
                    "name": { "type": "string" },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Benchmark: compile (Validator::new)
// ---------------------------------------------------------------------------

fn bench_compile(c: &mut Criterion) {
    let schemas: &[(&str, Value)] = &[
        ("simple_type", simple_type_schema()),
        ("object", object_schema()),
        ("allOf", allof_schema()),
        ("nested", nested_schema()),
    ];

    let mut group = c.benchmark_group("compile");
    for (name, schema) in schemas {
        group.bench_with_input(BenchmarkId::from_parameter(name), schema, |b, s| {
            b.iter(|| {
                Validator::new(s, ValidationOptions::default()).expect("schema must compile")
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: validate (pre-compiled validator)
// ---------------------------------------------------------------------------

fn bench_validate_valid(c: &mut Criterion) {
    let cases: &[(&str, Value, Value)] = &[
        ("simple_type", simple_type_schema(), json!("hello world")),
        (
            "object",
            object_schema(),
            json!({ "id": 1, "name": "Alice", "email": "a@b.com", "active": true }),
        ),
        ("allOf", allof_schema(), json!({ "id": 42 })),
        (
            "nested",
            nested_schema(),
            json!({ "user": { "id": 7, "name": "Bob", "tags": ["rust", "json"] } }),
        ),
    ];

    let mut group = c.benchmark_group("validate/valid");
    for (name, schema, instance) in cases {
        let validator =
            Validator::new(schema, ValidationOptions::default()).expect("schema must compile");
        group.bench_with_input(BenchmarkId::from_parameter(name), instance, |b, inst| {
            b.iter(|| validator.validate(inst));
        });
    }
    group.finish();
}

fn bench_validate_invalid(c: &mut Criterion) {
    let cases: &[(&str, Value, Value)] = &[
        ("simple_type", simple_type_schema(), json!(42)),
        (
            "object",
            object_schema(),
            json!({ "id": "not-an-int", "name": "Alice" }),
        ),
        ("allOf", allof_schema(), json!({ "name": "no id here" })),
        (
            "nested",
            nested_schema(),
            json!({ "user": { "id": "bad", "name": 0, "tags": [1, 2] } }),
        ),
    ];

    let mut group = c.benchmark_group("validate/invalid");
    for (name, schema, instance) in cases {
        let validator =
            Validator::new(schema, ValidationOptions::default()).expect("schema must compile");
        group.bench_with_input(BenchmarkId::from_parameter(name), instance, |b, inst| {
            b.iter(|| validator.validate(inst));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: compile + validate in one shot
// ---------------------------------------------------------------------------

fn bench_compile_and_validate(c: &mut Criterion) {
    let schema = object_schema();
    let instance = json!({ "id": 1, "name": "Alice" });

    c.bench_function("compile_and_validate/object", |b| {
        b.iter(|| {
            let v =
                Validator::new(&schema, ValidationOptions::default()).expect("schema must compile");
            v.validate(&instance)
        });
    });
}

criterion_group!(
    benches,
    bench_compile,
    bench_validate_valid,
    bench_validate_invalid,
    bench_compile_and_validate,
);
criterion_main!(benches);
