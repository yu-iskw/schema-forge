# ADR 0006 — Runtime Plan Format

| Field   | Value        |
|---------|--------------|
| Status  | Accepted     |
| Date    | 2026-07-14   |

## Context

The Runtime Plan path (see ADR 0004) requires a serialisable representation of
validation logic that can be:

- Embedded in Python wheels and Node.js packages without a Rust compiler in
  the deployment environment.
- Evaluated by a small, dependency-free interpreter.
- Potentially executed inside a WASM sandbox (deferred — see
  `docs/wasm-feasibility.md`).
- Cached and invalidated based on schema content (hash-stable).

The plan format must be versioned because breaking changes to the evaluator
will occur over the project lifetime.

An earlier iteration of this ADR proposed MessagePack as the primary binary
encoding.  That choice was **not implemented**; it is superseded by the
decision below.

## Decision

The Runtime Plan format is defined in `schemaforge-runtime` and has the
following properties:

1. **Encoding** — The plan is described as versioned Rust constants in
   `schemaforge-runtime/src/lib.rs` (the `RUNTIME_PLAN` constant and
   associated `PlanVersion` / `Instruction` types).  JSON is the only
   serialisation format currently generated; compact binary encoding (formerly
   proposed as MessagePack) is **deferred** until runtime performance
   benchmarks justify the additional dependency and complexity.
2. **Versioning** — A `version` field at the top level uses a `major.minor`
   schema defined by the `RUNTIME_PLAN` constant.  The evaluator rejects plans
   with a higher major version than it understands.  The authoritative version
   string lives in Rust source and is never duplicated in build scripts or
   configuration files.
3. **Determinism** — Identical IR input produces byte-identical plan output.
   Map keys are sorted; floating-point values are normalised.  This property
   enables content-addressed caching.
4. **Structure** — The plan is a sequence of typed instructions (discriminated
   by a `kind` tag) that form a DAG, not a tree, to share sub-schemas referenced
   by multiple `$ref` nodes.
5. **No executable code** — The plan contains data only; no native code
   pointers, no closures.  This makes it safe to deserialise from untrusted
   sources after verifying the version field.

## Consequences

**Positive:**

- Plan files are portable across platforms; one compile of `schemaforge-cli`
  can produce a plan that runs on Linux, macOS, Windows, and (when WASM lands)
  in a browser sandbox.
- Hash-stable output enables build-system caching (Bazel, Buck2, Cargo build
  scripts).
- The data-only constraint simplifies security review; plan deserialisers do
  not need sandboxing.
- Keeping the canonical version in Rust constants (rather than a separate
  schema file) makes version drift between the emitter and the evaluator a
  compile-time rather than a runtime error.
- Deferring MessagePack avoids adding a binary serialisation dependency until
  there is a measured need.

**Negative / trade-offs:**

- JSON plans are larger on disk than a compact binary format would be.  For
  the current schema corpus this is acceptable; revisit when benchmark data
  shows a bottleneck.
- The DAG structure is more complex to emit and consume than a simple tree;
  the emitter must detect shared sub-schema references and emit them as
  labelled nodes.
- Adding new instruction kinds requires a minor version bump even if the change
  is additive; evaluators that do not know a `kind` must fail safely rather
  than silently skip.

## Deferred items

- **Compact binary encoding** — MessagePack or a similar format may be adopted
  in a future ADR if profiling identifies JSON serialisation as a bottleneck in
  the Python or Node.js binding hot path.
- **WASM plan execution** — tracked in `docs/wasm-feasibility.md`; depends on
  the binary encoding decision above.
