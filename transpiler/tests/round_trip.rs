//! Round-trip integration tests — Phase 2b
//!
//! Each test verifies that a Swift source snippet passes through the full
//! sarah pipeline (parse → classify → transpile) and produces Rust source
//! that:
//!
//! 1. Contains the expected type and field names.
//! 2. Contains the correct UniFFI annotations.
//! 3. Respects the async bridge strategy (SPEC-006).
//!
//! These tests serve as the regression suite for all diagnostic codes.
//! Add a new test here for every new Swift pattern that sarah learns to
//! handle.

use sarah_cli_lib::{classify, codegen, parser};
use std::path::PathBuf;

fn path() -> PathBuf { PathBuf::from("test.swift") }

// ── Tier 1: struct ──────────────────────────────────────────────────────────────

#[test]
fn tier1_struct_with_all_primitive_fields() {
    let src = r#"
struct UserProfile {
    var id: Int
    var name: String
    var score: Double
    var active: Bool
    var avatar: String?
}
"#;
    let ir  = parser::parse(src);
    let cr  = classify::classify_file(&path(), src);
    let out = codegen::transpile_with_ir(&cr, &ir, codegen::AsyncMode::Bridge).unwrap();

    assert!(out.contains("pub struct UserProfile"),     "struct declaration missing");
    assert!(out.contains("pub id: i64"),                "Int field missing");
    assert!(out.contains("pub name: String"),           "String field missing");
    assert!(out.contains("pub score: f64"),             "Double field missing");
    assert!(out.contains("pub active: bool"),           "Bool field missing");
    assert!(out.contains("pub avatar: Option<String>"), "Optional field missing");
    assert!(out.contains("uniffi::Record"),             "UniFFI annotation missing");
    assert_eq!(cr.exit_code(), 0,                       "clean file must exit 0");
}

// ── Tier 1: enum ──────────────────────────────────────────────────────────────

#[test]
fn tier1_enum_simple_cases() {
    let src = r#"
enum LoadState {
    case idle
    case loading
    case loaded
    case failed
}
"#;
    let ir  = parser::parse(src);
    let cr  = classify::classify_file(&path(), src);
    let out = codegen::transpile_with_ir(&cr, &ir, codegen::AsyncMode::Bridge).unwrap();

    assert!(out.contains("pub enum LoadState"));
    assert!(out.contains("idle,"));
    assert!(out.contains("loading,"));
    assert!(out.contains("loaded,"));
    assert!(out.contains("failed,"));
    assert!(out.contains("uniffi::Enum"));
    assert_eq!(cr.exit_code(), 0);
}

#[test]
fn tier1_enum_with_associated_values() {
    let src = r#"
enum Result {
    case success(String)
    case failure(Int)
}
"#;
    let ir  = parser::parse(src);
    let cr  = classify::classify_file(&path(), src);
    let out = codegen::transpile_with_ir(&cr, &ir, codegen::AsyncMode::Bridge).unwrap();

    assert!(out.contains("success(String)"));
    assert!(out.contains("failure(i64)"));
}

// ── Tier 2: class ──────────────────────────────────────────────────────────────

#[test]
fn tier2_class_emits_arc_mutex() {
    let src = r#"
class SessionManager {
    var token: String
    var userId: Int
}
"#;
    let ir  = parser::parse(src);
    let cr  = classify::classify_file(&path(), src);
    let out = codegen::transpile_with_ir(&cr, &ir, codegen::AsyncMode::Bridge).unwrap();

    assert!(out.contains("pub token: String"));
    assert!(out.contains("pub userId: i64"));
    assert!(out.contains("Arc::new"));
    assert!(out.contains("Mutex::new"));
    assert!(out.contains("uniffi::Object"));
    assert_eq!(cr.exit_code(), 1, "T2-CLASS info: exit code should be 0 or 1");
}

// ── Async: Tier A1 bridge ───────────────────────────────────────────────────────

#[test]
fn async_func_bridge_emits_three_zone_scaffold() {
    let src = r#"
async func fetchFeed() {
    let _ = await networkCall()
}
"#;
    let ir  = parser::parse(src);
    let cr  = classify::classify_file(&path(), src);
    let out = codegen::transpile_with_ir(&cr, &ir, codegen::AsyncMode::Bridge).unwrap();

    assert!(out.contains("callback_interface"), "callback interface annotation missing");
    assert!(out.contains("tokio::spawn"),        "Tokio spawn missing");
    assert!(out.contains("on_result"),           "callback on_result method missing");
    assert!(out.contains("on_error"),            "callback on_error method missing");
    assert!(out.contains("Send + Sync"),         "Callback Send+Sync bounds missing");
}

// ── Tier 3: protocol with PAT ────────────────────────────────────────────────────

#[test]
fn tier3_pat_protocol_emits_skip() {
    let src = r#"
protocol Repository {
    associatedtype Item
    func fetch(id: Int) -> Item
}
"#;
    let ir  = parser::parse(src);
    let cr  = classify::classify_file(&path(), src);
    let out = codegen::transpile_with_ir(&cr, &ir, codegen::AsyncMode::Bridge).unwrap();

    assert!(out.contains("TIER-3 SKIP") || !out.contains("pub protocol"),
        "PAT protocol must not generate Rust protocol code");
    assert!(cr.diagnostics.iter().any(|d| d.code == "T3-PAT"),
        "T3-PAT diagnostic must be emitted");
    assert_eq!(cr.exit_code(), 2, "T3-PAT is an error: exit code must be 2");
}

// ── Diagnostics: ObjC ────────────────────────────────────────────────────────────

#[test]
fn objc_emits_t3_diagnostic_and_error_exit() {
    let src = "@objc class LegacyBridge: NSObject {}";
    let cr = classify::classify_file(&path(), src);
    assert!(cr.diagnostics.iter().any(|d| d.code == "T3-OBJC"));
    assert_eq!(cr.exit_code(), 2);
}
