//! sarah-cli — entry point
//!
//! Subcommands:
//! ```text
//! sarah classify   <file.swift>                        -- SPEC-001 classifier
//! sarah parse      <file.swift> [--parser treesitter]  -- emit SwiftFile IR JSON
//! sarah lower      <file.swift> [--parser treesitter]  -- Tier 1 lowering, emit Rust
//! sarah transpile  <file.swift> [--parser treesitter]  -- full pipeline
//! sarah shell      <file.swift> [--parser treesitter]  -- emit Swift Shell source
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

use parser::ParserBackend;

#[derive(Parser)]
#[command(
    name    = "sarah",
    about   = "Swift-to-Rust transpiler",
    version = "0.2.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "terminal", global = true)]
    diagnostics: diagnostics::OutputMode,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the SPEC-001 tier classifier and emit a JSON report.
    Classify { file: PathBuf },

    /// Parse a Swift file and emit the SwiftFile IR as JSON.
    Parse {
        file: PathBuf,
        /// Parser backend. See SPEC-008 §5.1 for why this is a flag.
        #[arg(long, default_value = "treesitter")]
        parser: ParserBackend,
    },

    /// Lower Tier 1 declarations to Rust (SPEC-002).
    Lower {
        file:   PathBuf,
        #[arg(short, long)] output: Option<PathBuf>,
        #[arg(long, default_value = "treesitter")]
        parser: ParserBackend,
    },

    /// Run the full transpilation pipeline.
    Transpile {
        file:   PathBuf,
        #[arg(short, long)] output: Option<PathBuf>,
        #[arg(long, default_value = "bridge")]
        async_mode: codegen::AsyncMode,
        /// Parser backend. See SPEC-008 §5.1 for why this is a flag.
        #[arg(long, default_value = "treesitter")]
        parser: ParserBackend,
    },

    /// Emit Swift Shell source from a Swift input file (SPEC-006).
    Shell {
        file:   PathBuf,
        #[arg(short, long)] output: Option<PathBuf>,
        /// UniFFI module name to import in the generated Shell.
        #[arg(long, default_value = "CoreFFI")] ffi_module: String,
        /// Parser backend. See SPEC-008 §5.1 for why this is a flag.
        #[arg(long, default_value = "treesitter")]
        parser: ParserBackend,
    },
}

fn main() -> Result<()> {
    let cli       = Cli::parse();
    let diag_mode = cli.diagnostics;

    match cli.command {
        Commands::Classify { file } => {
            let source = std::fs::read_to_string(&file)?;
            let result = classify::classify_file(&file, &source);
            println!("{}", serde_json::to_string_pretty(&result)?);
            diagnostics::print_all(&result.diagnostics, diag_mode);
            std::process::exit(result.exit_code());
        }

        Commands::Parse { file, parser: backend } => {
            let source = std::fs::read_to_string(&file)?;
            let (ir, parse_diags) = parser::parse_with_backend(&source, backend);
            println!("{}", serde_json::to_string_pretty(&ir)?);
            diagnostics::print_all(&parse_diags, diag_mode);
        }

        Commands::Lower { file, output, parser: backend } => {
            let source = std::fs::read_to_string(&file)?;
            let cr  = classify::classify_file(&file, &source);
            diagnostics::print_all(&cr.diagnostics, diag_mode);
            let (ir, parse_diags) = parser::parse_with_backend(&source, backend);
            diagnostics::print_all(&parse_diags, diag_mode);
            let out = codegen::lower_tier1_with_ir(&cr, &ir)?;
            emit_output(output, &out)?;
            std::process::exit(cr.exit_code());
        }

        Commands::Transpile { file, output, async_mode, parser: backend } => {
            let source = std::fs::read_to_string(&file)?;
            let cr  = classify::classify_file(&file, &source);
            diagnostics::print_all(&cr.diagnostics, diag_mode);
            let (ir, parse_diags) = parser::parse_with_backend(&source, backend);
            diagnostics::print_all(&parse_diags, diag_mode);
            let out = codegen::transpile_with_ir(&cr, &ir, async_mode)?;
            emit_output(output, &out)?;
            std::process::exit(cr.exit_code());
        }

        Commands::Shell { file, output, ffi_module, parser: backend } => {
            let source = std::fs::read_to_string(&file)?;
            let (ir, parse_diags) = parser::parse_with_backend(&source, backend);
            diagnostics::print_all(&parse_diags, diag_mode);
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
