# Release and Provenance

This document describes the release checklist, supply-chain controls, and
attestation requirements for SchemaForge. Every published artefact must trace
back to a tagged commit that passed the full CI suite.

---

## 1. Pre-release Checklist

### 1.1 Code and tests

- [ ] All changes are on a release branch (`release/vX.Y.Z`) and have been
      reviewed.
- [ ] `make lint && make test` passes locally and in CI (`test.yml`).
- [ ] No open severity-high or severity-critical advisories in
      `cargo audit --deny warnings`.
- [ ] Clippy is clean: `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] `cargo fmt --all --check` reports no diffs.
- [ ] CodeQL scan (`codeql.yml`) shows zero new findings.

### 1.2 Version bump

- [ ] Root `Cargo.toml` `[workspace.package]` `version` field updated.
- [ ] All member crate `Cargo.toml` files that inherit the version are
      consistent (they should use `version.workspace = true`).
- [ ] `Cargo.lock` is committed with the new versions.
- [ ] `packages/python/pyproject.toml` version updated.
- [ ] `packages/node/package.json` version updated.
- [ ] `CHANGELOG.md` entry added (if the changelog workflow is active).

### 1.3 Tagged commit

- [ ] Create and push the annotated tag from the release branch HEAD:
  ```bash
  git tag -a vX.Y.Z -m "Release vX.Y.Z"
  git push origin vX.Y.Z
  ```
- [ ] CI passes on the tag ref before any artefact is published.

---

## 2. Artefact Publication

### 2.1 Rust crates (crates.io)

Publish crates in dependency order so crates.io can resolve them:

```text
schemaforge-source
schemaforge-diagnostics
schemaforge-dialect
schemaforge-resolver
schemaforge-ir
schemaforge-analysis
schemaforge-formats
schemaforge-jsonschema
schemaforge-openapi
schemaforge-runtime
schemaforge-compiler
schemaforge-codegen-rust
schemaforge-cli
# FFI crates last (require feature-gate confirmation)
schemaforge-python
schemaforge-node
```

```bash
# Dry-run first
cargo publish -p schemaforge-source --dry-run
# Then publish for real
cargo publish -p schemaforge-source
# Repeat for each crate
```

### 2.2 Python wheels (PyPI)

Build and upload via [maturin](https://github.com/PyO3/maturin):

```bash
cd packages/python
maturin build --release --features pyo3-ffi
maturin upload --repository pypi dist/*.whl
```

The `schemaforge-release.yml` workflow automates this step via the
`publish-python` job (see `.github/workflows/schemaforge-release.yml`).

### 2.3 Node.js package (npm)

Build with napi-rs and publish:

```bash
cd packages/node
napi build --release --features napi-ffi
npm publish --access public
```

---

## 3. SBOM Generation

An SBOM (Software Bill of Materials) must be generated for every release.
See [`docs/sbom.md`](./sbom.md) for the full procedure.

The CI release workflow stores the SBOM as a build artefact and attaches it to
the GitHub Release.

---

## 4. Attestations

### 4.1 Compiler manifest digest

`schemaforge-runtime` exposes a `RUNTIME_PLAN` constant that describes the
current plan schema version. Before publishing, record the digest of the
compiled `sfg` binary:

```bash
sha256sum target/release/sfg
```

Store this digest in the GitHub Release notes and in the SBOM's component
metadata for the CLI binary.

### 4.2 SLSA provenance (recommended)

Use GitHub's [slsa-github-generator](https://github.com/slsa-framework/slsa-github-generator)
to produce SLSA level 3 provenance for release artefacts:

```yaml
# In the release workflow:
- uses: slsa-framework/slsa-github-generator/.github/workflows/builder_go_slsa3.yml@v1
```

Provenance files (`*.intoto.jsonl`) are uploaded alongside the artefacts and
verified by consumers using `slsa-verifier`.

### 4.3 Sigstore / cosign (optional)

Sign release artefacts with [cosign](https://github.com/sigstore/cosign):

```bash
cosign sign-blob --bundle sfg.bundle target/release/sfg
cosign verify-blob --bundle sfg.bundle target/release/sfg
```

---

## 5. Post-release Verification

- [ ] `cargo install schemaforge-cli` succeeds from crates.io.
- [ ] `pip install schemaforge` installs the correct wheel version.
- [ ] `npm install @schemaforge/core` resolves the expected package.
- [ ] The GitHub Release page shows the SBOM attachment and digest.
- [ ] The `sfg --version` output matches the release tag.

---

## 6. Rollback Procedure

If a critical defect is found after publication:

1. **Rust crates** — yank the bad version on crates.io:
   ```bash
   cargo yank --vers X.Y.Z -p schemaforge-cli
   ```
2. **Python** — yank via PyPI web UI or `twine` (PyPI does not support
   re-upload of the same version).
3. **Node** — deprecate via `npm deprecate @schemaforge/core@X.Y.Z "critical bug"`.
4. Tag a patch release (`vX.Y.Z+1`) with the fix and re-run the full checklist.

---

## 7. Shared Compiler Manifest Digests

The `RUNTIME_PLAN` constant in `schemaforge-runtime/src/lib.rs` encodes the
plan schema version. Every release must record:

| Release | `RUNTIME_PLAN` version | `sfg` binary SHA-256 |
| ------- | ---------------------- | -------------------- |
| v0.1.0  | 0.1                    | _(populated by CI)_  |

This table is updated automatically by the `schemaforge-release.yml` workflow
step `record-manifest-digest`.
