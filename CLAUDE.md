# SchemaForge — Claude Code Memory

## Project Overview

SchemaForge is a hybrid JSON Schema compiler Rust workspace.  It compiles JSON
Schema documents (Draft 07, Draft 2019-09, Draft 2020-12, OpenAPI 3.x) into
either native Rust validation code (ahead-of-time path) or a compact Runtime
Plan (interpreted path).

Codex-specific project guidance lives in `AGENTS.md`. Keep Claude-only
workflow details here and under `.claude/`.

- **Build System**: Cargo workspace (resolver 3, edition 2024)
- **Linting/Formatting**: Clippy pedantic, rustfmt, and Trunk
- **Testing**: `cargo test --workspace --all-features` + conformance suite
- **Security**: GitHub CodeQL and Trunk security linters

## Quick Commands

```bash
make setup      # Fetch Cargo dependencies
make lint       # Run Trunk plus strict workspace Clippy
make format     # Format Rust and repo files
make test       # Run workspace tests
make codeql     # Run local CodeQL analysis
make build      # Build release binaries and libraries
make clean      # Remove build artefacts
```

## Crate Map

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

## Compiler Invariants

These invariants are non-negotiable; CI enforces them:

1. **Offline by default** — `schemaforge-resolver` uses the filesystem loader.
   HTTP/HTTPS requires the `http-loader` feature flag and a URI allowlist.
2. **No `unsafe` in core** — `unsafe_code = "forbid"` in workspace lints.
   Only `schemaforge-python` (PyO3) and `schemaforge-node` (napi-rs) may use
   `unsafe`, with per-function documented justification.
3. **Draft 2020-12 canonical** — `schemaforge-ir` and all downstream crates
   are dialect-agnostic.  Desugaring happens in `schemaforge-dialect`.
4. **Hybrid native + plan** — AoT and Runtime Plan paths share the same IR and
   must produce semantically equivalent validation behaviour.
5. **Deterministic plan output** — identical IR → byte-identical plan.

## Rust Guardrails

- Prefer shared versions in `[workspace.dependencies]` over duplicating
  dependency versions in member crates.
- Each crate must opt into workspace lints with:

```toml
[lints]
workspace = true
```

- Keep `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean.
- Treat Clippy `pedantic`, `cargo`, and `cognitive_complexity` findings as
  mandatory fixes.
- Refactor functions before they become hard to read; the cognitive complexity
  threshold is `10`.
- Avoid `unsafe` unless there is a documented need and explicit review.

## Testing

- Add crate-local unit tests near the code they cover.
- Add integration tests under `crates/<crate-name>/tests/` when testing public
  behaviour across modules.
- Run `make lint && make test` before committing.
- Conformance tests live under `conformance/` and run against the JSON Schema
  Test Suite.
- Use `cargo run -p schemaforge-cli` to verify the CLI entry point stays
  healthy.

## Architecture

See `docs/rfc/0001-schemaforge-hybrid-compiler.md` for the full architecture
RFC and `docs/adr/` for individual architecture decision records.

## Common Gotchas

- Do not duplicate dependency versions inside member crates when the dependency
  can live in `[workspace.dependencies]`.
- Keep `Cargo.lock` committed because this workspace includes an executable
  crate.
- Trunk manages non-Rust repo linters hermetically; do not replace it with
  ad hoc local installs.
- If a new member crate is added, update workspace membership and ensure it
  enables workspace lints.
- The `http-loader` feature is disabled by default; do not enable it in tests
  that should run offline.

## Git Workflow

- Create feature branches from `main`.
- Use conventional commit messages such as `feat(resolver): add URI allowlist`.
- Run `make lint && make test` before commits.
- Record release notes with the `manage-changelog` skill when that workflow is
  in use.

## Available Skills

- `initialize-project`: rename the template and its workspace members
- `manage-adr`: maintain architecture decisions in `docs/adr`
- `manage-changelog`: maintain changelog fragments when enabled
- `.claude/skills` remains the canonical skill source even when other agents
  consume the mirrored tree under `.agents/skills`

## Self-Improvement

- Add or refine Claude rules here when recurring Rust-specific mistakes appear.
- Prefer reusable skills under `.claude/skills/` for workflows that should
  survive across projects.
