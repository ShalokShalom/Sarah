# Sarah — Roadmap

> This roadmap reflects the current architectural baseline. Phases are sequential; each phase gates the next.
> Last updated: 2026-04-02 (post Phase 2 implementation audit).

---

## Phase 1 — Foundation *(complete)*

Establish the project structure, documentation, and a working end-to-end skeleton.

- [x] DDD document set (SPEC-001 through SPEC-009, ADR-005, ADR-006, ADR-006b, RFC-001)
- [x] Rust `core/` crate with UniFFI scaffolding (`Counter` example object)
- [x] Swift `shell/` package with SwiftUI ViewModel and ContentView
- [x] Async strategy defined and documented (three-zone hybrid, ADR-006)
- [x] Diagnostic system specified (SPEC-005)
- [x] Onboarding guide and contributing guidelines

---

## Phase 2 — Transpiler Engine *(largely complete — 3 gaps remain)*

Implement the actual Swift → Rust transpilation pipeline.

### 2a — Swift AST Parser + Classifier

- [x] Regex-based Swift source parser emitting `SwiftFile` IR (`parser.rs`)
- [x] `SwiftFile` IR defined and documented (SPEC-009)
- [x] `sarah parse <file> --parser regex|treesitter` — emit IR JSON
- [x] SPEC-001 classifier wired to IR; `sarah classify <file>` emits tier JSON
- [x] tree-sitter-swift dependency added (`Cargo.toml`)
- [x] `ParserBackend` enum + `parse_with_backend()` dispatcher
- [x] `--parser` flag on all subcommands (SPEC-008 §5.1)
- [x] Round-trip equivalence tests: both backends assert identical `SwiftFile` IR (`tests/round_trip.rs`)

> **Note:** `parse_with_treesitter()` body is a stub pending Phase 2c grammar
> node wiring (SPEC-008 §9 steps 2c.3–2c.4). The flag and dispatch layer are
> fully in place.

### 2b — Tier 1 Code Generator

- [x] SPEC-002 lowering for `struct`, `enum`, pure `func` (`codegen.rs`)
- [x] Swift→Rust type mapping (`types.rs`, SPEC-002 table)
- [x] `#[uniffi::export]` and `#[derive(uniffi::Record/Enum/Object)]` annotations
- [x] `sarah lower <file>` and `sarah transpile <file>` subcommands
- [ ] Emit and validate `Cargo.toml` stubs for generated crates

### 2c — Tier 2 Code Generator

- [x] SPEC-003 class → `Arc<Mutex<T>>` translation (`codegen.rs` `emit_class_ir`)
- [x] `Drop` impl emitter for `deinit` blocks (`drop_gen.rs`)
- [x] `drop_gen` wired into `codegen.rs` — `emit_class_ir` calls `emit_drop` when `has_deinit`
- [x] Inheritance flattening — `CLASS-SUBCLASS` comment + diagnostic emitted

### 2d — Async Shell Generator

- [x] SPEC-006 three-zone async model in `shell_gen.rs` and `codegen.rs`
- [x] `withCheckedThrowingContinuation` wrappers for Tier A1 (`shell_gen.rs`)
- [x] `@MainActor ObservableObject` ViewModel wrappers for class async methods
- [x] `ASYNC-LOCK-RISK` diagnostic + Tier A2 detection (`classify.rs`)
- [x] `spawn_blocking` paths for A1-sync (no-await async func) (`codegen.rs`)
- [ ] `withTaskCancellationHandler` + `CancellationToken` for `@cancellable` (SPEC-006 §7)

### 2e — Diagnostic Engine

- [x] SPEC-005 diagnostic aggregation (`diagnostics.rs`)
- [x] Terminal output with colour and severity symbols
- [x] JSON output mode (`--diagnostics json`)
- [x] Exit code protocol (0 = clean, 1 = warn, 2 = error)
- [ ] SARIF output for CI / editor integration

---

## Phase 3 — Integration and Validation

Connect the transpiler to real Swift projects and validate output quality.

- [ ] End-to-end test: transpile the `Counter` example from Swift to Rust and verify against existing `core/src/counter.rs`
- [ ] Add property-based tests for type mapping correctness
- [ ] Add regression suite for all diagnostic codes
- [ ] Validate generated UniFFI bindings compile against the Swift Shell
- [ ] Performance baseline: measure transpilation time on a 10k-line Swift codebase

---

## Phase 4 — Native Async Migration Path

Evaluate and optionally activate native UniFFI async (`--async-mode=native`).

- [ ] Monitor UniFFI upstream Sendable gap resolution (ADR-006 review trigger)
- [ ] Implement `--async-mode=native` flag in the async shell generator
- [ ] Validate generated code under `-strict-concurrency=complete`
- [ ] Run side-by-side comparison: bridge mode vs native mode output quality
- [ ] Deprecate bridge mode if native mode passes all validation gates

---

## Phase 5 — Ecosystem

- [ ] Swift Package Manager plugin for incremental transpilation
- [ ] Xcode extension for inline diagnostic display
- [ ] GitHub Actions workflow template
- [ ] Public documentation site
- [ ] First public release (v0.1.0)
