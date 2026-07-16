//! Conformance tests for JSON Schema Draft 2020-12.
//!
//! Fixtures live under `conformance/json-schema-test-suite/` at the workspace
//! root and follow the official JSON Schema Test Suite shape:
//!
//! ```json
//! [{ "description": "…", "schema": {…}, "tests": [{ "description": "…", "data": …, "valid": true }] }]
//! ```
//!
//! Each `#[test]` function loads one fixture file, compiles every schema with
//! [`Validator`], and asserts that the reported validity matches `valid`.
//!
//! # Differential tests
//!
//! A second pass drives validation through the keyword-phase ordering defined
//! by [`schemaforge_runtime::RUNTIME_PLAN`]: the fixture schema is split into
//! per-phase sub-schemas, each sub-schema is validated independently, and the
//! AND of those results is compared with the result from the direct
//! [`Validator`].  Both paths must agree on every fixture data point.

use std::collections::HashSet;
use std::path::Path;

use schemaforge_jsonschema::{ValidationOptions, Validator, is_valid};
use schemaforge_runtime::{Phase, RUNTIME_PLAN};
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Fixture types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Suite {
    description: String,
    schema: Value,
    tests: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    description: String,
    data: Value,
    valid: bool,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load and parse a fixture JSON file from `conformance/json-schema-test-suite/`.
fn load_fixture(name: &str) -> Vec<Suite> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../conformance/json-schema-test-suite")
        .join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("malformed fixture {name}.json: {e}"))
}

/// Plan-driven validation.
///
/// Splits the top-level schema keywords into per-phase buckets according to
/// [`RUNTIME_PLAN`] and validates each bucket independently.  The instance is
/// considered valid iff every non-empty phase sub-schema validates it.
///
/// This exercises that the keyword-phase boundaries captured in the runtime
/// plan are consistent with the monolithic validator.  Because keywords that
/// interact (e.g. `properties` and `additionalProperties`) belong to the same
/// phase, the split does not break cross-keyword semantics.
///
/// # Non-object schemas
///
/// Boolean schemas (`true` / `false`) are forwarded to [`is_valid`] directly
/// because they carry no keyword structure to split.
fn plan_driven_is_valid(schema: &Value, data: &Value) -> bool {
    let Some(obj) = schema.as_object() else {
        return is_valid(schema, data);
    };

    let all_phases = [
        Phase::Core,
        Phase::Applicator,
        Phase::Properties,
        Phase::Validation,
        Phase::Unevaluated,
        Phase::Metadata,
    ];

    for phase in all_phases {
        let phase_kws: HashSet<&str> = RUNTIME_PLAN
            .keywords_in_phase(phase)
            .map(|e| e.name)
            .collect();

        let phase_schema: serde_json::Map<String, Value> = obj
            .iter()
            .filter(|(k, _)| phase_kws.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if phase_schema.is_empty() {
            continue;
        }

        if !is_valid(&Value::Object(phase_schema), data) {
            return false;
        }
    }

    true
}

/// Run a fixture file through both the direct and plan-driven validators and
/// assert that each test case produces the expected outcome and that both paths
/// agree.
fn run_conformance(fixture_name: &str) {
    let suites = load_fixture(fixture_name);

    for suite in &suites {
        let validator =
            Validator::new(&suite.schema, ValidationOptions::default()).unwrap_or_else(|e| {
                panic!(
                    "[{fixture_name}] could not compile schema for \"{}\": {e}",
                    suite.description
                )
            });

        for tc in &suite.tests {
            let direct = validator.validate(&tc.data).is_valid();
            let plan = plan_driven_is_valid(&suite.schema, &tc.data);

            assert_eq!(
                direct, tc.valid,
                "[{fixture_name}] suite=\"{}\" test=\"{}\" \
                 expected valid={} but direct validator returned {}",
                suite.description, tc.description, tc.valid, direct
            );

            assert_eq!(
                plan, tc.valid,
                "[{fixture_name}] suite=\"{}\" test=\"{}\" \
                 expected valid={} but plan-driven validator returned {} \
                 (direct validator agreed with expected)",
                suite.description, tc.description, tc.valid, plan
            );
        }
    }
}

// ---------------------------------------------------------------------------
// One test function per fixture file
// ---------------------------------------------------------------------------

#[test]
fn conformance_type() {
    run_conformance("type");
}

#[test]
fn conformance_properties() {
    run_conformance("properties");
}

#[test]
fn conformance_required() {
    run_conformance("required");
}

#[test]
fn conformance_items() {
    run_conformance("items");
}

#[test]
fn conformance_one_of() {
    run_conformance("oneOf");
}

#[test]
fn conformance_any_of() {
    run_conformance("anyOf");
}

#[test]
fn conformance_all_of() {
    run_conformance("allOf");
}

#[test]
fn conformance_enum() {
    run_conformance("enum");
}

#[test]
fn conformance_const() {
    run_conformance("const");
}

#[test]
fn conformance_additional_properties() {
    run_conformance("additionalProperties");
}

// ---------------------------------------------------------------------------
// Differential: verify runtime plan keyword coverage
// ---------------------------------------------------------------------------

/// Every keyword that appears in any fixture schema must be registered in
/// `RUNTIME_PLAN`.  This ensures our static plan is not silently ignoring
/// keywords used in conformance tests.
#[test]
fn runtime_plan_covers_all_fixture_keywords() {
    let fixture_names = [
        "type",
        "properties",
        "required",
        "items",
        "oneOf",
        "anyOf",
        "allOf",
        "enum",
        "const",
        "additionalProperties",
    ];

    let plan_kws: HashSet<&str> = RUNTIME_PLAN.keywords.iter().map(|e| e.name).collect();

    // Keywords that are intentionally outside the validation vocabulary
    // (they appear as schema meta-keywords or are always-valid annotations).
    let excluded: HashSet<&str> = ["$schema", "title", "description"]
        .iter()
        .copied()
        .collect();

    for name in fixture_names {
        let suites = load_fixture(name);
        for suite in &suites {
            collect_top_level_keywords(&suite.schema, &plan_kws, &excluded, name);
        }
    }
}

fn collect_top_level_keywords(
    schema: &Value,
    plan_kws: &HashSet<&str>,
    excluded: &HashSet<&str>,
    fixture_name: &str,
) {
    let Some(obj) = schema.as_object() else {
        return;
    };
    for key in obj.keys() {
        if excluded.contains(key.as_str()) {
            continue;
        }
        assert!(
            plan_kws.contains(key.as_str()),
            "[{fixture_name}] keyword \"{key}\" used in fixture schema \
             is not registered in RUNTIME_PLAN"
        );
    }
}
