//! Keyword phase catalog for the Schemaforge runtime.
//!
//! The [`RuntimePlan`] encodes keyword-processing order and phase boundaries
//! as Rust `const` data.  This catalog is used by conformance differential
//! tests and inspection tooling; a full plan evaluator is deferred.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

    /// Return the names of all keywords that belong to `phase`, in order.
    #[must_use]
    pub fn keyword_names_in_phase(self, phase: Phase) -> Vec<&'static str> {
        self.keywords_in_phase(phase).map(|k| k.name).collect()
    }

    /// Return whether a keyword is registered as an applicator.
    #[must_use]
    pub fn is_applicator(self, name: &str) -> bool {
        self.keywords.iter().any(|k| k.name == name && k.applicator)
    }

    /// Return whether a keyword is registered at all.
    #[must_use]
    pub fn contains_keyword(self, name: &str) -> bool {
        self.keywords.iter().any(|k| k.name == name)
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
            name: "dependentSchemas",
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
    use std::collections::HashSet;

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

    #[test]
    fn core_keywords_before_applicators() {
        let core = RUNTIME_PLAN.keyword_names_in_phase(Phase::Core);
        let applicators = RUNTIME_PLAN.keyword_names_in_phase(Phase::Applicator);
        let first_app = RUNTIME_PLAN
            .keywords
            .iter()
            .position(|k| applicators.contains(&k.name))
            .unwrap();
        for name in core {
            let idx = RUNTIME_PLAN
                .keywords
                .iter()
                .position(|k| k.name == name)
                .unwrap();
            assert!(idx < first_app, "{name} must precede applicators");
        }
    }

    #[test]
    fn unevaluated_after_validation() {
        let val_names = RUNTIME_PLAN.keyword_names_in_phase(Phase::Validation);
        let uneval_names = RUNTIME_PLAN.keyword_names_in_phase(Phase::Unevaluated);
        let last_val = RUNTIME_PLAN
            .keywords
            .iter()
            .rposition(|k| val_names.contains(&k.name))
            .unwrap();
        for name in uneval_names {
            let idx = RUNTIME_PLAN
                .keywords
                .iter()
                .position(|k| k.name == name)
                .unwrap();
            assert!(idx > last_val, "{name} must follow all validation keywords");
        }
    }

    #[test]
    fn properties_are_applicators() {
        assert!(RUNTIME_PLAN.is_applicator("properties"));
        assert!(RUNTIME_PLAN.is_applicator("additionalProperties"));
        assert!(RUNTIME_PLAN.is_applicator("allOf"));
        assert!(RUNTIME_PLAN.is_applicator("$ref"));
    }

    #[test]
    fn validation_keywords_not_applicators() {
        assert!(!RUNTIME_PLAN.is_applicator("type"));
        assert!(!RUNTIME_PLAN.is_applicator("minimum"));
        assert!(!RUNTIME_PLAN.is_applicator("required"));
    }

    #[test]
    fn all_draft2020_keywords_registered() {
        let required = [
            "$schema",
            "$ref",
            "$dynamicRef",
            "$dynamicAnchor",
            "allOf",
            "anyOf",
            "oneOf",
            "not",
            "if",
            "properties",
            "patternProperties",
            "additionalProperties",
            "propertyNames",
            "dependentSchemas",
            "prefixItems",
            "items",
            "contains",
            "unevaluatedProperties",
            "unevaluatedItems",
            "type",
            "enum",
            "const",
            "minimum",
            "maximum",
            "required",
            "dependentRequired",
            "format",
        ];
        for kw in required {
            assert!(RUNTIME_PLAN.contains_keyword(kw), "missing keyword: {kw}");
        }
    }

    #[test]
    fn executor_matches_validator_phase_semantics() {
        let expected_order = [
            Phase::Core,
            Phase::Applicator,
            Phase::Properties,
            Phase::Validation,
            Phase::Unevaluated,
            Phase::Metadata,
        ];
        let mut seen_phases: Vec<Phase> = RUNTIME_PLAN
            .keywords
            .iter()
            .map(|k| k.phase)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        seen_phases.sort();
        let mut expected_sorted = expected_order.to_vec();
        expected_sorted.sort();
        assert_eq!(seen_phases, expected_sorted);
    }
}
