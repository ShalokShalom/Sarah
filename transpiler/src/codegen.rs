//! Code generator — SPEC-002 Tier 1 lowering and async shell generation
//!
//! Phase 2b: now consumes `SwiftFile` IR from `parser.rs` to emit
//! accurate struct fields, enum variants, function signatures, and
//! async callback scaffolding.
//!
//! Phase 2a stubs (classify-only paths) are preserved for compatibility.

use anyhow::Result;
use std::str::FromStr;

use crate::classify::{AsyncTier, ClassificationResult, DeclarationResult, SyncTier};
use crate::parser::{SwiftClass, SwiftEnum, SwiftFile, SwiftFunc, SwiftStruct};

// ── Async mode ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncMode {
    Bridge,
    Native,
}

impl FromStr for AsyncMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bridge" => Ok(AsyncMode::Bridge),
            "native" => Ok(AsyncMode::Native),
            other    => Err(format!("unknown async mode: {other}")),
        }
    }
}

impl clap::ValueEnum for AsyncMode {
    fn value_variants<'a>() -> &'a [Self] { &[AsyncMode::Bridge, AsyncMode::Native] }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            AsyncMode::Bridge => clap::builder::PossibleValue::new("bridge"),
            AsyncMode::Native => clap::builder::PossibleValue::new("native"),
        })
    }
}

// ── Public entry points (Phase 2b — IR-aware) ──────────────────────────────

/// Lower Tier 1 declarations using the full parsed IR.
pub fn lower_tier1_with_ir(result: &ClassificationResult, ir: &SwiftFile) -> Result<String> {
    transpile_with_ir(result, ir, AsyncMode::Bridge)
}

/// Full transpile using parsed IR, with chosen async mode.
pub fn transpile_with_ir(
    result: &ClassificationResult,
    ir: &SwiftFile,
    async_mode: AsyncMode,
) -> Result<String> {
    let mut out = preamble(&result.file);

    // Structs
    for s in &ir.structs {
        out.push_str(&emit_struct_ir(s));
    }
    // Enums
    for e in &ir.enums {
        out.push_str(&emit_enum_ir(e));
    }
    // Classes
    for c in &ir.classes {
        out.push_str(&emit_class_ir(c));
    }
    // Top-level functions
    for f in &ir.funcs {
        out.push_str(&emit_func_ir(f, async_mode));
    }

    // Tier 3 skips from classifier (declarations the IR can't lower)
    for decl in &result.declarations {
        if decl.tier == SyncTier::Tier3 {
            out.push_str(&format!(
                "// TIER-3 SKIP: `{}` ({}) is Shell-only — see T3 diagnostics.\n\n",
                decl.name, decl.kind
            ));
        }
    }

    Ok(out)
}

// ── Phase 2a compatibility shims ───────────────────────────────────────────────

pub fn lower_tier1(result: &ClassificationResult) -> Result<String> {
    transpile(result, AsyncMode::Bridge)
}

pub fn transpile(result: &ClassificationResult, async_mode: AsyncMode) -> Result<String> {
    let mut out = preamble(&result.file);
    for decl in &result.declarations {
        match decl.tier {
            SyncTier::Tier1 => out.push_str(&emit_tier1_decl(decl, async_mode)),
            SyncTier::Tier2 => out.push_str(&emit_tier2_decl(decl, async_mode)),
            SyncTier::Tier3 => out.push_str(&format!(
                "// TIER-3 SKIP: `{}` ({}) is Shell-only.\n\n", decl.name, decl.kind
            )),
        }
    }
    Ok(out)
}

// ── IR-aware emitters ───────────────────────────────────────────────────────────────

fn emit_struct_ir(s: &SwiftStruct) -> String {
    let fields: String = s.fields.iter().map(|f| {
        let rust_type = f.rust_type()
            .unwrap_or_else(|| format!("() /* UNMAPPED: {} */", f.swift_type));
        let mutability = if f.mutable { "" } else { "" }; // Rust fields are always mutable via &mut
        format!("    pub {}: {rust_type},\n", f.name)
    }).collect();

    let methods: String = s.methods.iter().map(|m| emit_func_ir(m, AsyncMode::Bridge)).collect();

    format!(
        "/// Transpiled from Swift `struct {name}` (SPEC-002 Tier 1)\n\
         #[derive(Debug, Clone, uniffi::Record)]\n\
         pub struct {name} {{\n\
         {fields}}}\
         \n\n{methods}",
        name = s.name,
        fields = if fields.is_empty() { "    // (no fields)\n".to_owned() } else { fields },
    )
}

