# RFC 0001 — SchemaForge Hybrid Schema Compiler

| Field       | Value            |
| ----------- | ---------------- |
| Status      | Accepted         |
| Authors     | SchemaForge team |
| Created     | 2025-01-01       |
| Last update | 2026-07-14       |

---

## Abstract

SchemaForge is a Rust workspace that compiles JSON Schema documents into a
**Canonical Semantic IR** and then emits either native Rust validation code
(ahead-of-time path) or a compact serialisable **Runtime Plan** (interpreted
path). The two paths share the same front-end and IR, so behaviour is
identical across outputs. This document describes the architecture,
eight delivery phases, security stance, and dialect strategy.

---

## 1. Motivation

Existing Rust JSON Schema libraries choose one of two extremes:

- **Fully interpreted** — maximum dialect coverage, poor peak throughput.
- **Pure code-generation** — fast, but tied to a single dialect and schema
  version, with long compile times when embedded in user crates.

SchemaForge occupies the middle ground: compile once off the hot path, run fast
on the hot path, and support multiple dialects through a common IR.

---

## 2. Goals

- Produce correct, auditable validators for JSON Schema Draft 2020-12 as the
  canonical dialect.
- Support Draft 07, Draft 2019-09, and OpenAPI 3.x through dialect-specific
  desugaring into the canonical IR.
- Provide a native Rust code-generation back-end (no proc-macro, no derive
  magic).
- Provide a serialisable Runtime Plan back-end for embedding in constrained
  environments (WASM, Python wheels, Node.js).
- Resolve all external references **offline by default**; network fetches are
  opt-in and auditable.
- Forbid `unsafe` in every crate except `schemaforge-node` (FFI boundary) and
  `schemaforge-python` (PyO3 requirement), both with documented justification.
- Expose a stable diagnostic interface so editor integrations receive
  structured errors with spans.

## 3. Non-Goals

- Full Relax NG or XSD support.
- A general-purpose JSON serialiser / deserialiser (use `serde_json`).
- Runtime schema mutation after compilation.
- A web UI or hosted validation service.

---

## 4. Architecture Overview

```text
  ┌──────────────────────────────────────────────────────┐
  │                   Source layer                        │
  │  schemaforge-source  ·  schemaforge-diagnostics       │
  └────────────────────┬─────────────────────────────────┘
                       │  (raw bytes → parsed AST)
  ┌────────────────────▼─────────────────────────────────┐
  │                  Dialect layer                        │
  │  schemaforge-dialect  ·  schemaforge-resolver         │
  └────────────────────┬─────────────────────────────────┘
                       │  (dialect detection, $ref resolution,
                       │   desugaring to Draft 2020-12 nodes)
  ┌────────────────────▼─────────────────────────────────┐
  │              Canonical Semantic IR                    │
  │               schemaforge-ir                          │
  └──────────────┬────────────────────┬──────────────────┘
                 │                    │
  ┌──────────────▼──────┐  ┌──────────▼───────────────────┐
  │  Native code-gen    │  │  Runtime Plan emitter         │
  │  schemaforge-       │  │  schemaforge-runtime          │
  │  codegen-rust       │  └──────────────────────────────┘
  └─────────────────────┘
                 │
  ┌──────────────▼──────────────────────────────────────┐
  │  Language bindings                                   │
  │  schemaforge-python · schemaforge-node               │
  └──────────────────────────────────────────────────────┘
  ┌──────────────────────────────────────────────────────┐
  │  Cross-cutting                                        │
  │  schemaforge-analysis · schemaforge-formats           │
  │  schemaforge-jsonschema · schemaforge-openapi         │
  │  schemaforge-compiler (orchestration)                 │
  │  schemaforge-cli (binary entry point)                 │
  └──────────────────────────────────────────────────────┘
```

Each crate has a single responsibility; the compiler crate is the only one
allowed to depend on all others.

---

## 5. Canonical Semantic IR

The IR (`schemaforge-ir`) is a typed Rust enum tree that represents every
Draft 2020-12 keyword. Dialect-specific desugaring is complete before the IR
is constructed; downstream crates never see Draft 07 or 2019-09 nodes.

Key invariants:

- All `$ref` values are fully resolved absolute URIs (no relative references
  survive into the IR).
- `$recursiveRef` / `$dynamicRef` are normalised to dynamic-dispatch nodes.
- `format` assertions carry a resolved `FormatValidator` handle (or are
  annotated as non-asserting depending on configuration).
- The IR is `Clone + Serialize + Deserialize + PartialEq + Debug` so it can be
  round-tripped for caching and testing.

---

## 6. Runtime Plan Format

The Runtime Plan (`schemaforge-runtime`) is a compact, versioned binary
(MessagePack or JSON) representation of validation logic. It is designed to
be embedded in Python wheels and Node.js packages without a Rust compiler
dependency at distribution time.

Properties:

- Version-stamped; forward-incompatible changes bump the major version.
- Deterministic: identical IR → identical plan bytes (hash-stable for caching).
- Interpreted by a small, dependency-free evaluator that can run inside WASM.

---

## 7. Dialect Strategy

