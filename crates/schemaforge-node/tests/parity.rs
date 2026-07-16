//! Cross-language parity tests.
//!
//! Reads `conformance/parity/fixtures.json` and asserts that
//! [`schemaforge_node::validate_json`] produces the expected validity for
//! every fixture.  The same fixture file is consumed by the Python and Node.js
//! test harnesses; see `conformance/parity/README.md` for details.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct Fixture {
    description: String,
    schema: Value,
    instance: Value,
    valid: bool,
}

fn fixtures_path() -> std::path::PathBuf {
    let candidates = [
        Path::new("conformance/parity/fixtures.json"),
        Path::new("../../conformance/parity/fixtures.json"),
    ];
    for c in &candidates {
        if c.exists() {
            return c.to_path_buf();
        }
    }
    panic!("conformance/parity/fixtures.json not found; run from the workspace root");
}

#[test]
fn parity_fixtures() {
    let raw =
        std::fs::read_to_string(fixtures_path()).expect("read conformance/parity/fixtures.json");
    let fixtures: Vec<Fixture> =
        serde_json::from_str(&raw).expect("parse conformance/parity/fixtures.json");

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for fx in &fixtures {
        let schema_str = fx.schema.to_string();
        let instance_str = fx.instance.to_string();

        let result = schemaforge_node::validate_json(&schema_str, &instance_str);
        let is_valid = result.is_ok();

        if is_valid == fx.valid {
            passed += 1;
        } else {
            failed += 1;
            let detail = if fx.valid {
                format!(
                    "FAIL [{}]: expected valid but got errors: {:?}",
                    fx.description,
                    result.unwrap_err()
                )
            } else {
                format!("FAIL [{}]: expected invalid but got Ok(())", fx.description)
            };
            failures.push(detail);
        }
    }

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("{f}");
        }
        panic!("{failed} parity fixture(s) failed, {passed} passed");
    }

    println!("All {passed} parity fixtures passed");
}