fn emit_enum_ir(e: &SwiftEnum) -> String {
    let cases: String = e.cases.iter().map(|c| {
        if c.associated_types.is_empty() {
            format!("    {},\n", c.name)
        } else {
            let types: Vec<String> = c.associated_types.iter().map(|t| {
                crate::types::swift_to_rust(t)
                    .map(|r| r.to_owned())
                    .unwrap_or_else(|| format!("() /* UNMAPPED: {t} */"))
            }).collect();
            format!("    {}({}),\n", c.name, types.join(", "))
        }
    }).collect();

    format!(
        "/// Transpiled from Swift `enum {name}` (SPEC-002 Tier 1)\n\
         #[derive(Debug, Clone, uniffi::Enum)]\n\
         pub enum {name} {{\n\
         {cases}}}\n\n",
        name = e.name,
        cases = if cases.is_empty() { "    // (no cases)\n".to_owned() } else { cases },
    )
}

fn emit_class_ir(c: &SwiftClass) -> String {
    let fields: String = c.fields.iter().map(|f| {
        let rust_type = f.rust_type()
            .unwrap_or_else(|| format!("() /* UNMAPPED: {} */", f.swift_type));
        format!("    pub {}: {rust_type},\n", f.name)
    }).collect();

    let deinit_note = if c.has_deinit {
        "/// NOTE: Swift `deinit` present — Drop impl required (SPEC-003, CLASS-DEINIT)\n"
    } else { "" };

    let superclass_note = c.superclass.as_ref().map(|sc| {
        format!("/// NOTE: superclass `{sc}` — flattened to composition (CLASS-SUBCLASS)\n")
    }).unwrap_or_default();

    let methods: String = c.methods.iter().map(|m| emit_func_ir(m, AsyncMode::Bridge)).collect();

    format!(
        "{deinit_note}{superclass_note}\
         /// Transpiled from Swift `class {name}` (SPEC-003 Tier 2 — Arc<Mutex<T>>)\n\
         #[derive(uniffi::Object)]\n\
         pub struct {name} {{\n\
         \tinner: Mutex<{name}Inner>,\n\
         }}\n\n\
         pub struct {name}Inner {{\n\
         {fields}}}\n\n\
         #[uniffi::export]\n\
         impl {name} {{\n\
         \t#[uniffi::constructor]\n\
         \tpub fn new() -> Arc<Self> {{\n\
         \t\tArc::new(Self {{ inner: Mutex::new({name}Inner {{ {defaults} }}) }})\n\
         \t}}\n\
         }}\n\n{methods}",
        name    = &c.name,
        fields  = if fields.is_empty() { "    // (no fields)\n".to_owned() } else { fields },
        defaults = c.fields.iter().map(|f| format!("{}: Default::default()", f.name)).collect::<Vec<_>>().join(", "),
    )
}

fn emit_func_ir(f: &SwiftFunc, async_mode: AsyncMode) -> String {
    let sig = f.rust_signature();
    if f.is_async {
        emit_async_func_sig(&f.name, &sig, async_mode)
    } else {
        format!(
            "/// Transpiled from Swift `func {name}` (SPEC-002 Tier 1)\n\
             #[uniffi::export]\n\
             {sig} {{\n\
             \ttodo!(\"Phase 2c: implement {name}\")\n\
             }}\n\n",
            name = f.name,
        )
    }
}

fn emit_async_func_sig(name: &str, sig: &str, async_mode: AsyncMode) -> String {
    match async_mode {
        AsyncMode::Bridge => format!(
            "/// Transpiled from Swift `async func {name}` (SPEC-006 §3, Tier A1)\n\
             /// Swift Shell wraps with `withCheckedThrowingContinuation`.\n\
             #[uniffi::export(callback_interface)]\n\
             pub trait {name}Callback: Send + Sync {{\n\
             \tfn on_result(&self, result: ());  // TODO: typed result\n\
             \tfn on_error(&self, error: String);\n\
             }}\n\n\
             #[uniffi::export]\n\
             pub fn {name}(callback: Arc<dyn {name}Callback>) {{\n\
             \ttokio::spawn(async move {{\n\
             \t\t// TODO: implement {name} (Phase 2c)\n\
             \t\tcallback.on_result(());\n\
             \t}});\n\
             }}\n\n",
            name = name,
        ),
        AsyncMode::Native => format!(
            "/// Transpiled from Swift `async func {name}` (native UniFFI async — Stage 2)\n\
             #[uniffi::export]\n\
             pub async fn {name}() {{\n\
             \ttodo!(\"Phase 2c: implement {name}\")\n\
             }}\n\n",
            name = name,
        ),
    }
}

