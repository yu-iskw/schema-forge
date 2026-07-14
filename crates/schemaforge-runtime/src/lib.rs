//! Compile-time runtime plan constants for Schemaforge.
//!
//! The [`RuntimePlan`] encodes the keyword-processing order and phase
//! boundaries as Rust `const` data, eliminating runtime planning overhead.

/// A single keyword entry in the runtime plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeywordEntry {
    /// The keyword name as it appears in JSON Schema.
    pub name: &'static str,
    /// Which compilation phase handles this keyword.
    pub phase: Phase,
    /// Whether this keyword can affect other keywords' results.
    pub applicator: bool,
}

/// Compilation phase for a keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// Core keywords processed first: `$schema`, `$id`, `$vocabulary`, `$defs`.
    Core = 0,
    /// Applicator keywords: `allOf`, `anyOf`, `oneOf`, `not`, `if/then/else`.
    Applicator = 1,
    /// Property keywords: `properties`, `additionalProperties`, etc.
    Properties = 2,
    /// Validation keywords: `type`, `enum`, `const`, bounds, lengths.
    Validation = 3,
    /// Unevaluated keywords (post-applicator): `unevaluatedProperties`.
    Unevaluated = 4,
    /// Metadata keywords: `title`, `description`, `default`, `examples`.
    Metadata = 5,
}

/// The static runtime plan describing keyword processing order.
#[derive(Debug, Clone, Copy)]
pub struct RuntimePlan {
    /// Ordered list of keyword entries.
    pub keywords: &'static [KeywordEntry],
}

impl RuntimePlan {
    /// Iterate over keywords in a given phase.
    pub fn keywords_in_phase(self, phase: Phase) -> impl Iterator<Item = &'static KeywordEntry> {
        self.keywords.iter().filter(move |k| k.phase == phase)
    }
}

/// Canonical runtime plan for JSON Schema Draft 2020-12.
pub const RUNTIME_PLAN: RuntimePlan = RuntimePlan {
    keywords: &[
        // Phase::Core
        KeywordEntry {
            name: "$schema",
            phase: Phase::Core,
            applicator: false,
        },
        KeywordEntry {
            name: "$id",
            phase: Phase::Core,
            applicator: false,
        },
        KeywordEntry {
            name: "$anchor",
            phase: Phase::Core,
            applicator: false,
        },
        KeywordEntry {
            name: "$dynamicAnchor",
            phase: Phase::Core,
            applicator: false,
        },
        KeywordEntry {
            name: "$vocabulary",
            phase: Phase::Core,
            applicator: false,
        },
        KeywordEntry {
            name: "$defs",
            phase: Phase::Core,
            applicator: false,
        },
        KeywordEntry {
            name: "$comment",
            phase: Phase::Core,
            applicator: false,
        },
        // Phase::Applicator
        KeywordEntry {
            name: "$ref",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "$dynamicRef",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "allOf",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "anyOf",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "oneOf",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "not",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "if",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "then",
            phase: Phase::Applicator,
            applicator: true,
        },
        KeywordEntry {
            name: "else",
            phase: Phase::Applicator,
            applicator: true,
        },
        // Phase::Properties
        KeywordEntry {
            name: "properties",
            phase: Phase::Properties,
            applicator: true,
        },
        KeywordEntry {
            name: "patternProperties",
            phase: Phase::Properties,
            applicator: true,
        },
        KeywordEntry {
            name: "additionalProperties",
            phase: Phase::Properties,
            applicator: true,
        },
        KeywordEntry {
            name: "propertyNames",
            phase: Phase::Properties,
            applicator: true,
        },
        KeywordEntry {
            name: "items",
            phase: Phase::Properties,
            applicator: true,
        },
        KeywordEntry {
            name: "prefixItems",
            phase: Phase::Properties,
            applicator: true,
        },
        KeywordEntry {
            name: "contains",
            phase: Phase::Properties,
            applicator: true,
        },
        // Phase::Validation
        KeywordEntry {
            name: "type",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "enum",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "const",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "multipleOf",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "maximum",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "exclusiveMaximum",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "minimum",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "exclusiveMinimum",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "maxLength",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "minLength",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "pattern",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "maxItems",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "minItems",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "uniqueItems",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "maxContains",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "minContains",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "maxProperties",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "minProperties",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "required",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "dependentRequired",
            phase: Phase::Validation,
            applicator: false,
        },
        KeywordEntry {
            name: "format",
            phase: Phase::Validation,
            applicator: false,
        },
        // Phase::Unevaluated
        KeywordEntry {
            name: "unevaluatedProperties",
            phase: Phase::Unevaluated,
            applicator: true,
        },
        KeywordEntry {
            name: "unevaluatedItems",
            phase: Phase::Unevaluated,
            applicator: true,
        },
        // Phase::Metadata
        KeywordEntry {
            name: "title",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "description",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "default",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "examples",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "deprecated",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "readOnly",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "writeOnly",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "contentEncoding",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "contentMediaType",
            phase: Phase::Metadata,
            applicator: false,
        },
        KeywordEntry {
            name: "contentSchema",
            phase: Phase::Metadata,
            applicator: false,
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_plan_has_keywords() {
        assert!(!RUNTIME_PLAN.keywords.is_empty());
    }

    #[test]
    fn core_phase_first() {
        let first = RUNTIME_PLAN.keywords.first().unwrap();
        assert_eq!(first.phase, Phase::Core);
    }

    #[test]
    fn phases_in_order() {
        let mut last_phase = Phase::Core;
        for kw in RUNTIME_PLAN.keywords {
            assert!(kw.phase >= last_phase);
            last_phase = kw.phase;
        }
    }

    #[test]
    fn keywords_in_phase_filter() {
        let core: Vec<_> = RUNTIME_PLAN.keywords_in_phase(Phase::Core).collect();
        assert!(!core.is_empty());
        assert!(core.iter().all(|k| k.phase == Phase::Core));
    }

    #[test]
    fn applicator_flag_set() {
        assert!(RUNTIME_PLAN.keywords.iter().any(|k| k.applicator));
    }
}
