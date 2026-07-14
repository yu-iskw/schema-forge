//! Format validators for the JSON Schema `format` keyword.
//!
//! The default policy is **annotation** (formats are recorded but not
//! asserted). Use [`FormatRegistry::assert_all`] to enable assertion mode for
//! specific or all formats.

use std::collections::HashMap;

use regex::Regex;
use thiserror::Error;

/// Error returned when a format validator cannot be constructed.
#[derive(Debug, Error)]
pub enum FormatError {
    /// The format name is not registered.
    #[error("unknown format: {0}")]
    UnknownFormat(String),
    /// The value does not satisfy the format.
    #[error("value does not match format `{format}`: {reason}")]
    Invalid {
        /// The format name.
        format: String,
        /// Why the value failed.
        reason: String,
    },
}

/// Validation result for a single format check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatResult {
    /// The format assertion passed.
    Valid,
    /// The format assertion failed with a reason.
    Invalid(String),
    /// The format is in annotation-only mode; no assertion was made.
    Annotation,
}

impl FormatResult {
    /// Returns `true` when the result is not a failure.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        !matches!(self, Self::Invalid(_))
    }
}

/// A format validator function.
type ValidatorFn = fn(&str) -> FormatResult;

/// Registry of named format validators.
pub struct FormatRegistry {
    validators: HashMap<String, ValidatorFn>,
    asserted: HashMap<String, bool>,
}

impl FormatRegistry {
    /// Create a registry populated with the standard JSON Schema formats.
    #[must_use]
    pub fn with_defaults() -> Self {
        let mut reg = Self {
            validators: HashMap::new(),
            asserted: HashMap::new(),
        };
        reg.register_defaults();
        reg
    }

    /// Register a custom format validator function.
    pub fn register(&mut self, name: impl Into<String>, f: ValidatorFn) {
        self.validators.insert(name.into(), f);
    }

    /// Enable assertion mode for the named format.
    pub fn assert_format(&mut self, name: impl Into<String>) {
        self.asserted.insert(name.into(), true);
    }

    /// Enable assertion mode for all registered formats.
    pub fn assert_all(&mut self) {
        let names: Vec<_> = self.validators.keys().cloned().collect();
        for name in names {
            self.asserted.insert(name, true);
        }
    }

    /// Validate `value` against the named `format`.
    ///
    /// Returns [`FormatResult::Annotation`] when the format is registered but
    /// assertion mode is disabled (the default).
    #[must_use]
    pub fn validate(&self, format: &str, value: &str) -> FormatResult {
        let Some(f) = self.validators.get(format) else {
            return FormatResult::Annotation;
        };
        if *self.asserted.get(format).unwrap_or(&false) {
            f(value)
        } else {
            FormatResult::Annotation
        }
    }

    fn register_defaults(&mut self) {
        self.register("date-time", validate_datetime);
        self.register("date", validate_date);
        self.register("time", validate_time);
        self.register("duration", validate_duration);
        self.register("email", validate_email);
        self.register("idn-email", validate_email);
        self.register("hostname", validate_hostname);
        self.register("idn-hostname", validate_hostname);
        self.register("ipv4", validate_ipv4);
        self.register("ipv6", validate_ipv6);
        self.register("uri", validate_uri);
        self.register("uri-reference", validate_uri_reference);
        self.register("iri", validate_uri);
        self.register("iri-reference", validate_uri_reference);
        self.register("uuid", validate_uuid);
        self.register("json-pointer", validate_json_pointer);
        self.register("relative-json-pointer", validate_relative_json_pointer);
        self.register("regex", validate_regex_format);
    }
}

impl Default for FormatRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ── Individual format validators ──────────────────────────────────────────────

fn validate_date(s: &str) -> FormatResult {
    if date_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid RFC 3339 date"))
    }
}

fn validate_time(s: &str) -> FormatResult {
    if time_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid RFC 3339 time"))
    }
}

fn validate_datetime(s: &str) -> FormatResult {
    if datetime_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid RFC 3339 date-time"))
    }
}

fn validate_duration(s: &str) -> FormatResult {
    if duration_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid ISO 8601 duration"))
    }
}

fn validate_email(s: &str) -> FormatResult {
    if email_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid email address"))
    }
}

fn validate_hostname(s: &str) -> FormatResult {
    if hostname_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid hostname"))
    }
}

fn validate_ipv4(s: &str) -> FormatResult {
    if ipv4_regex().is_match(s) && s.split('.').all(|o| o.parse::<u8>().is_ok()) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid IPv4 address"))
    }
}