// ── Phase 2a classify-only emitters (preserved for compatibility) ─────────────

fn emit_tier1_decl(decl: &DeclarationResult, async_mode: AsyncMode) -> String {
    match decl.kind.as_str() {
        "struct" => format!(
            "/// Transpiled from Swift `struct {name}` (SPEC-002 Tier 1)\n\
             #[derive(Debug, Clone, uniffi::Record)]\n\
             pub struct {name} {{\n    // fields: run `sarah transpile` for full IR output\n}}\n\n",
            name = decl.name),
        "enum" => format!(
            "/// Transpiled from Swift `enum {name}` (SPEC-002 Tier 1)\n\
             #[derive(Debug, Clone, uniffi::Enum)]\n\
             pub enum {name} {{\n    // variants: run `sarah transpile` for full IR output\n}}\n\n",
            name = decl.name),
        "func" => {
            let is_async = decl.async_tier.is_some();
            if is_async {
                emit_async_func_sig(&decl.name, &format!("pub fn {}()", decl.name), async_mode)
            } else {
                format!(
                    "#[uniffi::export]\npub fn {}() {{ todo!() }}\n\n",
                    decl.name)
            }
        }
        _ => format!("// UNSUPPORTED: {} {}\n\n", decl.kind, decl.name),
    }
}

fn emit_tier2_decl(decl: &DeclarationResult, _async_mode: AsyncMode) -> String {
    if decl.kind == "class" {
        format!(
            "/// Transpiled from Swift `class {name}` (SPEC-003 Tier 2)\n\
             #[derive(uniffi::Object)]\n\
             pub struct {name} {{ inner: Mutex<{name}Inner> }}\n\
             pub struct {name}Inner {{ /* fields: run `sarah transpile` */ }}\n\
             #[uniffi::export]\n\
             impl {name} {{\n\
             \t#[uniffi::constructor]\n\
             \tpub fn new() -> Arc<Self> {{ Arc::new(Self {{ inner: Mutex::new({name}Inner {{}}) }}) }}\n\
             }}\n\n",
            name = decl.name)
    } else {
        emit_tier1_decl(decl, _async_mode)
    }
}

// ── Preamble ──────────────────────────────────────────────────────────────────

fn preamble(source_file: &str) -> String {
    format!(
        "// Generated by sarah-cli — DO NOT EDIT\n\
         // Source: {source_file}\n\
         // Phase 2b output — IR-aware Tier 1 / Tier 2 lowering\n\
         // See: SPEC-002, SPEC-003, SPEC-004, SPEC-006\n\n\
         use std::sync::{{Arc, Mutex}};\n\n"
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify_file;
    use crate::parser::parse;
    use std::path::PathBuf;

    fn path() -> PathBuf { PathBuf::from("test.swift") }

    #[test]
    fn struct_with_fields_emits_correct_rust() {
        let src = r#"struct Point {
    var x: Double
    var y: Double
    let label: String
}"#;
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("pub x: f64"));
        assert!(out.contains("pub y: f64"));
        assert!(out.contains("pub label: String"));
        assert!(out.contains("uniffi::Record"));
    }

    #[test]
    fn enum_with_cases_emits_variants() {
        let src = r#"enum Direction {
    case north
    case south
    case coordinate(Double, Double)
}"#;
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("north,"));
        assert!(out.contains("south,"));
        assert!(out.contains("coordinate(f64, f64)"));
    }

    #[test]
    fn class_emits_arc_mutex_with_fields() {
        let src = r#"class SessionManager {
    var token: String
    var userId: Int
}"#;
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("pub token: String"));
        assert!(out.contains("pub userId: i64"));
        assert!(out.contains("Arc::new"));
        assert!(out.contains("Mutex::new"));
    }

    #[test]
    fn async_func_emits_callback_interface() {
        let src = "async func loadData() {}";
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("callback_interface"));
        assert!(out.contains("tokio::spawn"));
    }
}
