# {{ project_name }}

{{ project_description }}

Production-ready Rust workspace template built around Cargo, Trunk, and GitHub Actions.

## AI Assistants

- Codex should use [AGENTS.md](./AGENTS.md) for repo-specific instructions and verification expectations.
- Claude-specific workflow details remain in `CLAUDE.md` and `.claude/`.
- Shared reusable skills are authored in `.claude/skills` and exposed to other agents through the symlink mirror in `.agents/skills`.

## Workspace Layout

```text
.
├── Cargo.toml
├── crates/
│   ├── workspace-cli/
│   └── workspace-core/
├── dev/
└── .github/workflows/
```

- `crates/workspace-core`: example library crate for shared domain logic
- `crates/workspace-cli`: example binary crate depending on `workspace-core`
- `[workspace.dependencies]`: central place for shared dependency versions
- `[workspace.lints.clippy]`: workspace-wide Clippy policy and AI guardrails

## Quality Guardrails

- `cargo fmt --all --check` for formatting
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- GitHub CodeQL analysis for Rust projects via the `cpp` workflow configuration
- Clippy cognitive complexity threshold capped at `10`

## Development

```bash
make setup      # Fetch workspace dependencies
make lint       # Run Trunk checks and workspace clippy
make format     # Run rustfmt and Trunk formatters
make test       # Run workspace tests
make build      # Build release artifacts for every member
make codeql     # Run local CodeQL analysis
```

## Bootstrap Flow

Use `.claude/skills/initialize-project/SKILL.md` to rename the template safely:

- update the root workspace metadata
- rename `workspace-core` and `workspace-cli`
- update inter-crate dependency names and README placeholders
