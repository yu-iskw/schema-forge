# Schemaforge Benchmarks

## Overview

Performance benchmarks for the `schemaforge-jsonschema` crate are implemented
using [Criterion.rs](https://github.com/bheisler/criterion.rs) and live
under `crates/schemaforge-jsonschema/benches/validation.rs`.

## Running the benchmarks

```bash
# Run all benchmarks and print a summary to stdout
cargo bench -p schemaforge-jsonschema

# Run only the compile benchmarks
cargo bench -p schemaforge-jsonschema -- compile

# Run only the validate benchmarks
cargo bench -p schemaforge-jsonschema -- validate

# Produce an HTML report (requires the `html_reports` feature, enabled by default)
# Reports are written to target/criterion/<benchmark-name>/report/index.html
cargo bench -p schemaforge-jsonschema
```

## Benchmark groups

| Group                    | Description                                                            |
| ------------------------ | ---------------------------------------------------------------------- |
| `compile/*`              | Time to call `Validator::new` (schema compilation only).               |
| `validate/valid/*`       | Time to validate a matching instance against a pre-compiled validator. |
| `validate/invalid/*`     | Time to validate a non-matching instance (error path).                 |
| `compile_and_validate/*` | End-to-end: compile schema then validate in one shot.                  |

## Schemas under test

| Name          | Keywords exercised                                       |
| ------------- | -------------------------------------------------------- |
| `simple_type` | `type`                                                   |
| `object`      | `type`, `required`, `properties`, `additionalProperties` |
| `allOf`       | `allOf`, `type`, `required`, `properties`                |
| `nested`      | `type`, `properties`, `required`, `items` (deep nesting) |

## Methodology

- **Tool**: Criterion 0.5 with `html_reports` feature enabled.
- **Sample size**: Criterion default (100 samples, warm-up 3 s).
- **Instances**: One valid and one invalid instance per schema to cover both the
  happy path and the early-exit error path.
- **What is measured**: Wall-clock time per iteration, including JSON value
  traversal but excluding JSON parsing (instances are pre-built `serde_json::Value`
  literals).
- **Compile benchmarks**: Measure only `Validator::new`, which clones the
  schema and builds format registries. Useful for tracking per-schema
  compilation overhead.
- **Outlier rejection**: Criterion's built-in statistical model is used; no
  manual outlier filtering.

## Adding new benchmarks

1. Add the schema and instance literals to `benches/validation.rs`.
2. Insert the case into the relevant `cases` slice.
3. If a new benchmark group is needed, define a `bench_*` function and register
   it in the `criterion_group!` macro call.
