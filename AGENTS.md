# Codex Project Guide — SchemaForge

## Purpose

This repository is the SchemaForge Rust workspace: a hybrid JSON Schema
compiler that emits either native Rust validation code (ahead-of-time path)
or a compact Runtime Plan (interpreted path). Use this file as the
Codex-facing source of truth for project conventions.

## Project Shape

- Root `Cargo.toml` defines the Cargo workspace, shared dependency versions,
  and workspace lint policy.
- `crates/` contains fifteen member crates (see table below).
- `dev/` contains the project scripts for setup, lint, format, test, build,
  clean, and local CodeQL analysis.
- `.trunk/trunk.yaml` defines repository-wide linting for Rust and non-Rust
  files.

## Crate Overview

| Crate                      | Role                                                    |
| -------------------------- | ------------------------------------------------------- |
| `schemaforge-source`       | Byte loading, UTF-8 validation, span tracking           |
| `schemaforge-diagnostics`  | Structured error/warning type with file + span          |
| `schemaforge-dialect`      | Dialect detection and desugaring adapters               |
| `schemaforge-resolver`     | `$ref` resolution; offline by default, HTTP opt-in      |
| `schemaforge-ir`           | Canonical Semantic IR (Draft 2020-12 node types)        |
| `schemaforge-analysis`     | Reachability, cycle detection, unused anchor analysis   |
| `schemaforge-formats`      | `format` keyword registry and built-in validators       |
| `schemaforge-jsonschema`   | High-level JSON Schema API surface                      |
| `schemaforge-openapi`      | OpenAPI schema parsing and dialect bridge               |
| `schemaforge-compiler`     | Orchestration: ties all crates into one pipeline        |
| `schemaforge-codegen-rust` | IR → Rust source text (no proc-macros)                  |
| `schemaforge-runtime`      | Runtime Plan format, emitter, and evaluator             |
| `schemaforge-python`       | PyO3 bindings (only crate allowing `unsafe` for FFI)    |
| `schemaforge-node`         | napi-rs bindings (only crate allowing `unsafe` for FFI) |
| `schemaforge-cli`          | `sfg` binary entry point                                |

## Compiler Invariants

These invariants are enforced by CI and must not be violated:

1. **Offline by default** — `schemaforge-resolver` uses the filesystem loader
   unless `http-loader` feature is explicitly enabled with a URI allowlist.
2. **No `unsafe` in core** — `unsafe_code = "forbid"` applies to every crate
   except `schemaforge-python` and `schemaforge-node` (FFI boundaries).
3. **Draft 2020-12 canonical** — all dialects are desugared before the IR is
   constructed; `schemaforge-ir` and all downstream crates are dialect-agnostic.
4. **Hybrid native + plan** — the AoT and Runtime Plan paths share the same IR
   and must produce semantically equivalent validation behaviour (verified by
   shared conformance tests).
5. **Deterministic plan output** — identical IR input must produce byte-identical
   plan output (map keys sorted, floats normalised).

## Required Verification

Use the project entrypoints that already exist:

```bash
make lint
make test
make build
```

Before finishing substantial code changes, run at least `make lint && make test`.
Use `make build` when changes affect crate wiring, binary behaviour, or release
artefacts.

## Rust Guardrails

- Prefer shared versions in `[workspace.dependencies]` over duplicating versions
  in member crates.
- Keep crate lint opt-in enabled with:

```toml
[lints]
workspace = true
```

- Keep `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  clean.
- Treat workspace Clippy `all`, `cargo`, and `pedantic` findings as mandatory
  fixes.
- The workspace forbids `unsafe` code (except FFI crates) and denies warnings
  in `[workspace.lints.rust]`.
- Refactor code before it becomes hard to read; the Clippy cognitive complexity
  threshold is `10`.

## Editing Expectations

- Update the root `Cargo.toml` first when adding shared dependencies or
  changing workspace-wide lint policy.
- Do not duplicate dependency versions inside member crates when the dependency
  can live in `[workspace.dependencies]`.
- Keep `Cargo.lock` committed because this workspace includes an executable
  crate.
- If you add a new member crate, update workspace membership and ensure the
  crate enables workspace lints.
- Reuse `make` targets and `dev/` scripts instead of adding one-off
  verification commands to documentation.

## Claude Coexistence

- Existing files under `.claude/` are Claude Code specific.
- Do not assume Claude hooks, settings, plugins, or agent definitions apply to
  Codex.
- Keep Codex guidance in this file and keep Claude-specific operating details
  in `CLAUDE.md` and `.claude/`.
- Shared skill discovery for non-Claude agents lives under `.agents/skills`,
  which mirrors top-level directories from `.claude/skills` with symlinks.
- Treat `.claude/skills` as the canonical source of truth and edit skills there
  rather than under `.agents/skills`.
