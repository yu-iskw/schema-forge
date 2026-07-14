# SchemaForge

A hybrid JSON Schema compiler written in Rust.  SchemaForge compiles JSON
Schema documents (Draft 07, Draft 2019-09, Draft 2020-12, OpenAPI 3.x) into
either **native Rust validation code** (ahead-of-time path) or a compact
**Runtime Plan** (interpreted path).  Both paths share the same front-end and
Canonical Semantic IR, so behaviour is identical across outputs.

## AI Assistants

- Codex should use [AGENTS.md](./AGENTS.md) for repo-specific instructions and verification expectations.
- Claude-specific workflow details remain in `CLAUDE.md` and `.claude/`.
- Shared reusable skills are authored in `.claude/skills` and exposed to other agents through the symlink mirror in `.agents/skills`.

## Architecture

```
  Source & Diagnostics
    schemaforge-source · schemaforge-diagnostics
        ↓
  Dialect layer (desugaring to Draft 2020-12)
    schemaforge-dialect · schemaforge-resolver
        ↓
  Canonical Semantic IR
    schemaforge-ir
        ↓               ↓
  AoT code-gen      Runtime Plan
  schemaforge-      schemaforge-runtime
  codegen-rust
        ↓               ↓
                  Language bindings
                  schemaforge-python · schemaforge-node
```

Cross-cutting crates: `schemaforge-analysis`, `schemaforge-formats`,
`schemaforge-jsonschema`, `schemaforge-openapi`, `schemaforge-compiler`
(orchestration), `schemaforge-cli` (binary entry point).

## Crate Overview

| Crate                     | Role                                                    |
|---------------------------|---------------------------------------------------------|
| `schemaforge-source`      | Byte loading, UTF-8 validation, span tracking           |
| `schemaforge-diagnostics` | Structured error/warning type with file + span          |
| `schemaforge-dialect`     | Dialect detection and desugaring adapters               |
| `schemaforge-resolver`    | `$ref` resolution; offline by default, HTTP opt-in      |
| `schemaforge-ir`          | Canonical Semantic IR (Draft 2020-12 node types)        |
| `schemaforge-analysis`    | Reachability, cycle detection, unused anchor analysis   |
| `schemaforge-formats`     | `format` keyword registry and built-in validators       |
| `schemaforge-jsonschema`  | High-level JSON Schema API surface                      |
| `schemaforge-openapi`     | OpenAPI schema parsing and dialect bridge               |
| `schemaforge-compiler`    | Orchestration: ties all crates into one pipeline        |
| `schemaforge-codegen-rust`| IR → Rust source text (no proc-macros)                 |
| `schemaforge-runtime`     | Runtime Plan format, emitter, and evaluator             |
| `schemaforge-python`      | PyO3 bindings (only crate allowing `unsafe` for FFI)    |
| `schemaforge-node`        | napi-rs bindings (only crate allowing `unsafe` for FFI) |
| `schemaforge-cli`         | `sfg` binary entry point                                |

## Quick Start

```bash
# Compile a schema to Rust (AoT path)
sfg compile --schema path/to/schema.json --out src/generated/

# Produce a Runtime Plan (interpreted path)
sfg compile --schema path/to/schema.json --plan out/schema.plan

# Inspect a plan in human-readable form
sfg inspect out/schema.plan

# Show transitive URI dependencies (useful before enabling remote loading)
sfg deps --schema path/to/schema.json --format uri
```

## Make Commands

```bash
make setup      # Fetch Cargo dependencies
make lint       # Run Trunk checks and workspace Clippy (pedantic)
make format     # Run rustfmt and Trunk formatters
make test       # Run workspace tests (unit + integration + conformance)
make build      # Build release artifacts for every member crate
make codeql     # Run local CodeQL analysis
make clean      # Remove build artefacts
```

## Compiler Invariants

- **Offline by default** — the resolver uses the local filesystem.  HTTP/HTTPS
  fetches require the `http-loader` feature flag and an explicit URI allowlist.
- **No `unsafe` in core** — `unsafe_code = "forbid"` applies to all crates
  except `schemaforge-python` and `schemaforge-node`, which are FFI boundaries
  with documented per-function justification.
- **Draft 2020-12 canonical** — all dialects are desugared to Draft 2020-12
  before the IR is constructed; back-end crates are dialect-agnostic.
- **Hybrid native + plan** — the AoT and Runtime Plan paths share the same IR
  and produce semantically equivalent validation behaviour.

## Documentation

- `docs/rfc/0001-schemaforge-hybrid-compiler.md` — architecture RFC
- `docs/adr/` — architecture decision records
- `docs/threat-model.md` — security threat model
- `docs/wasm-feasibility.md` — deferred WASM RFC stub (Phase 7)

## Quality Guardrails

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- GitHub CodeQL analysis for Rust
- Clippy cognitive complexity threshold capped at `10`
- Conformance tests run against the [JSON Schema Test Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite)
