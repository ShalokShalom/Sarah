//! SPEC-001 classifier — Swift source → tier JSON
//!
//! This is a line-oriented heuristic classifier. It does not parse a full
//! Swift AST (that requires `swift-syntax` integration in Phase 2a); instead
//! it applies pattern matching to derive tier assignments with high confidence
//! for the common cases. A full AST parser will replace this in Phase 2a.
//!
//! Emits a `ClassificationResult` that is both machine-serialisable (JSON)
//! and consumed by the code generator.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::diagnostics::{Diagnostic, Severity};

// ── Tier types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTier {
    Tier1,
    Tier2,
    Tier3,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsyncTier {
    A1,
    A1Sync,
    A2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarationResult {
    pub name:         String,
    pub kind:         String,
    pub tier:         SyncTier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub async_tier:   Option<AsyncTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combined_tier: Option<String>,
    pub diagnostics:  Vec<String>,  // diagnostic codes only, for JSON output
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    pub file:         String,
    pub file_tier:    String,
    pub declarations: Vec<DeclarationResult>,
    #[serde(skip)]
    pub diagnostics:  Vec<Diagnostic>,
}

impl ClassificationResult {
    /// SPEC-005 exit code based on worst diagnostic level.
    pub fn exit_code(&self) -> i32 {
        crate::diagnostics::exit_code(&self.diagnostics)
    }
}

// ── Classifier ────────────────────────────────────────────────────────────────

/// Classify a Swift source file and return a `ClassificationResult`.
/// The `file` path is used only for diagnostic messages.
pub fn classify_file(file: &Path, source: &str) -> ClassificationResult {
    let file_str = file.to_string_lossy().to_string();
    let mut declarations = Vec::new();
    let mut diagnostics  = Vec::new();
    let mut worst_tier   = SyncTier::Tier1;

    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();

        // ── struct ────────────────────────────────────────────────────────────
        if let Some(name) = extract_decl_name(trimmed, "struct ") {
            declarations.push(DeclarationResult {
                name,
                kind: "struct".into(),
                tier: SyncTier::Tier1,
                async_tier: None,
                combined_tier: None,
                diagnostics: vec![],
            });
        }

        // ── enum ──────────────────────────────────────────────────────────────
        else if let Some(name) = extract_decl_name(trimmed, "enum ") {
            declarations.push(DeclarationResult {
                name,
                kind: "enum".into(),
                tier: SyncTier::Tier1,
                async_tier: None,
                combined_tier: None,
                diagnostics: vec![],
            });
        }

        // ── class ─────────────────────────────────────────────────────────────
        else if let Some(name) = extract_decl_name(trimmed, "class ") {
            if worst_tier == SyncTier::Tier1 {
                worst_tier = SyncTier::Tier2;
            }
            diagnostics.push(
                Diagnostic::info("T2-CLASS",
                    &format!("`class {name}` → Arc<Mutex<T>> in Rust output"),
                    &file_str, line_num + 1)
                .with_see("SPEC-003")
            );
            declarations.push(DeclarationResult {
                name,
                kind: "class".into(),
                tier: SyncTier::Tier2,
                async_tier: None,
                combined_tier: None,
                diagnostics: vec!["T2-CLASS".into()],
            });
        }

        // ── protocol ──────────────────────────────────────────────────────────
        else if let Some(name) = extract_decl_name(trimmed, "protocol ") {
            let has_assoc = source.lines()
                .skip(line_num)
                .take(30)
                .any(|l| l.trim_start().starts_with("associatedtype "));
            if has_assoc {
                worst_tier = SyncTier::Tier3;
                diagnostics.push(
                    Diagnostic::error("T3-PAT",
                        &format!("Protocol `{name}` has associated types — Shell only"),
                        &file_str, line_num + 1)
                    .with_hint("Move protocol conformances to the Swift Shell layer.")
                    .with_see("SPEC-001 §3.2")
                );
                declarations.push(DeclarationResult {
                    name,
                    kind: "protocol".into(),
                    tier: SyncTier::Tier3,
                    async_tier: None,
                    combined_tier: None,
                    diagnostics: vec!["T3-PAT".into()],
                });
            }
        }

        // ── @objc / NSObject ─────────────────────────────────────────────────
        else if trimmed.contains("@objc") || trimmed.contains(": NSObject") {
            worst_tier = SyncTier::Tier3;
            diagnostics.push(
                Diagnostic::error("T3-OBJC",
                    "ObjC interop detected — not transpilable",
                    &file_str, line_num + 1)
                .with_hint("Keep ObjC-dependent code in the Swift Shell.")
                .with_see("SPEC-001 §3.2")
            );
        }

        // ── func ─────────────────────────────────────────────────────────────
        else if trimmed.starts_with("func ")
             || trimmed.starts_with("public func ")
             || trimmed.starts_with("internal func ")
             || trimmed.starts_with("private func ")
             || trimmed.starts_with("async func ")
             || trimmed.contains(" async func ")
        {
            let is_async = trimmed.contains("async func") || trimmed.contains("async ");
            let name = extract_func_name(trimmed).unwrap_or_else(|| "unknown".into());

            // Check if receiver is a class (simple heuristic: look back)
            let receiver_is_class = is_class_receiver(&declarations);

            let async_tier = if is_async {
                if receiver_is_class {
                    diagnostics.push(
                        Diagnostic::warn("ASYNC-LOCK-RISK",
                            &format!("Async func `{name}` on class receiver — lock-before-await pattern applied"),
                            &file_str, line_num + 1)
                        .with_hint("Acquire Mutex, clone state, release lock before any .await point.")
                        .with_see("SPEC-006 §4, SPEC-003 §5")
                    );
                    Some(AsyncTier::A2)
                } else {
                    // Heuristic: scan forward for `await` in next 20 lines
                    let has_await = source.lines()
                        .skip(line_num + 1)
                        .take(20)
                        .any(|l| l.contains("await "));
                    if has_await {
                        Some(AsyncTier::A1)
                    } else {
                        diagnostics.push(
                            Diagnostic::info("ASYNC-NO-AWAIT",
                                &format!("Async func `{name}` has no await — spawn_blocking used"),
                                &file_str, line_num + 1)
                        );
                        Some(AsyncTier::A1Sync)
                    }
                }
            } else {
                None
            };

            let combined = async_tier.as_ref().map(|at| {
                let sync = if receiver_is_class { "2" } else { "1" };
                let async_s = match at {
                    AsyncTier::A1     => "A1",
                    AsyncTier::A1Sync => "A1-sync",
                    AsyncTier::A2     => "A2",
                };
                format!("{sync}/{async_s}")
            });

            let tier = if receiver_is_class { SyncTier::Tier2 } else { SyncTier::Tier1 };

            declarations.push(DeclarationResult {
                name,
                kind: "func".into(),
                tier,
                async_tier,
                combined_tier: combined,
                diagnostics: vec![],
            });
        }
    }

    let file_tier = match worst_tier {
        SyncTier::Tier1 => "Core",
        SyncTier::Tier2 => "Core (Tier 2 present)",
        SyncTier::Tier3 => "Shell",
    }.to_owned();

    ClassificationResult {
        file: file_str,
        file_tier,
        declarations,
        diagnostics,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extract_decl_name(line: &str, keyword: &str) -> Option<String> {
    let rest = line.strip_prefix(keyword)
        .or_else(|| line.strip_prefix(&format!("public {keyword}")))
        .or_else(|| line.strip_prefix(&format!("internal {keyword}")))
        .or_else(|| line.strip_prefix(&format!("private {keyword}")))?;
    let name = rest.split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()?;
    if name.is_empty() { None } else { Some(name.to_owned()) }
}

fn extract_func_name(line: &str) -> Option<String> {
    let after_func = line.find("func ").map(|i| &line[i + 5..])?;
    let name = after_func
        .split(|c: char| c == '(' || c == '<' || c == ' ')
        .next()?;
    if name.is_empty() { None } else { Some(name.to_owned()) }
}

/// Heuristic: the most recently declared top-level item is a class.
fn is_class_receiver(declarations: &[DeclarationResult]) -> bool {
    declarations.last().map(|d| d.kind == "class").unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn path() -> std::path::PathBuf { PathBuf::from("test.swift") }

    #[test]
    fn struct_is_tier1() {
        let src = "struct Point { var x: Double; var y: Double }";
        let r = classify_file(&path(), src);
        assert_eq!(r.file_tier, "Core");
        assert_eq!(r.declarations[0].tier, SyncTier::Tier1);
    }

    #[test]
    fn class_is_tier2() {
        let src = "class SessionManager { var token: String = \"\" }";
        let r = classify_file(&path(), src);
        assert_eq!(r.file_tier, "Core (Tier 2 present)");
        assert_eq!(r.declarations[0].tier, SyncTier::Tier2);
    }

    #[test]
    fn objc_is_tier3() {
        let src = "@objc class LegacyBridge: NSObject {}";
        let r = classify_file(&path(), src);
        // ObjC line detected → Tier 3 but no declaration extracted from @objc line itself
        assert!(r.diagnostics.iter().any(|d| d.code == "T3-OBJC"));
    }

    #[test]
    fn async_func_no_await_is_a1sync() {
        let src = "struct Foo {}\nasync func compute() -> Int { return 42 }";
        let r = classify_file(&path(), src);
        let func_decl = r.declarations.iter().find(|d| d.name == "compute");
        assert!(func_decl.is_some());
        assert_eq!(func_decl.unwrap().async_tier, Some(AsyncTier::A1Sync));
    }
}
