//! Keywords whose values are sub-schemas (Draft 2020-12 structural positions).
//!
//! Construction-time and resolution walks must only descend into these
//! positions.  Non-schema annotations such as `default`, `const`, `enum`,
//! `examples`, `title`, and `description` are plain JSON values and must not
//! be recursed into.

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
