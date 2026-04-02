# Sarah — Roadmap

> This roadmap reflects the current architectural baseline. Phases are sequential; each phase gates the next.

---

## Phase 1 — Foundation *(complete)*

Establish the project structure, documentation, and a working end-to-end skeleton.

- [x] DDD document set (SPEC-001 through SPEC-006, ADR-005, ADR-006, RFC-001)
- [x] Rust `core/` crate with UniFFI scaffolding (`Counter` example object)
- [x] Swift `shell/` package with SwiftUI ViewModel and ContentView
- [x] Async strategy defined and documented (three-zone hybrid)
- [x] Diagnostic system specified (SPEC-005)
- [x] Onboarding guide and contributing guidelines

---

## Phase 2 — Transpiler Engine *(next)*

Implement the actual Swift → Rust transpilation pipeline.

### 2a — Swift AST Parser

- [ ] Integrate `swift-syntax` (or equivalent) as the Swift source parser
- [ ] Emit raw AST JSON for a given Swift file
- [ ] Wire into SPEC-001 classifier; emit tier JSON per declaration

### 2b — Tier 1 Code Generator

- [ ] Implement SPEC-002 lowering for `struct`, `enum`, pure `func`
- [ ] Map Swift value types to Rust equivalents (SPEC-002 type mapping table)
- [ ] Generate `#[uniffi::export]` annotations (SPEC-004)
- [ ] Emit and validate `Cargo.toml` stubs

### 2c — Tier 2 Code Generator

- [ ] Implement SPEC-003 class → `Arc<Mutex<T>>` translation
- [ ] Generate `Drop` impls for `deinit`
- [ ] Handle inheritance flattening (emit `CLASS-SUBCLASS` diagnostic)

### 2d — Async Shell Generator

- [ ] Implement SPEC-006 three-zone async model in codegen
- [ ] Generate `withCheckedThrowingContinuation` wrappers (Tier A1)
- [ ] Generate `spawn_blocking` paths (Tier A1-sync)
- [ ] Generate `withTaskCancellationHandler` + `CancellationToken` (for `@cancellable`)
- [ ] Emit `ASYNC-LOCK-RISK` diagnostic and lock-before-await mitigation (Tier A2)

### 2e — Diagnostic Engine

- [ ] Implement SPEC-005 diagnostic aggregation
- [ ] Terminal and JSON output modes
- [ ] SARIF output for CI / editor integration
- [ ] Exit code protocol

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
