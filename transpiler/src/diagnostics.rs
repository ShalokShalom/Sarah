//! Diagnostic engine — SPEC-005
//!
//! Provides structured diagnostic records, severity levels, terminal
//! and JSON output, and the exit-code protocol.

use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// ── Severity ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    /// Informational — no action required.
    Info,
    /// Hint — suggested refactor, not blocking.
    Hint,
    /// Warning — output generated with mitigation; developer review required.
    Warn,
    /// Error — output not generated for this declaration.
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info  => write!(f, "INFO"),
            Severity::Hint  => write!(f, "HINT"),
            Severity::Warn  => write!(f, "WARN"),
            Severity::Error => write!(f, "ERROR"),
        }
    }
}

// ── Diagnostic record ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code:    String,
    pub level:   Severity,
    pub message: String,
    pub file:    String,
    pub line:    usize,
    pub column:  usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub see:     Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: &str, file: &str, line: usize) -> Self {
        Self::new(Severity::Error, code, message, file, line)
    }
    pub fn warn(code: &str, message: &str, file: &str, line: usize) -> Self {
        Self::new(Severity::Warn, code, message, file, line)
    }
    pub fn info(code: &str, message: &str, file: &str, line: usize) -> Self {
        Self::new(Severity::Info, code, message, file, line)
    }
    pub fn hint(code: &str, message: &str, file: &str, line: usize) -> Self {
        Self::new(Severity::Hint, code, message, file, line)
    }
    fn new(level: Severity, code: &str, message: &str, file: &str, line: usize) -> Self {
        Self {
            code:    code.to_owned(),
            level,
            message: message.to_owned(),
            file:    file.to_owned(),
            line,
            column:  1,
            span:    None,
            hint:    None,
            see:     None,
        }
    }
    pub fn with_hint(mut self, hint: &str) -> Self {
        self.hint = Some(hint.to_owned());
        self
    }
    pub fn with_see(mut self, see: &str) -> Self {
        self.see = Some(see.to_owned());
        self
    }
    pub fn with_span(mut self, span: &str) -> Self {
        self.span = Some(span.to_owned());
        self
    }
}

// ── Output mode ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Terminal,
    Json,
}

impl FromStr for OutputMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "terminal" => Ok(OutputMode::Terminal),
            "json"     => Ok(OutputMode::Json),
            other      => Err(format!("unknown diagnostic mode: {other}")),
        }
    }
}

// clap ValueEnum shim
impl clap::ValueEnum for OutputMode {
    fn value_variants<'a>() -> &'a [Self] {
        &[OutputMode::Terminal, OutputMode::Json]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            OutputMode::Terminal => clap::builder::PossibleValue::new("terminal"),
            OutputMode::Json     => clap::builder::PossibleValue::new("json"),
        })
    }
}

// ── Print helpers ─────────────────────────────────────────────────────────────

pub fn print_all(diags: &[Diagnostic], mode: OutputMode) {
    if diags.is_empty() { return; }
    match mode {
        OutputMode::Terminal => {
            for d in diags { print_terminal(d); }
        }
        OutputMode::Json => {
            let json = serde_json::to_string_pretty(diags)
                .unwrap_or_else(|_| "[]".to_owned());
            eprintln!("{json}");
        }
    }
}

fn print_terminal(d: &Diagnostic) {
    let (symbol, label) = match d.level {
        Severity::Error => ("✗".red().bold(),   "ERROR".red().bold()),
        Severity::Warn  => ("⚠".yellow().bold(), "WARN".yellow().bold()),
        Severity::Info  => ("ℹ".cyan(),           "INFO".cyan()),
        Severity::Hint  => ("→".green(),          "HINT".green()),
    };
    eprintln!("{symbol}  {label}  {}", d.code.bold());
    eprintln!("   {}:{}:{}", d.file, d.line, d.column);
    eprintln!("   {}", d.message);
    if let Some(ref h) = d.hint {
        eprintln!("   {} {h}", "→".green());
    }
    if let Some(ref s) = d.see {
        eprintln!("   See: {s}");
    }
    eprintln!();
}

// ── Exit code ─────────────────────────────────────────────────────────────────

/// Compute the SPEC-005 exit code from a slice of diagnostics.
pub fn exit_code(diags: &[Diagnostic]) -> i32 {
    let worst = diags.iter().map(|d| &d.level).max();
    match worst {
        None                  => 0,
        Some(Severity::Info)  => 0,
        Some(Severity::Hint)  => 0,
        Some(Severity::Warn)  => 1,
        Some(Severity::Error) => 2,
    }
}
