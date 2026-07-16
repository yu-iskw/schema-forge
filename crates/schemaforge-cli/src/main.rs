//! Schemaforge command-line interface.
//!
//! # Commands
//!
//! | Command         | Aliases              | Description                              |
//! |-----------------|----------------------|------------------------------------------|
//! | `inspect`       | `info`               | Dialect, node count, capabilities        |
//! | `normalize`     |                      | OpenAPI 3.0 nullable → type arrays       |
//! | `generate`      | `compile`, `codegen` | Generate Rust types from a schema        |
//! | `validate`      |                      | Validate a JSON instance against schema  |
//! | `benchmark`     |                      | Time the compilation pipeline            |
//! | `explain`       |                      | Print representation / strategy info     |
//! | `compatibility` |                      | Detect breaking changes between schemas  |
//! | `vendor`        |                      | Copy local schema files to vendor dir    |
//! | `lock`          |                      | Write `schemaforge.lock.toml`            |

mod error;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use schemaforge_compiler::{Compiler, sha256_hex};

use crate::error::{CliError, DiagFormat, from_compile, print_error, to_exit_code};

// ── CLI definition ─────────────────────────────────────────────────────────────

/// Schemaforge — hybrid JSON Schema compiler.
#[derive(Debug, Parser)]
#[command(name = "sfg", version, about, long_about = None)]
struct Cli {
    /// Diagnostic output format.
    #[arg(long, global = true, default_value = "human", value_name = "FORMAT")]
    diagnostic_format: DiagFormat,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print dialect, node count, and capabilities for a schema.
    #[command(alias = "info")]
    Inspect(InspectArgs),

    /// Normalise an OpenAPI 3.0 document (nullable → type arrays).
    Normalize(NormalizeArgs),

    /// Generate Rust types from a JSON Schema.
    #[command(alias = "compile", alias = "codegen")]
    Generate(GenerateArgs),

    /// Validate a JSON instance against a schema.
    Validate(ValidateArgs),

    /// Time the schema compilation pipeline.
    Benchmark(BenchmarkArgs),

    /// Print representation strategy and codegen decisions for a schema.
    Explain(ExplainArgs),

    /// Compare two schemas and report breaking changes.
    Compatibility(CompatArgs),

    /// Copy local schema files into a vendor directory.
    Vendor(VendorArgs),

    /// Write a schemaforge.lock.toml pinning local schemas by SHA-256.
    Lock(LockArgs),
}

// ── per-command arg structs ────────────────────────────────────────────────────

#[derive(Debug, clap::Args)]
struct InspectArgs {
    /// Path to the JSON Schema file (`.json` or `.yaml`).
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
}

#[derive(Debug, clap::Args)]
struct NormalizeArgs {
    /// Path to the OpenAPI 3.0 document (`.json` or `.yaml`).
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Write output to FILE instead of stdout.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct GenerateArgs {
    /// Path to the JSON Schema file (`.json` or `.yaml`).
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Write output to FILE instead of stdout.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct ValidateArgs {
    /// Path to the JSON Schema file.
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Path to the JSON instance to validate.
    #[arg(value_name = "INSTANCE")]
    instance: PathBuf,
    /// Print a note even when the instance is valid.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct BenchmarkArgs {
    /// Path to the JSON Schema file (`.json` or `.yaml`).
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
}

#[derive(Debug, clap::Args)]
struct ExplainArgs {
    /// Path to the JSON Schema file (`.json` or `.yaml`).
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
}

#[derive(Debug, clap::Args)]
struct CompatArgs {
    /// Path to schema A (the baseline / older schema).
    #[arg(value_name = "SCHEMA_A")]
    schema_a: PathBuf,
    /// Path to schema B (the candidate / newer schema).
    #[arg(value_name = "SCHEMA_B")]
    schema_b: PathBuf,
}

#[derive(Debug, clap::Args)]
struct VendorArgs {
    /// Path to the JSON Schema file to vendor.
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Directory to vendor into (default: `vendor/`).
    #[arg(short, long, value_name = "DIR")]
    vendor_dir: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct LockArgs {
    /// Path to the JSON Schema file to lock.
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Write lock file to PATH (default: `schemaforge.lock.toml`).
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
}

// ── entry point ────────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fmt = cli.diagnostic_format.clone();
    match dispatch(cli.command, &fmt) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            print_error(&e, &fmt);
            to_exit_code(&e)
        }
    }
}

fn dispatch(cmd: Command, fmt: &DiagFormat) -> Result<(), CliError> {
    match cmd {
        Command::Inspect(a) => cmd_inspect(&a, fmt),
        Command::Normalize(a) => cmd_normalize(&a, fmt),
        Command::Generate(a) => cmd_generate(&a, fmt),
        Command::Validate(a) => cmd_validate(&a, fmt),
        Command::Benchmark(a) => cmd_benchmark(&a),
        Command::Explain(a) => cmd_explain(&a, fmt),
        Command::Compatibility(a) => cmd_compat(&a, fmt),
        Command::Vendor(a) => cmd_vendor(&a),
        Command::Lock(a) => cmd_lock(&a),
    }
}

