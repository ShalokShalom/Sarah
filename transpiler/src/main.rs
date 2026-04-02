//! sarah-cli — entry point
//!
//! Dispatches subcommands to the transpiler pipeline stages.
//!
//! ```text
//! sarah classify  <file.swift>   -- run SPEC-001 classifier, emit tier JSON
//! sarah lower     <file.swift>   -- run SPEC-002 Tier 1 lowering, emit Rust source
//! sarah transpile <file.swift>   -- full pipeline: classify → lower → emit
//! ```

mod classify;
mod codegen;
mod diagnostics;
mod types;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sarah",
    about = "Swift-to-Rust transpiler",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Diagnostic output format
    #[arg(long, default_value = "terminal", global = true)]
    diagnostics: diagnostics::OutputMode,
}

#[derive(Subcommand)]
enum Commands {
    /// Classify a Swift source file and emit tier JSON (SPEC-001)
    Classify {
        /// Path to the Swift source file
        file: PathBuf,
    },
    /// Lower a Swift source file to Tier 1 Rust (SPEC-002)
    Lower {
        /// Path to the Swift source file
        file: PathBuf,
        /// Output Rust file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Full pipeline: classify → lower → emit Rust + UniFFI annotations
    Transpile {
        /// Path to the Swift source file
        file: PathBuf,
        /// Output Rust file path (defaults to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Async bridging mode
        #[arg(long, default_value = "bridge")]
        async_mode: codegen::AsyncMode,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let diag_mode = cli.diagnostics;

    match cli.command {
        Commands::Classify { file } => {
            let source = std::fs::read_to_string(&file)?;
            let result = classify::classify_file(&file, &source);
            let report = serde_json::to_string_pretty(&result)?;
            println!("{report}");
            diagnostics::print_all(&result.diagnostics, diag_mode);
            std::process::exit(result.exit_code());
        }
        Commands::Lower { file, output } => {
            let source = std::fs::read_to_string(&file)?;
            let class_result = classify::classify_file(&file, &source);
            diagnostics::print_all(&class_result.diagnostics, diag_mode);
            let rust_source = codegen::lower_tier1(&class_result)?;
            emit_output(output, &rust_source)?;
            std::process::exit(class_result.exit_code());
        }
        Commands::Transpile { file, output, async_mode } => {
            let source = std::fs::read_to_string(&file)?;
            let class_result = classify::classify_file(&file, &source);
            diagnostics::print_all(&class_result.diagnostics, diag_mode);
            let rust_source = codegen::transpile(&class_result, async_mode)?;
            emit_output(output, &rust_source)?;
            std::process::exit(class_result.exit_code());
        }
    }
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
