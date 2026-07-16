# schemaforge (Python)

Python bindings for the [Schemaforge](https://github.com/yu-iskw/schema-forge)
JSON Schema compiler.

## Features

- Validate JSON instances against JSON Schema Draft 2020-12
- Compile schemas once, validate many times
- Native Rust extension for maximum performance (optional)
- Pure-Python subprocess fallback when native extension is not installed

## Installation

### From a wheel (recommended)

```sh
pip install schemaforge
```

The wheel is built with [maturin](https://github.com/PyO3/maturin) and includes the
native Rust extension module.

### Development install (requires Rust + maturin)

```sh
# Install maturin
pip install maturin

# Build and install in editable mode (native extension enabled)
cd packages/python
maturin develop --features extension-module

# Or build without the native extension (CLI fallback only)
pip install -e .
```

> **Note on `extension-module` feature**
>
> The `extension-module` Cargo feature activates PyO3 FFI glue inside the
> `schemaforge-python` crate. PyO3 requires `unsafe` code at the ABI
> boundary. The workspace forbids `unsafe` globally, so the FFI code is
> gated behind this feature flag and the crate compiles as safe Rust without
> it. See [ADR-0003](../../docs/adr/0003-ffi-binding-unsafe-exception.md) for
> the full rationale.

### CLI fallback only

If you only need the subprocess fallback (no Rust needed), install the CLI
separately:

```sh
cargo install schemaforge-cli
pip install schemaforge
```

## Usage

```python
import schemaforge

# One-shot validation — returns a list of error strings (empty = valid)
errors = schemaforge.validate_json('{"type": "string"}', '"hello"')
assert errors == []

errors = schemaforge.validate_json('{"type": "string"}', '42')
assert len(errors) > 0   # invalid

# Compile once, validate many times
schema = schemaforge.compile_schema('{"type": "number", "minimum": 0}')

assert schema.validate_json("3.14") == []
assert len(schema.validate_json("-1")) > 0
```

## API

### `validate_json(schema_str, instance_str) -> list[str]`

Validate a JSON instance against a JSON Schema. Both arguments must be
valid JSON strings.

- Returns `[]` when the instance is **valid**.
- Returns a non-empty list of human-readable error messages when **invalid**.
- Raises `ValueError` when either argument is not valid JSON or the schema
  cannot be compiled.

### `compile_schema(schema_str) -> CompiledSchema`

Compile a schema for repeated validation. Raises `ValueError` on invalid input.

### `CompiledSchema.validate_json(instance_str) -> list[str]`

Same return semantics as `validate_json`.

## Cross-language parity

The `conformance/parity/fixtures.json` file contains a shared test corpus
used to verify consistent behaviour across the Rust, Python, and Node.js
bindings. See [conformance/parity/README.md](../../conformance/parity/README.md)
for details.

## License

Apache-2.0
