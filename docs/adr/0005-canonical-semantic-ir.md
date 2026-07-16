# ADR 0005 — Canonical Semantic IR Based on Draft 2020-12

| Field  | Value      |
| ------ | ---------- |
| Status | Accepted   |
| Date   | 2026-07-14 |

## Context

SchemaForge must accept schemas written in multiple dialects (Draft 07,
Draft 2019-09, Draft 2020-12, OpenAPI 3.x) and produce consistent validation
output regardless of dialect. Without a shared intermediate representation,
each back-end (code-gen, runtime plan) would need its own dialect-handling
logic, multiplying the surface area for bugs.

Options considered:

1. A dialect-neutral IR designed from scratch.
2. Draft 2020-12 as the canonical IR, with older dialects desugared to it.
3. A union IR that preserves dialect-specific nodes and resolves them lazily.

## Decision

We use **JSON Schema Draft 2020-12 as the canonical IR dialect**. All other
dialects are desugared into Draft 2020-12 equivalents before the IR is
constructed. The IR lives in `schemaforge-ir` and is a typed Rust enum tree.

Desugaring is performed by `schemaforge-dialect` adapters. After desugaring:

- No dialect-specific node survives into the IR.
- All `$ref` values are fully resolved absolute URIs.
- `$recursiveRef` / `$dynamicRef` are normalised to explicit dynamic-dispatch
  nodes.
- `format` assertions carry resolved `FormatValidator` handles.

The IR derives `Clone`, `Serialize`, `Deserialize`, `PartialEq`, and `Debug`
so it can be round-tripped for caching, golden-file testing, and plan
serialisation.

## Consequences

**Positive:**

- Back-end crates (`schemaforge-codegen-rust`, `schemaforge-runtime`) are
  completely dialect-agnostic.
- Adding a new dialect only requires a new `schemaforge-dialect` adapter; no
  back-end changes are needed.
- IR equality is meaningful: two schemas that are semantically equivalent but
  written in different dialects produce identical IR nodes.

**Negative / trade-offs:**

- Desugaring from Draft 07 to Draft 2020-12 is not always lossless in edge
  cases (e.g., `definitions` vs `$defs`, `id` vs `$id`). We document the
  known semantic gaps.
- Draft 2020-12 keywords that have no older-dialect equivalent (e.g.,
  `unevaluatedProperties`) are not available to inputs written in older
  dialects; this is intentional and is surfaced as a diagnostic.
