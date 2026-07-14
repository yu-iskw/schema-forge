# SchemaForge Examples

Example JSON Schema documents for exploring the SchemaForge compiler.
Each schema is Draft 2020-12 and works with the `schemaforge` CLI.

## Schemas

| File                | Description                                      |
| ------------------- | ------------------------------------------------ |
| `basic-object.json` | Object with required fields, formats, and arrays |

## Running the examples

### Inspect and explain

```bash
schemaforge inspect examples/basic-object.json
schemaforge explain examples/basic-object.json
```

### Generate Rust types

```bash
schemaforge generate examples/basic-object.json --output /tmp/generated/
```

### Validate an instance

```bash
echo '{"id":"550e8400-e29b-41d4-a716-446655440000","email":"a@b.co","count":1,"tags":["x"]}' \
  | schemaforge validate examples/basic-object.json -
```

### Lock local resources

```bash
schemaforge lock examples/basic-object.json --output schemaforge.lock.toml
```
