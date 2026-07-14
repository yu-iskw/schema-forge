# 2. Schemaforge workspace crate structure

Date: 2026-07-14

## Status

Accepted

## Context

The repository template ships with placeholder crates (`workspace-core`,
`workspace-cli`). The Schemaforge hybrid schema compiler requires a
domain-specific set of crates that clearly separate concerns.

## Decision

Replace the placeholder crates with the following purpose-built crates:

| Crate | Role |
|---|---|
| `schemaforge-source` | Source file loading, spans |
| `schemaforge-diagnostics` | Structured error/warning reporting |
| `schemaforge-dialect` | JSON Schema dialect detection |
| `schemaforge-resolver` | URI resolution, offline-first |
| `schemaforge-ir` | Compiled intermediate representation |
| `schemaforge-runtime` | Compile-time keyword processing plan |
| `schemaforge-formats` | Format keyword validators |
| `schemaforge-jsonschema` | Draft 2020-12 validator |
| `schemaforge-analysis` | Type inference over IR |
| `schemaforge-compiler` | Source → IR pipeline |
| `schemaforge-codegen-rust` | IR → Rust code generator |
| `schemaforge-openapi` | OpenAPI 3.x document parsing |
| `schemaforge-python` | Python binding scaffold |
| `schemaforge-node` | Node.js binding scaffold |
| `schemaforge-cli` | `schemaforge` binary |

All crates share workspace-level `edition`, `license`, `authors`, and lint
policy via `[workspace.package]` and `[workspace.lints]`.

## Consequences

- Each crate has a single clear responsibility.
- Downstream consumers can depend on only the crates they need.
- The `schemaforge-cli` binary is the integration point for manual testing.
- Adding new dialect support or codegen backends is a new crate, not a change
  to an existing one.
