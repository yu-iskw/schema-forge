//! Diagnostic reporting for Schemaforge.
//!
//! [`Diagnostic`]s are structured error/warning/info/hint messages attached to
//! source [`Span`]s. A [`DiagnosticBag`] accumulates them during compilation
//! and can later be formatted for display or checked for errors.

use std::fmt::Write as _;

use schemaforge_source::{SourceMap, Span};

/// Severity level of a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// A hint or suggestion (informational).
    Hint,
    /// An informational note.
    Info,
    /// A warning that does not prevent compilation.
    Warning,
    /// A hard error that prevents successful compilation.
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hint => f.write_str("hint"),
            Self::Info => f.write_str("info"),
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

/// A labelled source location attached to a diagnostic.
#[derive(Debug, Clone)]
pub struct Label {
    /// The source span this label points to.
    pub span: Span,
    /// Short message displayed alongside the span.
    pub message: String,
}

impl Label {
    /// Create a new label.
    #[must_use]
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}

/// A structured compiler diagnostic with severity, code, and source locations.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Severity of this diagnostic.
    pub severity: Severity,
    /// Machine-readable error code (e.g. `"E0001"`).
    pub code: Option<String>,
    /// Human-readable primary message.
    pub message: String,
    /// Source labels pointing to relevant locations.
    pub labels: Vec<Label>,
    /// Additional notes appended below the primary message.
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// Create an error diagnostic.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// Create a warning diagnostic.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// Create an info diagnostic.
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(Severity::Info, message)
    }

    fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Attach a source code label to this diagnostic.
    #[must_use]
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// Attach a note string to this diagnostic.
    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// Attach a machine-readable code to this diagnostic.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Returns `true` if this is an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.severity, self.message)
    }
}

/// Accumulates [`Diagnostic`]s during compilation.
#[derive(Debug, Default)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    /// Create an empty bag.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a diagnostic to the bag.
    pub fn push(&mut self, diag: Diagnostic) {
        self.diagnostics.push(diag);
    }

    /// Emit an error with a span label.
    pub fn error(&mut self, span: Span, message: impl Into<String>) {
        let msg = message.into();
        let label = Label::new(span, msg.clone());
        self.push(Diagnostic::error(msg).with_label(label));
    }

    /// Emit a warning with a span label.
    pub fn warning(&mut self, span: Span, message: impl Into<String>) {
        let msg = message.into();
        let label = Label::new(span, msg.clone());
        self.push(Diagnostic::warning(msg).with_label(label));
    }

    /// Returns `true` if any error-severity diagnostic is present.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// Number of diagnostics in the bag.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Returns `true` when no diagnostics have been added.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Iterate over all diagnostics.
    pub fn iter(&self) -> std::slice::Iter<'_, Diagnostic> {
        self.diagnostics.iter()
    }

    /// Write all diagnostics to a [`SourceMap`]-aware string.
    #[must_use]
    pub fn render(&self, map: &SourceMap) -> String {
        self.diagnostics
            .iter()
            .map(|d| render_diagnostic(d, map))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl<'a> IntoIterator for &'a DiagnosticBag {
    type Item = &'a Diagnostic;
    type IntoIter = std::slice::Iter<'a, Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.iter()
    }
}

impl IntoIterator for DiagnosticBag {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.diagnostics.into_iter()
    }
}

// ── SARIF export ──────────────────────────────────────────────────────────────

/// Export a [`DiagnosticBag`] as a minimal SARIF 2.1.0 JSON value.
///
/// The SARIF document has one run containing one result per diagnostic.
#[must_use]
pub fn to_sarif(bag: &DiagnosticBag) -> serde_json::Value {
    let results: Vec<serde_json::Value> = bag.iter().map(sarif_result).collect();
    serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "schemaforge",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/yu-iskw/rust-projet-template"
                }
            },
            "results": results
        }]
    })
}

fn sarif_result(d: &Diagnostic) -> serde_json::Value {
    let level = sarif_level(d.severity);
    let rule_id = d.code.as_deref().unwrap_or("SF0000");
    serde_json::json!({
        "ruleId": rule_id,
        "level": level,
        "message": {"text": d.message}
    })
}

const fn sarif_level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info | Severity::Hint => "note",
    }
}

// ── human-readable render ─────────────────────────────────────────────────────

fn render_diagnostic(d: &Diagnostic, map: &SourceMap) -> String {
    let mut out = format!("{}: {}", d.severity, d.message);
    for label in &d.labels {
        let span = label.span;
        if let Some(file) = map.get(span.source) {
            let (line, col) = file.line_col(span.start);
            write!(
                out,
                "\n  --> {}:{}:{}: {}",
                file.uri(),
                line,
                col,
                label.message
            )
            .unwrap_or(());
        }
    }
    for note in &d.notes {
        write!(out, "\n  = note: {note}").unwrap_or(());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemaforge_source::{SourceMap, Span};

    fn make_span() -> (SourceMap, Span) {
        let mut map = SourceMap::new();
        let id = map.add("test://schema.json", r#"{"type":"string"}"#);
        (map, Span::new(id, 0, 7))
    }

    #[test]
    fn bag_records_errors() {
        let (_, span) = make_span();
        let mut bag = DiagnosticBag::new();
        bag.error(span, "bad type");
        assert!(bag.has_errors());
        assert_eq!(bag.len(), 1);
    }

    #[test]
    fn bag_warning_not_error() {
        let (_, span) = make_span();
        let mut bag = DiagnosticBag::new();
        bag.warning(span, "deprecated field");
        assert!(!bag.has_errors());
    }

    #[test]
    fn render_includes_location() {
        let (map, span) = make_span();
        let mut bag = DiagnosticBag::new();
        bag.error(span, "unexpected token");
        let rendered = bag.render(&map);
        assert!(rendered.contains("test://schema.json"));
        assert!(rendered.contains("error"));
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Info > Severity::Hint);
    }

    #[test]
    fn diagnostic_builder() {
        let d = Diagnostic::error("test error")
            .with_code("E0001")
            .with_note("see docs");
        assert!(d.is_error());
        assert_eq!(d.code.as_deref(), Some("E0001"));
        assert_eq!(d.notes.len(), 1);
    }

    #[test]
    fn into_iterator_ref() {
        let (_, span) = make_span();
        let mut bag = DiagnosticBag::new();
        bag.error(span, "e1");
        bag.warning(span, "w1");
        assert_eq!((&bag).into_iter().count(), 2);
    }

    #[test]
    fn sarif_export_structure() {
        let (_, span) = make_span();
        let mut bag = DiagnosticBag::new();
        bag.error(span, "bad keyword");
        let sarif = to_sarif(&bag);
        assert_eq!(sarif["version"], "2.1.0");
        let results = &sarif["runs"][0]["results"];
        assert_eq!(results[0]["level"], "error");
        assert_eq!(results[0]["message"]["text"], "bad keyword");
    }

    #[test]
    fn sarif_warning_level() {
        let (_, span) = make_span();
        let mut bag = DiagnosticBag::new();
        bag.warning(span, "deprecated");
        let sarif = to_sarif(&bag);
        assert_eq!(sarif["runs"][0]["results"][0]["level"], "warning");
    }
}
