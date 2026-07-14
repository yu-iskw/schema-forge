# Codex Project Guide

## Purpose

This repository includes Codex as a lightweight, repo-local collaborator for Rust workspace development. Use this file as the Codex-facing source of truth for project conventions, then follow the existing project scripts and checks instead of inventing parallel automation.

## Project Shape

- Root `Cargo.toml` defines the Cargo workspace, shared dependency versions, and workspace lint policy.
- `crates/workspace-core` is the reusable library crate placeholder.
- `crates/workspace-cli` is the example application crate depending on `workspace-core`.
- `dev/` contains the project scripts for setup, lint, format, test, build, clean, and local CodeQL analysis.
- `.trunk/trunk.yaml` defines repository-wide linting for Rust and non-Rust files.

## Required Verification

Use the project entrypoints that already exist:

```bash
make lint
make test
make build
```

Before finishing substantial code changes, run at least `make lint && make test`. Use `make build` when changes affect crate wiring, binary behavior, or release artifacts.

## Rust Guardrails

- Prefer shared versions in `[workspace.dependencies]` over duplicating versions in member crates.
- Keep crate lint opt-in enabled with:

```toml
[lints]
workspace = true
```

- Keep `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- Treat workspace Clippy `all`, `cargo`, and `pedantic` findings as mandatory fixes.
- The workspace forbids `unsafe` code and denies warnings in `[workspace.lints.rust]`.
- Refactor code before it becomes hard to read; the Clippy cognitive complexity threshold is `10`.

## Editing Expectations

- Update the root `Cargo.toml` first when adding shared dependencies or changing workspace-wide lint policy.
- Do not duplicate dependency versions inside `crates/workspace-core` or `crates/workspace-cli` when the dependency can live in `[workspace.dependencies]`.
- Keep `Cargo.lock` committed because this workspace includes an executable crate.
- If you add a new member crate, update workspace membership and ensure the crate enables workspace lints.
- Reuse `make` targets and `dev/` scripts instead of adding one-off verification commands to documentation.

## Claude Coexistence

- Existing files under `.claude/` are Claude Code specific.
- Do not assume Claude hooks, settings, plugins, or agent definitions apply to Codex.
- Keep Codex guidance in this file and keep Claude-specific operating details in `CLAUDE.md` and `.claude/`.
- Shared skill discovery for non-Claude agents lives under `.agents/skills`, which mirrors top-level directories from `.claude/skills` with symlinks.
- Treat `.claude/skills` as the canonical source of truth and edit skills there rather than under `.agents/skills`.
- Some mirrored skills still contain Claude- or Cursor-specific paths in their instructions; that portability cleanup is intentionally separate from the mirror itself.