// ── inspect ────────────────────────────────────────────────────────────────────

fn cmd_inspect(args: &InspectArgs, fmt: &DiagFormat) -> Result<(), CliError> {
    let ir = load_schema(&args.schema)?;
    let result = schemaforge_compiler::inspect_ir(&ir);
    match fmt {
        DiagFormat::Json | DiagFormat::Sarif => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        DiagFormat::Human => {
            println!("dialect:    {}", result.dialect_uri);
            println!("source:     {}", result.source_uri);
            println!("digest:     {}", result.source_digest);
            println!("nodes:      {}", result.node_count);
            println!("caps:       {}", result.capabilities.join(", "));
        }
    }
    Ok(())
}

// ── normalize ──────────────────────────────────────────────────────────────────

fn cmd_normalize(args: &NormalizeArgs, fmt: &DiagFormat) -> Result<(), CliError> {
    let text = read_file(&args.schema)?;
    let ext = file_ext(&args.schema);
    let normalized = build_normalized_map(ext, &text)?;
    let out = match fmt {
        DiagFormat::Json | DiagFormat::Sarif => serde_json::to_string(&normalized)?,
        DiagFormat::Human => serde_json::to_string_pretty(&normalized)?,
    };
    write_or_print(args.output.as_deref(), &out)
}

fn build_normalized_map(ext: &str, text: &str) -> Result<serde_json::Value, CliError> {
    let doc = if ext == "yaml" || ext == "yml" {
        schemaforge_openapi::OpenApiDoc::from_yaml(text)?
    } else {
        schemaforge_openapi::OpenApiDoc::from_json(text)?
    };
    let mut map = serde_json::Map::new();
    for (name, entry) in doc.component_schemas() {
        map.insert(name, entry.schema);
    }
    Ok(serde_json::Value::Object(map))
}

// ── generate ───────────────────────────────────────────────────────────────────

fn cmd_generate(args: &GenerateArgs, _fmt: &DiagFormat) -> Result<(), CliError> {
    let ir = load_schema(&args.schema)?;
    let opts = schemaforge_codegen_rust::CodegenOptions {
        module_doc: Some(format!("Generated from {}", args.schema.display())),
        max_bytes: Some(schemaforge_codegen_rust::DEFAULT_MAX_BYTES),
        ..schemaforge_codegen_rust::CodegenOptions::default()
    };
    let code = schemaforge_codegen_rust::generate(&ir, &opts)?;
    write_or_print(args.output.as_deref(), &code)
}

// ── validate ───────────────────────────────────────────────────────────────────

fn cmd_validate(args: &ValidateArgs, fmt: &DiagFormat) -> Result<(), CliError> {
    let schema_text = read_file(&args.schema)?;
    let instance_text = read_file(&args.instance)?;
    let schema_val: serde_json::Value = parse_json(&schema_text)?;
    let instance_val: serde_json::Value = parse_json(&instance_text)?;
    let validator = schemaforge_jsonschema::Validator::new(
        &schema_val,
        schemaforge_jsonschema::ValidationOptions::default(),
    )?;
    let output = validator.validate(&instance_val);
    report_validation(&output, fmt, args.verbose)
}

fn report_validation(
    output: &schemaforge_jsonschema::ValidationOutput,
    fmt: &DiagFormat,
    verbose: bool,
) -> Result<(), CliError> {
    if output.is_valid() {
        println!("valid");
        if verbose {
            println!("(no errors)");
        }
        return Ok(());
    }
    report_errors(&output.errors, fmt);
    Err(CliError::ValidationFailed)
}

fn report_errors(errors: &[schemaforge_jsonschema::ValidationError], fmt: &DiagFormat) {
    match fmt {
        DiagFormat::Json | DiagFormat::Sarif => report_errors_json(errors),
        DiagFormat::Human => report_errors_human(errors),
    }
}

fn error_to_json(e: &schemaforge_jsonschema::ValidationError) -> serde_json::Value {
    serde_json::json!({"path": e.instance_path, "message": e.message})
}

fn report_errors_json(errors: &[schemaforge_jsonschema::ValidationError]) {
    let errs: Vec<_> = errors.iter().map(error_to_json).collect();
    let v = serde_json::json!({"valid": false, "errors": errs});
    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
}

fn report_errors_human(errors: &[schemaforge_jsonschema::ValidationError]) {
    eprintln!("invalid");
    for err in errors {
        eprintln!("  - {} (at {})", err.message, err.instance_path);
    }
}

// ── benchmark ──────────────────────────────────────────────────────────────────

fn cmd_benchmark(args: &BenchmarkArgs) -> Result<(), CliError> {
    let text = read_file(&args.schema)?;
    let uri = file_uri(&args.schema);
    let ext = file_ext(&args.schema);
    let start = std::time::Instant::now();
    let ir = compile_text(&mut Compiler::new(), &uri, ext, &text)?;
    let elapsed = start.elapsed();
    let info = schemaforge_compiler::inspect_ir(&ir);
    println!("elapsed_ms: {}", elapsed.as_millis());
    println!("nodes:      {}", info.node_count);
    println!("dialect:    {}", info.dialect_uri);
    Ok(())
}

