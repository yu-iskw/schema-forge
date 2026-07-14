# SchemaForge Threat Model

| Field   | Value                    |
|---------|--------------------------|
| Version | 0.1                      |
| Date    | 2026-07-14               |
| Scope   | Compiler pipeline, CLI, language bindings |

---

## 1. Trust Boundaries

```
  Untrusted                     Trusted
  ─────────                     ───────
  Schema documents ──────────►  schemaforge-resolver (offline default)
  CLI arguments    ──────────►  schemaforge-cli (argument parsing)
  Plan files       ──────────►  schemaforge-runtime (deserialiser)
  Python/Node APIs ──────────►  schemaforge-python / schemaforge-node (FFI)
```

Schema documents and Runtime Plan files sourced from external locations are
considered **untrusted input**.  The compiler must not execute arbitrary code
on behalf of untrusted input under any configuration.

---

## 2. SSRF — Server-Side Request Forgery

### Threat

A hostile schema contains `$ref` URIs pointing to internal infrastructure
(e.g., `http://169.254.169.254/latest/meta-data/` in cloud environments).
If the resolver fetches URIs automatically, the compiler becomes an
unauthenticated proxy to internal services.

### Controls

- **Offline by default (primary control)** — `schemaforge-resolver` uses the
  filesystem loader unless the `http-loader` Cargo feature is explicitly
  enabled.  Most deployments never enable it.
- **URI allowlist (secondary control)** — when `http-loader` is enabled,
  callers supply a non-empty allowlist of URI prefixes.  Requests outside the
  allowlist are rejected with a structured diagnostic before any network
  connection is attempted.
- **Audit log** — all resolution attempts are emitted as `Diagnostic::Info`
  entries so operators can detect unexpected URIs in build logs.

### Residual risk

Applications that enable `http-loader` with an overly broad allowlist (e.g.,
`http://`) remain vulnerable.  Documentation explicitly warns against wildcard
allowlists.

---

## 3. ReDoS — Regular Expression Denial of Service

### Threat

The `format` keyword allows format names such as `email`, `uri`, and `date`.
If format validators use backtracking regular expressions, a hostile schema
value could cause catastrophic backtracking and hang the validator.

### Controls

- **Linear-time regex engine** — all regex-based format validators use the
  `regex` crate, which guarantees linear-time matching by construction.
- **No user-supplied patterns** — format validators are registered at startup
  from a fixed built-in set; schemas cannot inject arbitrary regex patterns.
- **Format assertion toggle** — `format` can be configured as annotation-only
  (no validation) when the format registry is not trusted.

### Residual risk

Custom format validators registered via the extension API may use third-party
regex libraries.  Callers are responsible for ensuring those libraries are
ReDoS-safe; this is documented in the extension API.

---

## 4. Codegen Expansion

### Threat

A deeply nested or combinatorially large schema (thousands of `allOf` branches,
exponentially nested `oneOf`) causes the code-generation back-end to produce
gigabytes of Rust source, exhausting disk space or compiler memory.

### Controls

- **Maximum output size** — `schemaforge-codegen-rust` enforces a configurable
  `max_output_lines` limit (default: 50,000).  Schemas that would exceed this
  limit produce a `Diagnostic::Error` instead of writing output.
- **Maximum recursion depth** — the IR-to-source lowering enforces a
  `max_nesting_depth` limit (default: 32).
- **Analysis pre-check** — `schemaforge-analysis` detects exponential
  `anyOf`/`oneOf` combinations and emits warnings before code generation
  begins.

### Residual risk

The default limits can be raised by callers.  Operators running SchemaForge
against untrusted schemas should keep the defaults or lower them.

---

## 5. Hostile Schemas

### 5.1 Circular `$ref` Chains

**Threat**: `$ref` cycles that are not detected cause infinite recursion or
stack overflow during resolution or code generation.

**Control**: `schemaforge-resolver` tracks visited URIs during a resolution
walk and returns a `Diagnostic::Error` on the first back-edge.  Cycles are
never silently followed.

### 5.2 Extremely Large `$defs`

**Threat**: A schema with tens of thousands of `$defs` entries exhausts memory
during IR construction.

**Control**: `schemaforge-compiler` enforces a configurable `max_node_count`
limit (default: 100,000 IR nodes).  Schemas that exceed this limit are
rejected before full IR construction completes.

### 5.3 `$anchor` Collision

**Threat**: Multiple schemas in the resolution set define the same `$anchor`,
causing silent aliasing.

**Control**: `schemaforge-resolver` treats `$anchor` collisions within the same
base URI as a `Diagnostic::Error`.  Cross-document anchor shadowing is treated
as a `Diagnostic::Warning`.

### 5.4 Malformed Plan Files

**Threat**: A tampered Runtime Plan file is deserialised and causes the
evaluator to behave incorrectly or panic.

**Control**: The plan deserialiser validates the version field first, then
performs structural validation before the evaluator accesses any field.  The
plan format contains no executable code; all instructions are data values with
a bounded type set.

---

## 6. FFI Boundary

`schemaforge-python` (PyO3) and `schemaforge-node` (napi-rs) are the only
crates that use `unsafe`.  Threat mitigations at this boundary:

- All `unsafe` blocks carry a `// SAFETY:` comment explaining the invariant
  maintained.
- Python `PyObject` and Node `JsValue` inputs are validated before conversion
  to Rust types; invalid inputs return a structured error, not a panic.
- The FFI crates link only to their respective runtimes; they do not expose
  arbitrary memory access to the host language.

---

## 7. Out of Scope

- Denial-of-service against the *generated* validators (not the compiler).
- Side-channel attacks on format validators.
- Supply chain attacks on Cargo dependencies (mitigated by `Cargo.lock` and
  GitHub Dependabot separately).
