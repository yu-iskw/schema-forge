# WASM Feasibility — Deferred RFC Stub (Phase 7)

| Field   | Value                    |
|---------|--------------------------|
| Status  | **Deferred**             |
| Phase   | 7                        |
| Date    | 2026-07-14               |

> **This document is a stub.**  Full investigation and RFC text will be written
> during Phase 7, after the Runtime Plan format and evaluator are stable
> (Phase 6 complete).  The sections below capture the open questions and
> constraints that must be resolved.

---

## 1. Motivation

The Runtime Plan evaluator (`schemaforge-runtime`) is designed to be
dependency-light and platform-portable.  Compiling it to WebAssembly would
enable:

- Embedding schema validation in browser applications without a server round-
  trip.
- Running the evaluator inside serverless runtimes (Cloudflare Workers,
  Fastly Compute) that accept WASM modules but not native binaries.
- Providing a single distributable artefact that works identically on Linux,
  macOS, Windows, and WASM hosts.

---

## 2. Constraints

### 2.1 No `std::net` or filesystem I/O in the evaluator

The evaluator itself must not perform network or filesystem operations.  The
offline-default resolver runs at compile time (plan generation), not at
validation time, so this constraint is already met by design.

### 2.2 No `unsafe` in WASM target

WebAssembly's linear memory model does not provide the same safety guarantees
as native Rust with the standard allocator.  The WASM build of the evaluator
must compile with `#![forbid(unsafe_code)]`, which is already the workspace
default.

### 2.3 Binary size

Full Rust standard library WASM binaries are often 1–5 MB before compression.
The evaluator must target `wasm32-unknown-unknown` (no `wasi`) or
`wasm32-wasi` depending on the host.  Size budget: < 500 KB gzip.

### 2.4 Plan format compatibility

A key open question is whether the WASM evaluator can consume the same plan
bytes as the native evaluator, or whether a separate plan layout is needed
(e.g., different endianness handling, no 64-bit atomics).

---

## 3. Open Questions

1. **Same plan bytes or separate WASM plan layout?**
   The native plan uses MessagePack.  MessagePack is byte-order neutral, so
   compatibility is likely, but needs verification with the `rmp-serde` WASM
   build.

2. **Allocator strategy for WASM** — use `wee_alloc`, the default `dlmalloc`,
   or a custom bump allocator?

3. **JavaScript interop API** — expose the evaluator as an ES module via
   `wasm-bindgen`, or as a raw WASM export consumed by a thin JS wrapper?

4. **Thread support** — `wasm32-unknown-unknown` does not support threads.
   Does the evaluator need to be async-safe or multi-threaded?

5. **Testing** — how are WASM build artefacts tested in CI?  Options include
   `wasmtime` (Rust native), `deno`, or `node` with `--experimental-wasm-*`
   flags.

---

## 4. Preliminary Dependency Audit

The following workspace dependencies need WASM-compatibility verification
before Phase 7 work begins:

| Dependency   | WASM status        | Notes                                      |
|--------------|--------------------|--------------------------------------------|
| `serde_json` | ✓ Known compatible | No OS dependencies                         |
| `rmp-serde`  | Likely compatible  | Needs `wasm32` CI job to confirm            |
| `regex`      | ✓ Known compatible | Uses `aho-corasick` which compiles to WASM |
| `fluent-uri` | Unknown            | Needs audit; may use `std::net`            |
| `indexmap`   | ✓ Known compatible | Pure data structure                        |
| `sha2`       | ✓ Known compatible | Pure computation                           |

---

## 5. Proposed Phase 7 Deliverables

When Phase 7 begins, this stub will be replaced by a full RFC covering:

1. WASM target selection (`wasm32-unknown-unknown` vs `wasm32-wasi`).
2. Binary size optimisation strategy.
3. JavaScript interop API design.
4. Plan format compatibility confirmation or migration guide.
5. CI integration for WASM build and test.
6. Security review of the WASM boundary (sandboxing, memory limits).

---

## 6. References

- RFC 0001, §8 Phase 7: `docs/rfc/0001-schemaforge-hybrid-compiler.md`
- ADR 0006 (Runtime Plan format): `docs/adr/0006-runtime-plan-format.md`
- [wasm-bindgen book](https://rustwasm.github.io/docs/wasm-bindgen/)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- [wasmtime](https://wasmtime.dev/)
