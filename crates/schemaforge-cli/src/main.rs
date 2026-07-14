//! Schemaforge command-line interface.
//!
//! # Usage
//!
//! ```text
//! schemaforge compile   schema.json           # Compile to IR (JSON)
//! schemaforge validate  schema.json data.json # Validate instance
//! schemaforge codegen   schema.json           # Generate Rust types
//! schemaforge info      schema.json           # Print dialect / digest
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use schemaforge_compiler::{CompileError, Compiler};
use schemaforge_jsonschema::{ValidationOptions, Validator};
use thiserror::Error;

/// Schemaforge — hybrid JSON Schema compiler.
#[derive(Debug, Parser)]
#[command(name = "schemaforge", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile a JSON Schema file and print the IR as JSON.
    Compile(CompileArgs),
    /// Validate a JSON instance against a schema.
    Validate(ValidateArgs),
    /// Generate Rust types from a JSON Schema.
    Codegen(CodegenArgs),
    /// Print dialect, source URI, and content digest for a schema.
    Info(InfoArgs),
}

#[derive(Debug, clap::Args)]
struct CompileArgs {
    /// Path to the JSON Schema file (`.json` or `.yaml`).
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Pretty-print the IR JSON output.
    #[arg(short, long)]
    pretty: bool,
}

#[derive(Debug, clap::Args)]
struct ValidateArgs {
    /// Path to the JSON Schema file.
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Path to the JSON instance file to validate.
    #[arg(value_name = "INSTANCE")]
    instance: PathBuf,
    /// Print all validation errors even when valid.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Debug, clap::Args)]
struct CodegenArgs {
    /// Path to the JSON Schema file.
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
    /// Output file path (defaults to stdout).
    #[arg(short, long, value_name = "OUTPUT")]
    output: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct InfoArgs {
    /// Path to the JSON Schema file.
    #[arg(value_name = "SCHEMA")]
    schema: PathBuf,
}

/// Top-level CLI error.
#[derive(Debug, Error)]
enum CliError {
    /// IO error reading a file.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Compilation failed.
    #[error("compile error: {0}")]
    Compile(#[from] CompileError),
    /// JSON serialisation failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// Schema validation setup failed.
    #[error("schema error: {0}")]
    Schema(#[from] schemaforge_jsonschema::SchemaError),
    /// Code generation failed.
    #[error("codegen error: {0}")]
    Codegen(#[from] schemaforge_codegen_rust::CodegenError),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Compile(args) => cmd_compile(&args),
        Command::Validate(args) => cmd_validate(&args),
        Command::Codegen(args) => cmd_codegen(&args),
        Command::Info(args) => cmd_info(&args),
    }
}

fn cmd_compile(args: &CompileArgs) -> Result<(), CliError> {
    let ir = compile_schema(&args.schema)?;
    let json_out = if args.pretty {
        serde_json::to_string_pretty(&ir)?
    } else {
        serde_json::to_string(&ir)?
    };
    println!("{json_out}");
    Ok(())
}

fn cmd_validate(args: &ValidateArgs) -> Result<(), CliError> {
    let schema_text = read_file(&args.schema)?;
    let instance_text = read_file(&args.instance)?;
    let schema_val: serde_json::Value = serde_json::from_str(&schema_text)?;
    let instance_val: serde_json::Value = serde_json::from_str(&instance_text)?;
    let validator = Validator::new(&schema_val, ValidationOptions::default())?;
    let output = validator.validate(&instance_val);
    if output.is_valid() {
        println!("valid");
    } else {
        eprintln!("invalid");
        for err in &output.errors {
            eprintln!("  - {} (at {})", err.message, err.instance_path);
        }
        return Ok(());
    }
    if args.verbose {
        println!("(no errors)");
    }
    Ok(())
}

fn cmd_codegen(args: &CodegenArgs) -> Result<(), CliError> {
    let ir = compile_schema(&args.schema)?;
    let code = schemaforge_codegen_rust::generate(
        &ir,
        &schemaforge_codegen_rust::CodegenOptions::default(),
    )?;
    match &args.output {
        Some(path) => std::fs::write(path, &code)?,
        None => print!("{code}"),
    }
    Ok(())
}

fn cmd_info(args: &InfoArgs) -> Result<(), CliError> {
    let ir = compile_schema(&args.schema)?;
    println!("source_uri:     {}", ir.source_uri);
    println!("dialect:        {}", ir.dialect_uri);
    println!("source_digest:  {}", ir.source_digest);
    Ok(())
}

fn compile_schema(path: &PathBuf) -> Result<schemaforge_ir::SchemaIr, CliError> {
    let uri = format!("file://{}", path.display());
    let text = read_file(path)?;
    let mut compiler = Compiler::new();
    let ext = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");
    let ir = if ext == "yaml" || ext == "yml" {
        compiler.compile_yaml(&uri, &text)?
    } else {
        compiler.compile_json(&uri, &text)?
    };
    Ok(ir)
}

fn read_file(path: &PathBuf) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(CliError::Io)
}
