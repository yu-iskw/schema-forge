//! Keywords whose values are sub-schemas (Draft 2020-12 structural positions).
//!
//! Construction-time and resolution walks must only descend into these
//! positions.  Non-schema annotations such as `default`, `const`, `enum`,
//! `examples`, `title`, and `description` are plain JSON values and must not
//! be recursed into.

use serde_json::{Map, Value};

/// Keywords whose value is a single sub-schema.
pub const SCHEMA_SINGLE_KEYWORDS: &[&str] = &[
    "additionalProperties",
    "contains",
    "contentSchema",
    "else",
    "if",
    "items",
    "not",
    "propertyNames",
    "then",
    "unevaluatedItems",
    "unevaluatedProperties",
];

/// Keywords whose value is an array of sub-schemas.
pub const SCHEMA_ARRAY_KEYWORDS: &[&str] = &["allOf", "anyOf", "oneOf", "prefixItems"];

/// Keywords whose value is an object mapping names to sub-schemas.
pub const SCHEMA_MAP_KEYWORDS: &[&str] = &[
    "$defs",
    "definitions",
    "dependentSchemas",
    "patternProperties",
    "properties",
];

/// Call `f` for each immediately reachable child schema of `obj`.
///
/// Only structural keywords are visited.  Non-schema annotations
/// (`default`, `const`, `enum`, `examples`, `title`, `description`, …)
/// are intentionally skipped.
///
/// Stops early and returns `Err(e)` when `f` returns `Err`.
pub fn for_each_schema_child<E, F>(obj: &Map<String, Value>, mut f: F) -> Result<(), E>
where
    F: FnMut(&Value) -> Result<(), E>,
{
    for &key in SCHEMA_SINGLE_KEYWORDS {
        if let Some(v) = obj.get(key) {
            f(v)?;
        }
    }
    for &key in SCHEMA_ARRAY_KEYWORDS {
        if let Some(Value::Array(arr)) = obj.get(key) {
            for item in arr {
                f(item)?;
            }
        }
    }
    for &key in SCHEMA_MAP_KEYWORDS {
        if let Some(Value::Object(map)) = obj.get(key) {
            for v in map.values() {
                f(v)?;
            }
        }
    }
    Ok(())
}
