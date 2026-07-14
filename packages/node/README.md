# @schemaforge/node

Node.js bindings for the [Schemaforge](https://github.com/yu-iskw/rust-project-template)
JSON Schema compiler.

## Features

- Validate JSON instances against JSON Schema Draft 2020-12
- Compile schemas once, validate many times
- Native napi-rs extension for maximum performance (optional)
- Pure-JavaScript subprocess fallback when native extension is not installed

## Installation

### From npm (recommended)

```sh
npm install @schemaforge/node
```

The package includes a prebuilt native napi-rs extension for common platforms.

### Build from source (requires Rust + napi-rs CLI)

```sh
npm install
npm run build
```

This compiles the `schemaforge-node` Rust crate with the `napi` Cargo feature
enabled and places the resulting `.node` file alongside `index.js`.

> **Note on the `napi` feature**
>
> The `napi` Cargo feature activates napi-rs FFI glue inside the
> `schemaforge-node` crate.  napi-rs requires `unsafe` code at the ABI
> boundary.  The workspace forbids `unsafe` globally, so the FFI code is
> gated behind this feature flag and the crate compiles as safe Rust without
> it.  See [ADR-0003](../../docs/adr/0003-ffi-binding-unsafe-exception.md) for
> the full rationale.

### CLI fallback only

```sh
cargo install schemaforge-cli
npm install @schemaforge/node
```

When the native extension is absent, the package automatically falls back to
spawning `schemaforge validate` as a child process.

## Usage

```js
const { validateJson, compileSchema } = require('@schemaforge/node');

// One-shot validation
const errors = validateJson('{"type":"string"}', '"hello"');
console.log(errors); // []

const errors2 = validateJson('{"type":"string"}', '42');
console.log(errors2); // ['value is not of type string']

// Compile once, validate many times
const schema = compileSchema('{"type":"number","minimum":0}');
console.log(schema.validateJson('3.14')); // []
console.log(schema.validateJson('-1'));   // ['...']
```

```ts
// TypeScript
import { validateJson, compileSchema, CompiledSchema } from '@schemaforge/node';

const errors: string[] = validateJson('{"type":"string"}', '"hello"');
```

## API

### `validateJson(schemaStr, instanceStr): string[]`

Validate a JSON instance against a JSON Schema.  Both arguments must be valid
JSON strings.

- Returns `[]` when the instance is **valid**.
- Returns a non-empty array of error message strings when **invalid**.
- Throws `Error` when either argument is not valid JSON, the schema cannot be
  compiled, or the fallback CLI binary is not on `PATH`.

### `compileSchema(schemaStr): CompiledSchema`

Compile a schema for repeated validation.  Throws on invalid input.

### `CompiledSchema#validateJson(instanceStr): string[]`

Same return semantics as `validateJson`.

## Cross-language parity

The `conformance/parity/fixtures.json` file in the repository root contains a
shared test corpus used to verify consistent behaviour across the Rust, Python,
and Node.js bindings.  See
[conformance/parity/README.md](../../conformance/parity/README.md) for details.

## License

Apache-2.0
