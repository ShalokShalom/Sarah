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
use crate::drop_gen;
use crate::parser::{SwiftClass, SwiftEnum, SwiftFile, SwiftFunc, SwiftStruct};

// ── Async mode ───────────────────────────────────────────────────────────────────────────────

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

// ── Public entry points (Phase 2b — IR-aware) ──────────────────────────────────

/// Lower Tier 1 declarations using the full parsed IR.
pub fn lower_tier1_with_ir(result: &ClassificationResult, ir: &SwiftFile) -> Result<String> {
    transpile_with_ir(result, ir, AsyncMode::Bridge)
}

/// Full transpile using parsed IR, with chosen async mode.
/// Returns generated Rust source. Any diagnostics produced during
/// Drop generation are appended to `result.diagnostics` via the
/// returned string's embedded comments (they are also emitted to
/// stderr by the caller via `diagnostics::print_all`).
pub fn transpile_with_ir(
    result: &ClassificationResult,
    ir: &SwiftFile,
    async_mode: AsyncMode,
) -> Result<String> {
    let mut out  = preamble(&result.file);
    let mut diags = result.diagnostics.clone();

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
        let (class_src, drop_diags) = emit_class_ir(c);
        out.push_str(&class_src);
        diags.extend(drop_diags);
    }
    // Top-level functions — resolve async tier from classifier output
    for f in &ir.funcs {
        let async_tier = result.declarations.iter()
            .find(|d| d.name == f.name && d.kind == "func")
            .and_then(|d| d.async_tier.clone());
        out.push_str(&emit_func_ir(f, async_mode, async_tier.as_ref()));
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

// ── Phase 2a compatibility shims ─────────────────────────────────────────────────────

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

// ── IR-aware emitters ────────────────────────────────────────────────────────────────

fn emit_struct_ir(s: &SwiftStruct) -> String {
    let fields: String = s.fields.iter().map(|f| {
        let rust_type = f.rust_type()
            .unwrap_or_else(|| format!("() /* UNMAPPED: {} */", f.swift_type));
        format!("    pub {}: {rust_type},\n", f.name)
    }).collect();

    let methods: String = s.methods.iter()
        .map(|m| emit_func_ir(m, AsyncMode::Bridge, None))
        .collect();

    format!(
        "/// Transpiled from Swift `struct {name}` (SPEC-002 Tier 1)\n\
         #[derive(Debug, Clone, uniffi::Record)]\n\
         pub struct {name} {{\n\
         {fields}}}\
         \n\n{methods}",
        name   = s.name,
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
        name  = e.name,
        cases = if cases.is_empty() { "    // (no cases)\n".to_owned() } else { cases },
    )
}

/// Emit a Rust class translation and, if `has_deinit`, a `Drop` impl.
/// Returns `(rust_source, diagnostics_from_drop_gen)`.
fn emit_class_ir(c: &SwiftClass) -> (String, Vec<crate::diagnostics::Diagnostic>) {
    let fields: String = c.fields.iter().map(|f| {
        let rust_type = f.rust_type()
            .unwrap_or_else(|| format!("() /* UNMAPPED: {} */", f.swift_type));
        format!("    pub {}: {rust_type},\n", f.name)
    }).collect();

    let deinit_note = if c.has_deinit {
        "/// NOTE: Swift `deinit` present — Drop impl generated below (SPEC-003 §6, CLASS-DEINIT)\n"
    } else { "" };

    let superclass_note = c.superclass.as_ref().map(|sc| {
        format!("/// NOTE: superclass `{sc}` — flattened to composition (CLASS-SUBCLASS)\n")
    }).unwrap_or_default();

    let methods: String = c.methods.iter()
        .map(|m| emit_func_ir(m, AsyncMode::Bridge, None))
        .collect();

    let mut src = format!(
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
        name     = &c.name,
        fields   = if fields.is_empty() { "    // (no fields)\n".to_owned() } else { fields },
        defaults = c.fields.iter()
            .map(|f| format!("{}: Default::default()", f.name))
            .collect::<Vec<_>>().join(", "),
    );

    // ── Drop impl (SPEC-003 §6) ───────────────────────────────────────────────
    let drop_diags = if c.has_deinit {
        // `parser.rs` stores the deinit body lines in `SwiftClass`.
        // For Phase 2b the body is not yet captured as separate lines;
        // we pass an empty slice so drop_gen emits the trivial stub.
        // Phase 2c will wire the actual body lines.
        let (drop_src, diags) = drop_gen::emit_drop(c, &[]);
        src.push_str(&drop_src);
        diags
    } else {
        vec![]
    };

    (src, drop_diags)
}

fn emit_func_ir(f: &SwiftFunc, async_mode: AsyncMode, async_tier: Option<&AsyncTier>) -> String {
    if f.is_async {
        emit_async_func_ir(&f.name, &f.rust_signature(), async_mode, async_tier)
    } else {
        format!(
            "/// Transpiled from Swift `func {name}` (SPEC-002 Tier 1)\n\
             #[uniffi::export]\n\
             {sig} {{\n\
             \ttodo!(\"Phase 2c: implement {name}\")\n\
             }}\n\n",
            name = f.name,
            sig  = f.rust_signature(),
        )
    }
}

/// Emit a Rust async function following SPEC-006:
///
/// - `AsyncMode::Bridge` + `A1Sync` (no await) → `spawn_blocking` (SPEC-006 §6)
/// - `AsyncMode::Bridge` + `A1`/`A2`/`None` → callback interface + `tokio::spawn`
/// - `AsyncMode::Native` → native `async fn` via `#[uniffi::export]`
fn emit_async_func_ir(
    name: &str,
    sig:  &str,
    async_mode: AsyncMode,
    async_tier: Option<&AsyncTier>,
) -> String {
    match async_mode {
        // ── Bridge mode ────────────────────────────────────────────────────────
        AsyncMode::Bridge => {
            // SPEC-006 §6: A1-sync (no await in body) → spawn_blocking
            if async_tier == Some(&AsyncTier::A1Sync) {
                return format!(
                    "/// Transpiled from Swift `async func {name}` (SPEC-006 §6, Tier A1-sync)\n\
                     /// Body has no await — spawn_blocking keeps Core logic in sync Rust.\n\
                     #[uniffi::export(callback_interface)]\n\
                     pub trait {name}Callback: Send + Sync {{\n\
                     \tfn on_result(&self, result: ());  // TODO: typed result\n\
                     \tfn on_error(&self, error: String);\n\
                     }}\n\n\
                     #[uniffi::export]\n\
                     pub fn {name}(callback: Arc<dyn {name}Callback>) {{\n\
                     \ttokio::spawn(async move {{\n\
                     \t\tlet result = tokio::task::spawn_blocking(move || {{\n\
                     \t\t\t// TODO: implement {name} (Phase 2c) — pure sync body\n\
                     \t\t}}).await;\n\
                     \t\tmatch result {{\n\
                     \t\t\tOk(v)  => callback.on_result(v),\n\
                     \t\t\tErr(e) => callback.on_error(format!(\"{{e}}\")),\n\
                     \t\t}}\n\
                     \t}});\n\
                     }}\n\n",
                    name = name,
                );
            }

            // A1 / A2 / unknown async → callback interface + tokio::spawn
            format!(
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
            )
        }

        // ── Native mode (Stage 2 — SPEC-006 §9) ────────────────────────────────
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
// These shims drive the old `sarah classify`-only path. They do not
// use the SwiftFile IR and produce stub-only output. Kept for
// backward compatibility with any tooling that calls `transpile()`.

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
            if decl.async_tier.is_some() {
                emit_async_func_ir(
                    &decl.name,
                    &format!("pub fn {}()", decl.name),
                    async_mode,
                    decl.async_tier.as_ref(),
                )
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

// ── Preamble ───────────────────────────────────────────────────────────────────────────────

fn preamble(source_file: &str) -> String {
    format!(
        "// Generated by sarah-cli — DO NOT EDIT\n\
         // Source: {source_file}\n\
         // Phase 2b output — IR-aware Tier 1 / Tier 2 lowering\n\
         // See: SPEC-002, SPEC-003, SPEC-004, SPEC-006\n\n\
         use std::sync::{{Arc, Mutex}};\n\n"
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────────────

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
    fn class_with_deinit_emits_drop_impl() {
        let src = r#"class Resource {
    var handle: Int
    deinit {}
}"#;
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("impl Drop for ResourceInner"));
        assert!(out.contains("fn drop(&mut self)"));
    }

    #[test]
    fn async_func_a1_emits_tokio_spawn() {
        let src = "async func loadData() { let _ = await fetch() }";
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("callback_interface"));
        assert!(out.contains("tokio::spawn"));
        assert!(!out.contains("spawn_blocking"));
    }

    #[test]
    fn async_func_a1sync_emits_spawn_blocking() {
        // No `await` in body — classifier assigns A1Sync — must use spawn_blocking
        let src = "async func compute() -> Int { return 42 }";
        let ir  = parse(src);
        let cr  = classify_file(&path(), src);
        let out = transpile_with_ir(&cr, &ir, AsyncMode::Bridge).unwrap();
        assert!(out.contains("spawn_blocking"), "expected spawn_blocking in:\n{out}");
    }
}
