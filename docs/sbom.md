# Software Bill of Materials (SBOM)

SchemaForge generates a CycloneDX SBOM for every release.  The SBOM lists all
direct and transitive Rust dependencies (resolved from `Cargo.lock`), Python
dependencies, and Node.js dependencies.

---

## 1. Tooling

| Ecosystem | Tool                  | Output format      |
|-----------|-----------------------|--------------------|
| Rust      | `cargo-cyclonedx`     | CycloneDX 1.4 JSON |
| Python    | `cyclonedx-bom`       | CycloneDX 1.4 JSON |
| Node.js   | `@cyclonedx/cyclonedx-npm` | CycloneDX 1.4 JSON |

Install the Rust tool:

```bash
cargo install cargo-cyclonedx
```

---

## 2. Generating the SBOM

### 2.1 Rust workspace

Run from the repository root:

```bash
cargo cyclonedx --format json --output-file sbom-rust.cdx.json
```

This inspects `Cargo.lock` and produces a CycloneDX JSON document covering all
workspace members.

### 2.2 Python package

```bash
cd packages/python
pip install cyclonedx-bom
cyclonedx-bom --format json --output sbom-python.cdx.json
```

### 2.3 Node.js package

```bash
cd packages/node
npx @cyclonedx/cyclonedx-npm --output-format JSON --output-file sbom-node.cdx.json
```

### 2.4 Merge into a single SBOM (optional)

Use [cdxgen](https://github.com/CycloneDX/cdxgen) to produce a merged workspace
SBOM:

```bash
cdxgen -t rust -o sbom-merged.cdx.json
```

---

## 3. CI Integration

The `schemaforge-release.yml` workflow (`.github/workflows/schemaforge-release.yml`)
runs SBOM generation as part of every tagged release:

```
generate-sbom job
  ├── cargo cyclonedx (Rust)
  ├── cyclonedx-bom  (Python)
  ├── cyclonedx-npm  (Node)
  └── Upload artefacts to GitHub Release
```

SBOMs are stored under `Artifacts` on the GitHub Actions run and attached to
the GitHub Release as `sbom-rust.cdx.json`, `sbom-python.cdx.json`, and
`sbom-node.cdx.json`.

---

## 4. Verification

Consumers can verify the SBOM against the published artefacts using
[CycloneDX CLI](https://github.com/CycloneDX/cyclonedx-cli):

```bash
cyclonedx validate --input-file sbom-rust.cdx.json --input-format json
```

---

## 5. Known Gaps

- The SBOM does not yet include the vendored JSON Schema Test Suite fixtures
  under `conformance/`.  These are Apache-2.0 licensed and will be added in a
  future release.
- The merged cross-ecosystem SBOM step is manual; full automation is planned
  once the CI matrix stabilises.
