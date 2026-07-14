# Schemaforge Hybrid Compiler — Task Tracker

## Phase 0 — Foundation (COMPLETE)

- [x] Remove `workspace-core` and `workspace-cli`
- [x] Update root `Cargo.toml` workspace members and dependencies
- [x] `schemaforge-source` — SourceFile, SourceId, Span, SourceMap
- [x] `schemaforge-diagnostics` — Diagnostic, Severity, DiagnosticBag
- [x] `schemaforge-dialect` — Dialect detection, Draft 2020-12 vocabularies
- [x] `schemaforge-resolver` — OfflineResolver, FileResolver, URI resolution
- [x] `schemaforge-ir` — TypeSet, SchemaNode, SchemaIr (full data model)
- [x] `schemaforge-runtime` — RUNTIME_PLAN const with keyword phases
- [x] `schemaforge-compiler` — JSON/YAML → IR pipeline, SHA-256 digest
- [x] `schemaforge-cli` — compile, validate, codegen, info subcommands

## Phase 1 — JSON Schema (COMPLETE)

- [x] `schemaforge-jsonschema` — Draft 2020-12 validator
  - [x] Core vocabulary (`$ref`, `$id`)
  - [x] Applicator vocabulary (`allOf`, `anyOf`, `oneOf`, `not`, `if/then/else`, `properties`, `additionalProperties`, `patternProperties`, `items`, `prefixItems`, `contains`)
  - [x] Validation vocabulary (`type`, `enum`, `const`, `minLength`, `maxLength`, `pattern`, `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `multipleOf`, `minItems`, `maxItems`, `uniqueItems`, `required`, `dependentRequired`, `minProperties`, `maxProperties`)
  - [x] Unevaluated vocabulary (`unevaluatedProperties`, `unevaluatedItems` — conservative pass)
  - [x] Format vocabulary (annotation-default, assertion-optional)
- [x] `schemaforge-formats` — 15+ format validators

## Phase 2 — Analysis (COMPLETE)

- [x] `schemaforge-analysis` — Type inference, TypeSet narrowing, `allOf` intersection
  - [x] Enum value type narrowing
  - [x] Const value type narrowing
  - [x] `allOf` type set intersection with contradiction detection
  - [x] Recursive property and item type analysis

## Phase 3 — Rust Codegen (COMPLETE)

- [x] `schemaforge-codegen-rust` — Rust struct/enum/type-alias generation
  - [x] Object → `struct` with serde derives
  - [x] `anyOf`/`oneOf` → untagged `enum`
  - [x] Primitive types → type aliases
  - [x] `camelCase` → `snake_case` field names with `#[serde(rename)]`
  - [x] Nested struct generation
  - [x] Optional field wrapping

## Phase 4 — Formats (COMPLETE)

- [x] `schemaforge-formats` — Format registry with annotation/assertion modes
  - [x] date-time, date, time, duration
  - [x] email, idn-email
  - [x] hostname, idn-hostname
  - [x] ipv4, ipv6
  - [x] uri, uri-reference, iri, iri-reference
  - [x] uuid
  - [x] json-pointer, relative-json-pointer
  - [x] regex

## Phase 5 — OpenAPI (COMPLETE)

- [x] `schemaforge-openapi` — OpenAPI 3.0 and 3.1 document parsing
  - [x] Version detection
  - [x] `components/schemas` extraction
  - [x] Path item request/response schema extraction
  - [x] OAS 3.0 `nullable` → JSON Schema `type` array adaptation

## Phase 6 — Python Bindings (SCAFFOLDED)

- [x] `schemaforge-python` — Safe Rust API for PyO3 wrapping
  - [ ] PyO3 FFI module (requires `unsafe_code` — see ADR-0003, feature-gated)
  - [ ] Python wheel build via maturin
  - [ ] `schemaforge.validate(schema, instance)` Python function
  - [ ] `schemaforge.compile(schema)` returning `CompiledSchema` Python class

## Phase 7 — Node.js Bindings (SCAFFOLDED)

- [x] `schemaforge-node` — Safe Rust API for napi-rs wrapping
  - [ ] napi-rs FFI module (requires `unsafe_code` — see ADR-0003, feature-gated)
  - [ ] NAPI build via `napi build`
  - [ ] `validate(schema, instance)` JS function
  - [ ] `CompiledSchema` JS class with `.validate()` method

## Remaining Gaps

### JSON Schema Validator
- [ ] `$dynamicRef` / `$dynamicAnchor` (Draft 2020-12 recursive refs)
- [ ] Full `$ref` resolution through the OfflineResolver in the validator
- [ ] Proper unevaluated tracking (annotation collection pass)
- [ ] `propertyNames` keyword
- [ ] `contentEncoding` / `contentMediaType` / `contentSchema`
- [ ] `dependentSchemas` keyword
- [ ] Conformance test suite integration (json-schema-test-suite)

### Compiler
- [ ] YAML → JSON Schema with anchor/alias expansion
- [ ] Cyclic `$ref` detection and safe handling
- [ ] Multi-document bundle support

### Analysis
- [ ] `anyOf` / `oneOf` union type inference
- [ ] Constraint propagation across `if/then/else`
- [ ] Dependent field detection

### Codegen (Rust)
- [ ] `$ref` → named type references (not inline)
- [ ] Enum string values → Rust `enum` variants
- [ ] `x-rust-type` extension for custom type overrides

### OpenAPI
- [ ] `$ref` resolution across component boundaries
- [ ] Header / parameter schema extraction
- [ ] Webhook schemas (OAS 3.1)

### Python / Node Bindings
- [ ] Actual FFI glue with PyO3 / napi-rs (see ADR-0003)
- [ ] Error type mapping to Python exceptions / JS errors
- [ ] Build configuration (`maturin.toml`, `package.json`)

### Infrastructure
- [ ] Conformance test harness integration
- [ ] Benchmark suite (`benchmarks/` directory)
- [ ] Fuzzing targets (`fuzz/` directory)
