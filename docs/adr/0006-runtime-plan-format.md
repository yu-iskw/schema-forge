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
- Potentially executed inside a WASM sandbox.
- Cached and invalidated based on schema content (hash-stable).

The plan format must be versioned because breaking changes to the evaluator
will occur over the project lifetime.

## Decision

The Runtime Plan format is defined in `schemaforge-runtime` and has the
following properties:

1. **Encoding** — MessagePack for binary distributions; JSON is also supported
   as a human-readable alternative (feature flag `plan-json`).
2. **Versioning** — A `version` field at the top level uses a major.minor
   schema.  The evaluator rejects plans with a higher major version than it
   understands.
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
  can produce a plan that runs on Linux, macOS, Windows, and WASM.
- Hash-stable output enables build-system caching (Bazel, Buck2, Cargo build
  scripts).
- The data-only constraint simplifies security review; plan deserialisers do
  not need sandboxing.

**Negative / trade-offs:**

- MessagePack is not human-readable; developers debugging plan output must use
  the `plan-json` feature or the `sfg inspect` CLI subcommand.
- The DAG structure is more complex to emit and consume than a simple tree;
  the emitter must detect shared sub-schema references and emit them as
  labelled nodes.
- Adding new instruction kinds requires a minor version bump even if the change
  is additive; evaluators that do not know a `kind` must fail safely rather than
  silently skip.
