# 3. FFI binding crates may opt out of `unsafe_code = "forbid"`

Date: 2026-07-14

## Status

Accepted

## Context

The workspace `[workspace.lints.rust]` table sets `unsafe_code = "forbid"` for
all member crates. PyO3 and napi-rs both require `unsafe` blocks internally to
implement the Rust ↔ host language boundary.

The binding crates (`schemaforge-python`, `schemaforge-node`) follow a **lazy
FFI views** design: the Rust side owns all values and the FFI layer only
provides thin, borrowed views into them.

## Decision

The `pyo3` and `napi` Cargo features in `schemaforge-python` and
`schemaforge-node` respectively enable the actual FFI glue.  When those
features are enabled the crates override `unsafe_code` at the crate level:

```toml
[lints.rust]
unsafe_code = "allow"
```

Without those features (the default), both crates compile as ordinary safe
Rust libraries. This allows `cargo test --workspace` to run without any
`unsafe` code.

The pure-Rust API (`CompiledSchema`, `JsCompiledSchema`, `validate`) is
implemented in `lib.rs` without unsafe, ensuring correctness can be verified
without FFI involvement.

## Consequences

- `cargo clippy --workspace --all-targets` (no feature flags) remains clean.
- Enabling `--features pyo3` or `--features napi` requires an explicit opt-in
  and the corresponding toolchain (Python headers / Node.js headers).
- The ADR must be updated if additional FFI crates are added.
- An FFI-specific CI job (behind a `ffi` matrix flag) is the recommended way
  to validate the unsafe code paths.
