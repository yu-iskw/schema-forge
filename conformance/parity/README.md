# Cross-Language Parity Fixtures

`fixtures.json` defines a shared test corpus used to verify consistent
validation behaviour across all Schemaforge language bindings.

## Schema

Each fixture is a JSON object with four fields:

| Field         | Type    | Description                                           |
| ------------- | ------- | ----------------------------------------------------- |
| `description` | string  | Human-readable name for the test case                 |
| `schema`      | object  | A JSON Schema to validate against                     |
| `instance`    | any     | The JSON value to be validated                        |
| `valid`       | boolean | Expected validity (`true` = valid, `false` = invalid) |

## Consuming the fixtures

### Rust

The `schemaforge-python` and `schemaforge-node` crates each have an integration
test under `crates/<crate>/tests/parity.rs` that reads `fixtures.json` and
asserts the expected outcome. Run with:

```sh
cargo test --workspace parity
```

### Python

Load `fixtures.json` with `json.load`, then call `schemaforge.validate_json`
(or the CLI subprocess fallback) for each fixture:

```python
import json, pathlib, schemaforge

fixtures = json.loads(pathlib.Path("conformance/parity/fixtures.json").read_text())
for fx in fixtures:
    schema_str = json.dumps(fx["schema"])
    instance_str = json.dumps(fx["instance"])
    errors = schemaforge.validate_json(schema_str, instance_str)
    assert (len(errors) == 0) == fx["valid"], fx["description"]
```

### Node.js

```javascript
const { validateJson } = require("@schemaforge/node");
const fixtures = require("./conformance/parity/fixtures.json");

for (const fx of fixtures) {
  const errors = validateJson(
    JSON.stringify(fx.schema),
    JSON.stringify(fx.instance),
  );
  const isValid = errors.length === 0;
  console.assert(isValid === fx.valid, fx.description);
}
```

## Adding fixtures

Add a new entry to `fixtures.json`. Keep cases minimal and orthogonal —
one constraint per fixture unless specifically testing interaction between
keywords. Run `cargo test --workspace parity` to confirm the new fixture
passes on the Rust side before committing.