// ── explain ────────────────────────────────────────────────────────────────────

fn cmd_explain(args: &ExplainArgs, fmt: &DiagFormat) -> Result<(), CliError> {
    let ir = load_schema(&args.schema)?;
    let result = schemaforge_compiler::explain_ir(&ir);
    match fmt {
        DiagFormat::Json | DiagFormat::Sarif => {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        DiagFormat::Human => print_explain_human(&result),
    }
    Ok(())
}

fn print_explain_human(r: &schemaforge_compiler::ExplainResult) {
    println!("dialect:     {}", r.dialect_uri);
    println!("strategy:    {}", r.type_strategy);
    println!("nullable:    {}", r.nullable);
    println!("properties:  {}", r.property_count);
    println!("combinators: {}", r.combinator_count);
    if !r.codegen_hints.is_empty() {
        println!("hints:");
        for hint in &r.codegen_hints {
            println!("  - {hint}");
        }
    }
}

// ── compatibility ──────────────────────────────────────────────────────────────

fn cmd_compat(args: &CompatArgs, fmt: &DiagFormat) -> Result<(), CliError> {
    let ir_a = load_schema(&args.schema_a)?;
    let ir_b = load_schema(&args.schema_b)?;
    let breaks = schemaforge_analysis::find_breaking_changes(&ir_a.root, &ir_b.root);
    report_breaking_changes(&breaks, fmt)
}

fn report_breaking_changes(breaks: &[String], fmt: &DiagFormat) -> Result<(), CliError> {
    if breaks.is_empty() {
        println!("compatible: no breaking changes detected");
        return Ok(());
    }
    print_breaking_changes(breaks, fmt);
    Err(CliError::ValidationFailed)
}

fn print_breaking_changes(breaks: &[String], fmt: &DiagFormat) {
    match fmt {
        DiagFormat::Json | DiagFormat::Sarif => {
            let v = serde_json::json!({"breaking_changes": breaks});
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
        }
        DiagFormat::Human => {
            eprintln!("breaking changes detected:");
            for b in breaks {
                eprintln!("  - {b}");
            }
        }
    }
}

// ── vendor ─────────────────────────────────────────────────────────────────────

fn cmd_vendor(args: &VendorArgs) -> Result<(), CliError> {
    let dir = args
        .vendor_dir
        .as_deref()
        .unwrap_or_else(|| Path::new("vendor"));
    std::fs::create_dir_all(dir)?;
    let filename = args
        .schema
        .file_name()
        .ok_or_else(|| CliError::Internal("schema path has no filename".to_owned()))?;
    let dest = dir.join(filename);
    std::fs::copy(&args.schema, &dest)?;
    println!("vendored: {}", dest.display());
    Ok(())
}

// ── lock ───────────────────────────────────────────────────────────────────────

fn cmd_lock(args: &LockArgs) -> Result<(), CliError> {
    let bytes = std::fs::read(&args.schema)?;
    let digest = sha256_hex(&bytes);
    let size = bytes.len();
    let uri = file_uri(&args.schema);
    let mut lock_file = schemaforge_resolver::LockFile::new();
    lock_file.upsert(schemaforge_resolver::LockEntry { uri, digest, size });
    let toml = lock_file.to_toml()?;
    let out_path = args
        .output
        .as_deref()
        .unwrap_or_else(|| Path::new("schemaforge.lock.toml"));
    std::fs::write(out_path, &toml)?;
    println!("wrote {}", out_path.display());
    Ok(())
}

// ── shared helpers ─────────────────────────────────────────────────────────────

fn load_schema(path: &Path) -> Result<schemaforge_ir::SchemaIr, CliError> {
    let text = read_file(path)?;
    let uri = file_uri(path);
    let ext = file_ext(path);
    compile_text(&mut Compiler::new(), &uri, ext, &text)
}

fn compile_text(
    compiler: &mut Compiler,
    uri: &str,
    ext: &str,
    text: &str,
) -> Result<schemaforge_ir::SchemaIr, CliError> {
    if ext == "yaml" || ext == "yml" {
        compiler.compile_yaml(uri, text).map_err(from_compile)
    } else {
        compiler.compile_json(uri, text).map_err(from_compile)
    }
}

fn read_file(path: &Path) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(CliError::Io)
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn file_ext(path: &Path) -> &str {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("")
}

fn write_or_print(output: Option<&Path>, content: &str) -> Result<(), CliError> {
    output.map_or_else(
        || {
            print!("{content}");
            Ok(())
        },
        |path| std::fs::write(path, content).map_err(CliError::Io),
    )
}

fn parse_json(text: &str) -> Result<serde_json::Value, CliError> {
    serde_json::from_str(text).map_err(|e| CliError::Parse(e.to_string()))
}
