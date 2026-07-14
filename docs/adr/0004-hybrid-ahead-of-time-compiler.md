# ADR 0004 — Hybrid Ahead-of-Time Compiler Architecture

| Field   | Value        |
|---------|--------------|
| Status  | Accepted     |
| Date    | 2026-07-14   |

## Context

SchemaForge needs to serve two audiences with different performance profiles:

1. **Build-time consumers** — code generators embedded in Cargo build scripts
   that need to emit validated Rust types from JSON Schema documents.
2. **Runtime consumers** — Python wheels and Node.js packages that validate
   JSON at request time without a Rust compiler available in the deployment
   environment.

A purely interpreted validator is not fast enough for the build-time consumer,
and a purely code-generated validator cannot be distributed in a pre-compiled
Python/Node package without re-compiling on each target platform.

## Decision

We adopt a **hybrid** architecture with two output paths that share a common
front-end and Canonical Semantic IR:

- **Ahead-of-Time (AoT) path** — the IR is lowered to Rust source text by
  `schemaforge-codegen-rust`.  This path is used by build scripts and CLI
  users who can tolerate a compile step.
- **Runtime Plan path** — the IR is serialised to a compact, versioned plan by
  `schemaforge-runtime`.  This plan is interpreted at validation time by a
  small evaluator that ships inside Python/Node packages.

Both paths are driven by the same `schemaforge-compiler` orchestration crate.
Identical input schemas must produce semantically equivalent validation
behaviour on both paths; this property is enforced by shared conformance tests.

## Consequences

**Positive:**

- Users choose the path that fits their deployment constraints without any
  API-level changes.
- Bug fixes in the shared IR and front-end automatically benefit both paths.
- Conformance tests can be written once and run against both outputs.

**Negative / trade-offs:**

- Two output back-ends must be maintained and kept in sync.
- The Runtime Plan evaluator adds code that must be audited for correctness and
  security independently of the AoT path.
- Serialisation format versioning adds long-term maintenance overhead.