fn validate_ipv6(s: &str) -> FormatResult {
    if ipv6_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid IPv6 address"))
    }
}

fn validate_uri(s: &str) -> FormatResult {
    if uri_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid URI"))
    }
}

fn validate_uri_reference(s: &str) -> FormatResult {
    if uri_reference_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid URI-reference"))
    }
}

fn validate_uuid(s: &str) -> FormatResult {
    if uuid_regex().is_match(s) {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid UUID"))
    }
}

fn validate_json_pointer(s: &str) -> FormatResult {
    if s.is_empty() || s.starts_with('/') {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid JSON Pointer"))
    }
}

fn validate_relative_json_pointer(s: &str) -> FormatResult {
    let digits_end = s.bytes().take_while(u8::is_ascii_digit).count();
    if digits_end > 0 {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid relative JSON Pointer"))
    }
}

fn validate_regex_format(s: &str) -> FormatResult {
    if Regex::new(s).is_ok() {
        FormatResult::Valid
    } else {
        FormatResult::Invalid(format!("{s:?} is not a valid ECMA regex"))
    }
}

// ── Compiled regexes (built once per call site; cheap since Rust caches them) ─

fn date_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])$").unwrap())
}

fn time_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)$",
        )
        .unwrap()
    })
}

fn datetime_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])[Tt](?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d(?:\.\d+)?(?:[Zz]|[+-](?:[01]\d|2[0-3]):[0-5]\d)$")
            .unwrap()
    })
}

fn duration_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^P(?:\d+Y)?(?:\d+M)?(?:\d+D)?(?:T(?:\d+H)?(?:\d+M)?(?:\d+(?:\.\d+)?S)?)?$")
            .unwrap()
    })
}

fn email_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap())
}

fn hostname_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:[a-zA-Z0-9](?:[a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)*[a-zA-Z]{2,}$").unwrap()
    })
}

fn ipv4_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$").unwrap())
}

fn ipv6_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[0-9a-fA-F:]+$").unwrap())
}

fn uri_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9+\-.]*:").unwrap())
}

fn uri_reference_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(?:[a-zA-Z][a-zA-Z0-9+\-.]*:)?[^\s]*$").unwrap())
}

fn uuid_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_format_valid() {
        assert_eq!(validate_date("2024-01-15"), FormatResult::Valid);
        assert!(matches!(
            validate_date("not-a-date"),
            FormatResult::Invalid(_)
        ));
    }

    #[test]
    fn datetime_format_valid() {
        assert_eq!(
            validate_datetime("2024-01-15T12:00:00Z"),
            FormatResult::Valid
        );
        assert!(matches!(validate_datetime("bad"), FormatResult::Invalid(_)));
    }

    #[test]
    fn email_format_valid() {
        assert_eq!(validate_email("user@example.com"), FormatResult::Valid);
        assert!(matches!(
            validate_email("not-an-email"),
            FormatResult::Invalid(_)
        ));
    }

    #[test]
    fn uuid_format_valid() {
        assert_eq!(
            validate_uuid("550e8400-e29b-41d4-a716-446655440000"),
            FormatResult::Valid
        );
        assert!(matches!(
            validate_uuid("not-uuid"),
            FormatResult::Invalid(_)
        ));
    }

    #[test]
    fn registry_annotation_mode_by_default() {
        let reg = FormatRegistry::with_defaults();
        let result = reg.validate("date", "not-a-date");
        assert_eq!(result, FormatResult::Annotation);
    }

    #[test]
    fn registry_assertion_mode() {
        let mut reg = FormatRegistry::with_defaults();
        reg.assert_format("date");
        assert_eq!(reg.validate("date", "2024-01-15"), FormatResult::Valid);
        assert!(matches!(
            reg.validate("date", "bad"),
            FormatResult::Invalid(_)
        ));
    }

    #[test]
    fn json_pointer_validation() {
        assert_eq!(validate_json_pointer(""), FormatResult::Valid);
        assert_eq!(validate_json_pointer("/foo/bar"), FormatResult::Valid);
        assert!(matches!(
            validate_json_pointer("no-slash"),
            FormatResult::Invalid(_)
        ));
    }

    #[test]
    fn ipv4_validation() {
        assert_eq!(validate_ipv4("192.168.1.1"), FormatResult::Valid);
        assert!(matches!(
            validate_ipv4("256.0.0.1"),
            FormatResult::Invalid(_)
        ));
    }
}
