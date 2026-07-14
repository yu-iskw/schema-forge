# SchemaForge Examples

This directory contains example JSON Schema documents that demonstrate
SchemaForge compilation.  Each schema is valid JSON Schema Draft 2020-12 and
can be used with the `sfg` CLI to explore the compiler outputs.

---

## Schemas

| File                  | Description                                              |
|-----------------------|----------------------------------------------------------|
| `basic-object.json`   | Simple object with required fields, formats, and arrays  |

---

## Running the Examples

### Ahead-of-time Rust code-generation

```bash
sfg compile --schema examples/basic-object.json --out /tmp/generated/
```

Inspect the generated Rust types:

```bash
cat /tmp/generated/basic_object.rs
```

### Runtime Plan (interpreted path)

```bash
sfg compile --schema examples/basic-object.json --plan /tmp/basic-object.plan
```

Inspect the plan in human-readable form:

```bash
sfg inspect /tmp/basic-object.plan
```

### Validate an instance

```bash
cat <<'EOF' | sfg validate --schema examples/basic-object.json --stdin
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Alice"
}
EOF
```

### Show transitive URI dependencies

```bash
sfg deps --schema examples/basic-object.json --format uri
```

---

## Adding New Examples

1. Place the schema file in this directory with a `.json` extension.
2. Verify it compiles cleanly: `sfg compile --schema examples/<file>.json`.
3. Add a row to the table above describing the schema.
4. If the schema exercises a specific feature (e.g. `$ref`, `anyOf`,
   OpenAPI dialect), name it accordingly so it serves as a reference.
