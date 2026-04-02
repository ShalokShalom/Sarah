//! sarah-cli — entry point
//!
//! Subcommands:
//! ```text
//! sarah classify   <file.swift>   -- SPEC-001 classifier, emit tier JSON
//! sarah parse      <file.swift>   -- AST parser, emit SwiftFile IR JSON
//! sarah lower      <file.swift>   -- SPEC-002 Tier 1 lowering, emit Rust
//! sarah transpile  <file.swift>   -- full pipeline: classify → lower → emit Rust
//! sarah shell      <file.swift>   -- emit Swift Shell source (Phase 2c)
//! ```

mod classify;
mod codegen;
mod diagnostics;
mod drop_gen;
mod parser;
mod shell_gen;
mod types;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name     = "sarah",
    about    = "Swift-to-Rust transpiler",
    version  = "0.2.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "terminal", global = true)]
    diagnostics: diagnostics::OutputMode,
}

#[derive(Subcommand)]
enum Commands {
    Classify  { file: PathBuf },
    Parse     { file: PathBuf },
    Lower     { file: PathBuf, #[arg(short, long)] output: Option<PathBuf> },
    Transpile {
        file:   PathBuf,
        #[arg(short, long)] output: Option<PathBuf>,
        #[arg(long, default_value = "bridge")] async_mode: codegen::AsyncMode,
    },
    /// Emit Swift Shell source from a Swift input file (Phase 2c).
    Shell {
        file:   PathBuf,
        #[arg(short, long)] output: Option<PathBuf>,
        /// UniFFI module name to import in the generated Shell
        #[arg(long, default_value = "CoreFFI")] ffi_module: String,
    },
}

fn main() -> Result<()> {
    let cli      = Cli::parse();
    let diag_mode = cli.diagnostics;

    match cli.command {
        Commands::Classify { file } => {
            let source = std::fs::read_to_string(&file)?;
            let result = classify::classify_file(&file, &source);
            println!("{}", serde_json::to_string_pretty(&result)?);
            diagnostics::print_all(&result.diagnostics, diag_mode);
            std::process::exit(result.exit_code());
        }
        Commands::Parse { file } => {
            let source = std::fs::read_to_string(&file)?;
            let ir = parser::parse(&source);
            println!("{}", serde_json::to_string_pretty(&ir)?);
        }
        Commands::Lower { file, output } => {
            let source = std::fs::read_to_string(&file)?;
            let cr  = classify::classify_file(&file, &source);
            diagnostics::print_all(&cr.diagnostics, diag_mode);
            let ir  = parser::parse(&source);
            let out = codegen::lower_tier1_with_ir(&cr, &ir)?;
            emit_output(output, &out)?;
            std::process::exit(cr.exit_code());
        }
        Commands::Transpile { file, output, async_mode } => {
            let source = std::fs::read_to_string(&file)?;
            let cr  = classify::classify_file(&file, &source);
            diagnostics::print_all(&cr.diagnostics, diag_mode);
            let ir  = parser::parse(&source);
            let out = codegen::transpile_with_ir(&cr, &ir, async_mode)?;
            emit_output(output, &out)?;
            std::process::exit(cr.exit_code());
        }
        Commands::Shell { file, output, ffi_module } => {
            let source = std::fs::read_to_string(&file)?;
            let ir  = parser::parse(&source);
            let out = shell_gen::emit_shell(&ir, &ffi_module);
            emit_output(output, &out)?;
        }
    }
    Ok(())
}

fn emit_output(path: Option<PathBuf>, content: &str) -> Result<()> {
    match path {
        Some(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(p, content)?;
        }
        None => print!("{content}"),
    }
    Ok(())
}
