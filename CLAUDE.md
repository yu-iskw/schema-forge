# Rust Workspace Template - Claude Code Memory

## Project Overview

This repository is a production-ready Rust workspace template.

Codex-specific project guidance lives in `AGENTS.md`. Keep Claude-only workflow details here and under `.claude/`.

- **Build System**: Cargo workspace
- **Linting/Formatting**: Clippy, rustfmt, and Trunk
- **Testing**: `cargo test --workspace --all-features`
- **Security**: GitHub CodeQL and Trunk security linters

## Quick Commands

```bash
make setup      # Fetch Cargo dependencies
make lint       # Run Trunk plus strict workspace clippy
make format     # Format Rust and repo files
make test       # Run workspace tests
make codeql     # Run local CodeQL analysis
make build      # Build release binaries and libraries
make clean      # Remove build artifacts
```

## Rust Guardrails

- Prefer shared versions in `[workspace.dependencies]` over duplicating dependency versions in member crates.
- Each crate must opt into workspace lints with:

```toml
[lints]
workspace = true
```

- Keep `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- Treat Clippy `pedantic`, `cargo`, and `cognitive_complexity` findings as mandatory fixes.
- Refactor functions before they become hard to read; the cognitive complexity threshold is `10`.
- Avoid `unsafe` unless there is a documented need and explicit review.

## Testing

- Add crate-local unit tests near the code they cover.
- Add integration tests under `crates/<crate-name>/tests/` when testing public behavior across modules.
- Run `make lint && make test` before committing.
- Use `cargo run -p workspace-cli` to verify the example binary path stays healthy.

## Architecture

- Root `Cargo.toml` defines the workspace and shared dependency versions.
- `crates/workspace-core` is the reusable library crate placeholder.
- `crates/workspace-cli` is the application crate placeholder.
- `dev/` holds helper scripts for local setup, lint, build, test, and CodeQL flows.
- `.claude/skills/initialize-project/SKILL.md` owns bootstrap-time renaming.

## Common Gotchas

- Do not duplicate dependency versions inside member crates when the dependency can live in `[workspace.dependencies]`.
- Keep `Cargo.lock` committed for this template because it includes an executable crate.
- Trunk manages non-Rust repo linters hermetically; do not replace it with ad hoc local installs.
- If a new member crate is added, update workspace membership and ensure it enables workspace lints.

## Git Workflow

- Create feature branches from `main`.
- Use conventional commit messages such as `feat(cli): add init command`.
- Run `make lint && make test` before commits.
- Record release notes with the `manage-changelog` skill when that workflow is in use.

## Available Skills

- `initialize-project`: rename the template and its workspace members
- `manage-adr`: maintain architecture decisions in `docs/adr`
- `manage-changelog`: maintain changelog fragments when enabled
- `.claude/skills` remains the canonical skill source even when other agents consume the mirrored tree under `.agents/skills`

## Self-Improvement

- Add or refine Claude rules here when recurring Rust-specific mistakes appear.
- Prefer reusable skills under `.claude/skills/` for workflows that should survive across projects.
