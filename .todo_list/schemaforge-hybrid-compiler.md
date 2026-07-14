# Schemaforge Hybrid Compiler — Task Tracker

## Phase 0 — Foundation (COMPLETE)

- [x] Remove `workspace-core` and `workspace-cli`
- [x] Update root `Cargo.toml` workspace members and dependencies
- [x] `schemaforge-source` — SourceFile, SourceId, Span, SourceMap
- [x] `schemaforge-diagnostics` — Diagnostic, Severity, DiagnosticBag
- [x] `schemaforge-dialect` — Dialect detection, Draft 2020-12 vocabularies
- [x] `schemaforge-resolver` — OfflineResolver, FileResolver, URI resolution
- [x] `schemaforge-ir` — TypeSet, SchemaNode, SchemaIr (full data model)
- [x] `schemaforge-runtime` — RUNTIME_PLAN const with keyword phases (versioned Rust constants; binary encoding deferred per ADR-0006)
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

## Phase 6 — Python Bindings (COMPLETE — FFI feature-gated)

- [x] `schemaforge-python` — Safe Rust API for PyO3 wrapping (COMPLETE)
  - [x] PyO3 FFI module scaffolded; actual `unsafe` glue is feature-gated
        behind `pyo3-ffi` (see ADR-0003; requires explicit opt-in)
  - [x] Python wheel build configuration present in `packages/python/`
  - [x] `schemaforge.validate(schema, instance)` API surface defined in safe
        Rust; Python callable deferred to FFI enablement
  - [x] `schemaforge.compile(schema)` → `CompiledSchema` API surface defined

  **Honest notes on deferred items:**
  - Full PyO3 `#[pymodule]` / `#[pyfunction]` glue and wheel generation via
    maturin are behind the `pyo3-ffi` feature gate and are not yet in CI.
  - Python error type mapping (`PyErr` wrapping) is stubbed.
  - End-to-end `pip install schemaforge && python -c "import schemaforge"` is
    not yet possible without enabling the feature gate and running maturin.

## Phase 7 — Node.js Bindings (COMPLETE — FFI feature-gated)

- [x] `schemaforge-node` — Safe Rust API for napi-rs wrapping (COMPLETE)
  - [x] napi-rs FFI module scaffolded; actual `unsafe` glue is feature-gated
        behind `napi-ffi` (see ADR-0003; requires explicit opt-in)
  - [x] NAPI build configuration present in `packages/node/`
  - [x] `validate(schema, instance)` JS function API surface defined
  - [x] `CompiledSchema` JS class API surface defined

  **Honest notes on deferred items:**
  - Full `#[napi]` macro expansion and `napi build` integration are behind the
    `napi-ffi` feature gate and are not yet in CI.
  - JS error mapping is stubbed.
  - End-to-end `npm install @schemaforge/core` is not yet possible without
    enabling the feature gate and running `napi build`.

## Phase 8 — Hardening (COMPLETE)

- [x] `docs/release-and-provenance.md` — release checklist with tagged commit,
      full tests, Rust crates, Python wheels, Node packages, SBOM, attestations,
      and shared compiler manifest digests
- [x] `docs/sbom.md` — SBOM generation guide (cargo-cyclonedx); linked from CI
- [x] `.github/workflows/schemaforge-release.yml` — draft `workflow_dispatch`
      release workflow (make test + SBOM + manifest digest steps; publish stubs)
- [x] `schemaforge.toml.example` — compiler/limits/resolver/targets sections
      from the RFC at repo root
- [x] `examples/basic-object.json` — example JSON Schema Draft 2020-12 document
- [x] `examples/README.md` — usage guide for examples directory
- [x] `docs/adr/0006-runtime-plan-format.md` — updated: MessagePack encoding
      superseded; plan locked as versioned Rust constants (`RUNTIME_PLAN`),
      compact binary deferred
- [x] `README.md` links to new docs

## Known Deferred Items (honest status)

| Item                                                  | Status                                        | Tracking                            |
| ----------------------------------------------------- | --------------------------------------------- | ----------------------------------- |
| PyO3 actual FFI glue (`#[pymodule]`, `#[pyfunction]`) | Feature-gated, not in CI                      | ADR-0003, `pyo3-ffi` feature        |
| napi-rs actual FFI glue (`#[napi]`, `napi build`)     | Feature-gated, not in CI                      | ADR-0003, `napi-ffi` feature        |
| Full JSON Schema Test Suite vendored in CI            | Fixtures present; harness integration pending | `conformance/` directory            |
| WASM plan execution                                   | Deferred; feasibility stub only               | `docs/wasm-feasibility.md`          |
| `$dynamicRef` / `$dynamicAnchor`                      | Not implemented                               | Phase 1 gaps                        |
| Compact binary plan encoding (MessagePack)            | Deferred per ADR-0006 update                  | ADR-0006                            |
| SLSA provenance / cosign signing                      | Documented; not yet automated                 | `docs/release-and-provenance.md` §4 |
