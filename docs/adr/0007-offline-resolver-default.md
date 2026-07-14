# ADR 0007 — Offline Resolver as Default

| Field  | Value      |
| ------ | ---------- |
| Status | Accepted   |
| Date   | 2026-07-14 |

## Context

JSON Schema allows schemas to reference external documents via `$ref` URIs
pointing to arbitrary HTTP endpoints (e.g., `https://example.com/schemas/foo`).
Automatically fetching these URIs during compilation creates several problems:

- **SSRF risk** — a hostile schema can cause the compiler to make requests to
  internal infrastructure.
- **Reproducibility** — builds become non-reproducible when remote schemas
  change or become unavailable.
- **Hermeticity** — build systems (Bazel, Nix, sandboxed CI) that run without
  network access will fail silently or produce incorrect output if external
  resolution is assumed.
- **Audit trail** — developers cannot easily enumerate which external schemas
  were fetched during compilation.

## Decision

`schemaforge-resolver` uses a **filesystem loader as its default**. Network
fetches require opting in explicitly:

1. HTTP/HTTPS loading is available via the `http-loader` Cargo feature flag,
   disabled by default.
2. When the HTTP loader is enabled, callers must supply a non-empty allowlist
   of URI prefixes. The resolver rejects any URI not matching the allowlist
   with a structured diagnostic (not a panic or silent failure).
3. All resolution attempts (success and failure) are emitted as
   `Diagnostic::Info` entries so build systems can log or replay them.
4. The CLI (`schemaforge-cli`) exposes `--allow-remote <prefix>` flags; passing
   none means offline-only.

Schema documents that are expected to be fetched from the network must be
pre-downloaded and placed in the local resolution path (a directory or a
`--schema-root` archive) before invoking the compiler.

## Consequences

**Positive:**

- Eliminates SSRF as a default attack surface.
- Builds are reproducible; remote schema changes do not affect previously
  pinned local copies.
- Works in network-restricted environments (CI sandboxes, air-gapped
  deployments) without configuration changes.
- Audit log of resolved URIs is available for compliance purposes.

**Negative / trade-offs:**

- Developers who rely on automatic remote resolution (common with
  `jsonschema.net` or schemastore.org) must pre-download schemas, which is
  a small but real workflow change.
- The allowlist mechanism requires callers to know which URIs a schema
  transitively references; this can be determined with the
  `sfg deps --format uri` CLI subcommand before enabling remote loading.
