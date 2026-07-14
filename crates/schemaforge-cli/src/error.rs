//! CLI error types, exit-code mapping, and diagnostic output formatting.

use std::process::ExitCode;

use clap::ValueEnum;
use schemaforge_compiler::CompileError;
use thiserror::Error;

/// Diagnostic output format selected via `--diagnostic-format`.
#[derive(Debug, Clone, Default, ValueEnum)]
pub enum DiagFormat {
    /// Human-readable plain text (default).
    #[default]
    Human,
    /// Machine-readable JSON array on stderr.
    Json,
    /// SARIF 2.1.0 JSON on stderr.
    Sarif,
}

/// Top-level CLI error.
///
/// Each variant maps to a specific exit code (see [`to_exit_code`]).
#[derive(Debug, Error)]
pub enum CliError {
    /// Exit 1 — syntax or validation error.
    #[error("parse error: {0}")]
    Parse(String),
    /// Exit 1 — validation produced errors.
    #[error("validation failed")]
    ValidationFailed,
    /// Exit 2 — `$ref` resolver failure.
    #[error("resolver error: {0}")]
    Resolver(String),
    /// Exit 4 — code generation failure.
    #[error("codegen error: {0}")]
    Codegen(#[from] schemaforge_codegen_rust::CodegenError),
    /// Exit 5 — schema / validator construction failure.
    #[error("schema error: {0}")]
    Schema(#[from] schemaforge_jsonschema::SchemaError),
    /// Exit 6 — I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Exit 6 — JSON serialisation error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Exit 6 — OpenAPI parse error.
    #[error("OpenAPI error: {0}")]
    OpenApi(#[from] schemaforge_openapi::OpenApiError),
    /// Exit 6 — internal / unexpected error.
    #[error("internal: {0}")]
    Internal(String),
}

/// Convert a [`CompileError`] to a [`CliError`] with the right exit-code
/// semantics.
#[must_use]
pub fn from_compile(e: CompileError) -> CliError {
    match e {
        CompileError::JsonParse(s) | CompileError::YamlParse(s) => CliError::Parse(s),
        CompileError::UnresolvedRef { uri, reason } => {
            CliError::Resolver(format!("unresolved ref `{uri}`: {reason}"))
        }
        CompileError::CyclicRef { uri } => {
            CliError::Resolver(format!("cyclic $ref detected: `{uri}`"))
        }
    }
}

/// Map a [`CliError`] to its numeric exit code.
#[must_use]
pub fn to_exit_code(e: &CliError) -> ExitCode {
    let code: u8 = match e {
        CliError::Parse(_) | CliError::ValidationFailed => 1,
        CliError::Resolver(_) => 2,
        CliError::Codegen(_) => 3,
        CliError::Schema(_) => 4,
        CliError::Io(_) | CliError::Json(_) | CliError::OpenApi(_) | CliError::Internal(_) => 5,
    };
    ExitCode::from(code)
}

/// Print a [`CliError`] to stderr in the requested format.
pub fn print_error(e: &CliError, fmt: &DiagFormat) {
    match fmt {
        DiagFormat::Human => eprintln!("error: {e}"),
        DiagFormat::Json => print_json_error(e),
        DiagFormat::Sarif => print_sarif_error(e),
    }
}

fn print_json_error(e: &CliError) {
    let v = serde_json::json!({"level": "error", "message": e.to_string()});
    eprintln!("{}", serde_json::to_string(&v).unwrap_or_default());
}

fn print_sarif_error(e: &CliError) {
    let msg = e.to_string();
    let v = minimal_sarif(&msg);
    eprintln!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

fn minimal_sarif(msg: &str) -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {"name": "schemaforge"}},
            "results": [{"ruleId": "error", "level": "error", "message": {"text": msg}}]
        }]
    })
}