| Dialect            | Input support | Canonical output |
| ------------------ | :-----------: | :--------------: |
| Draft 07           |       ✓       |  ✓ (desugared)   |
| Draft 2019-09      |       ✓       |  ✓ (desugared)   |
| Draft 2020-12      |  ✓ (native)   |        ✓         |
| OpenAPI 3.0 Schema |       ✓       |  ✓ (desugared)   |
| OpenAPI 3.1 Schema |       ✓       |        ✓         |

Desugaring rules live in `schemaforge-dialect`. Each dialect adapter
transforms its keyword set into Draft 2020-12 equivalents before handing off
to the resolver. This keeps the IR and all downstream crates dialect-agnostic.

---

## 8. Delivery Phases

### Phase 0 — Workspace skeleton

Establish the Cargo workspace, shared dependency versions, lint policy
(`unsafe_code = "forbid"`, `-D warnings`, Clippy pedantic), and CI matrix.
Deliverable: green CI on an empty workspace.

### Phase 1 — Source & diagnostics

Implement `schemaforge-source` (byte loading, UTF-8 validation, span tracking)
and `schemaforge-diagnostics` (structured error/warning type with file + span).
No parsing yet; just the I/O and error envelope.

### Phase 2 — Resolver & offline default

Implement `schemaforge-resolver` with a `FileSystemLoader` as the default and
an explicit opt-in `HttpLoader` behind a feature flag. Implement `$id`
walking, `$anchor` registration, and `$ref`→URI resolution. All resolution
operations are logged for audit.

### Phase 3 — Dialect layer & IR

Implement `schemaforge-dialect` adapters for Draft 07, 2019-09, 2020-12, and
OpenAPI. Build `schemaforge-ir` with the canonical node types. Wire them
together: parse → detect dialect → desugar → construct IR.

### Phase 4 — Analysis & formats

Implement `schemaforge-analysis` (schema reachability, unused anchor detection,
cycle detection) and `schemaforge-formats` (format keyword registry with
built-in validators for `date`, `uri`, `email`, `uuid`, etc.).

### Phase 5 — Native code-generation

Implement `schemaforge-codegen-rust`: IR → Rust source text (no proc-macros).
Output must compile with `#![forbid(unsafe_code)]` and pass Clippy pedantic.
Produce a test harness so generated code is validated against the JSON Schema
Test Suite.

### Phase 6 — Runtime Plan & bindings

Implement `schemaforge-runtime` plan format and evaluator. Wrap with
`schemaforge-python` (PyO3) and `schemaforge-node` (napi-rs). Both FFI crates
are the only place `unsafe` is permitted, with documented rationale per
function.

### Phase 7 — WASM & hardening

Investigate WASM compilation of the Runtime Plan evaluator. Harden against
hostile inputs (ReDoS, deeply-nested schemas, gigantic `$defs`). Conduct
threat-model review. See `docs/wasm-feasibility.md`.

---

## 9. Security Considerations

### 9.1 SSRF (Server-Side Request Forgery)

The resolver is offline by default. When `HttpLoader` is enabled, callers must
supply an allowlist of permitted URI prefixes. Attempts to resolve URIs
outside the allowlist are rejected with a diagnostic, not silently dropped.

### 9.2 ReDoS

Format validators that use regular expressions are compiled once at startup and
stored as `regex::Regex` handles. Catastrophic backtracking is mitigated by
the `regex` crate's linear-time engine; unbounded user-supplied patterns are
rejected.

### 9.3 Codegen Expansion

The code-generation back-end enforces a configurable maximum output size (lines
of Rust) and a maximum recursion depth for nested schemas. Schemas that exceed
these limits produce a diagnostic instead of hanging.

### 9.4 Hostile Schemas

- Circular `$ref` chains are detected during resolution and reported as errors,
  not followed infinitely.
- `$defs` with thousands of entries are processed incrementally; memory use is
  bounded by a configurable node limit.
- `allOf` / `anyOf` / `oneOf` with exponential combinations are flagged by the
  analysis pass.

---

## 10. Open Questions

1. Should the Runtime Plan adopt MessagePack or CBOR? (Decision deferred to
   Phase 6 based on ecosystem tooling.)
2. Can the WASM evaluator share the same plan bytes as the native evaluator, or
   does WASM require a separate layout? (See `docs/wasm-feasibility.md`.)
3. What is the minimum supported Rust edition? (Currently 2024; may relax for
   binding crates.)

---

## 11. Alternatives Considered

| Alternative                   | Reason rejected                                       |
| ----------------------------- | ----------------------------------------------------- |
| Fork `jsonschema-rs`          | No native codegen; IR not extensible to multi-dialect |
| Proc-macro derive approach    | Long compile times; hard to audit generated code      |
| Single fully-interpreted path | Insufficient peak throughput for codegen use-cases    |
| Full WASM-first design        | Native code-gen performance requirements not met      |

---

## 12. References

- [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12)
- [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite)
- [OpenAPI Specification 3.1](https://github.com/OAI/OpenAPI-Specification)
- `docs/adr/0004-hybrid-ahead-of-time-compiler.md`
- `docs/adr/0005-canonical-semantic-ir.md`
- `docs/adr/0006-runtime-plan-format.md`
- `docs/adr/0007-offline-resolver-default.md`
- `docs/threat-model.md`
- `docs/wasm-feasibility.md`
