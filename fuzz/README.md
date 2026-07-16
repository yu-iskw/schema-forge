# Schemaforge Fuzz Testing

## Overview

This directory describes the fuzz-testing strategy for Schemaforge. Fuzz
targets exercise the JSON Schema parser and validator with arbitrary,
machine-generated inputs to surface panics, assertion failures, and logic
errors that hand-written tests may not reach.

## Targets

| Target                          | Crate                    | What it tests                                                                                                      |
| ------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `fuzz_json_schema_parser`       | `schemaforge-jsonschema` | Feed arbitrary bytes as a JSON Schema; assert no panic.                                                            |
| `fuzz_validate_random_instance` | `schemaforge-jsonschema` | Feed arbitrary (schema, instance) byte pairs; assert no panic and that `Validator::new` + `validate` never panics. |

## Running with cargo-fuzz

[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) is the recommended
driver. It requires a nightly toolchain and LLVM's libFuzzer.

```bash
# Install cargo-fuzz (nightly required)
rustup override set nightly
cargo install cargo-fuzz

# Run the parser target (Ctrl-C to stop)
cargo fuzz run fuzz_json_schema_parser

# Run with a corpus directory
cargo fuzz run fuzz_json_schema_parser fuzz/corpus/fuzz_json_schema_parser/

# Check coverage (requires cargo-fuzz ≥ 0.12)
cargo fuzz coverage fuzz_json_schema_parser
```

## cargo-fuzz skeleton

The skeleton targets are listed below. To activate them, install `cargo-fuzz`
and initialise the fuzz workspace:

```bash
cargo fuzz init          # creates fuzz/Cargo.toml and fuzz/fuzz_targets/
cargo fuzz add fuzz_json_schema_parser
```

Replace the generated `fuzz_targets/fuzz_json_schema_parser.rs` with the
following:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use schemaforge_jsonschema::{ValidationOptions, Validator};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };
    // Attempt to compile `s` as a JSON Schema.
    // The call must never panic — errors are expected and acceptable.
    if let Ok(validator) = schemaforge_jsonschema::from_str(s) {
        // Validate a fixed, non-panicking instance.
        let _ = validator.validate(&serde_json::json!({}));
    }
    // Also attempt to parse `s` as an instance and validate against a fixed schema.
    if let Ok(instance) = serde_json::from_str::<serde_json::Value>(s) {
        let schema = serde_json::json!({"type": "object"});
        let v = Validator::new(&schema, ValidationOptions::default()).unwrap();
        let _ = v.validate(&instance);
    }
});
```

## Property-based tests

While `cargo-fuzz` is not available in all CI environments, the crate ships
property-style unit tests under
`crates/schemaforge-jsonschema/src/lib.rs` (see the `prop_` test prefix).
These tests exercise a representative but deterministic set of "random-ish"
inputs — invalid UTF-8 sequences, deeply nested arrays, large integers, and
mixed-type collections — and assert that the validator never panics.

## CI integration

The property-style unit tests run as part of `cargo test --workspace` and
therefore as part of `make test`.

Full libFuzzer-based fuzzing is **not** wired into the default CI pipeline
because it requires:

1. A nightly Rust toolchain.
2. `cargo-fuzz` to be installed.
3. Extended (hours-long) run time to be meaningful.

To run fuzz targets in CI, add a scheduled workflow that installs the nightly
toolchain and `cargo-fuzz`, then runs each target for a fixed duration, e.g.:

```yaml
- name: Fuzz (15 minutes)
  run: cargo fuzz run fuzz_json_schema_parser -- -max_total_time=900
```

## Corpus management

Seed corpus files should be placed in
`fuzz/corpus/<target-name>/` as raw JSON bytes. Minimised crash inputs should
be committed to `fuzz/artifacts/<target-name>/`.

Neither directory is pre-populated in this repository because no crashes have
been found yet.
