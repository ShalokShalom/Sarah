# Contributing to Sarah

Welcome. Sarah is a Swift-to-Rust transpiler and the contribution bar is high: every change must be architecturally sound, documented, and additive.

---

## Before You Start

1. Read `docs/00_ONBOARDING_THE_CAUSE.md` — this is the entry point.
2. Read the relevant SPECs for the area you are working in.
3. Open an issue or RFC before writing significant code. Architecture decisions go through ADRs.

---

## Repository Layout

```
Sarah/
├── core/               Rust crate — UniFFI scaffolding, Core logic
│   ├── src/
│   ├── Cargo.toml
│   ├── build.rs
│   └── uniffi.toml
├── shell/              Swift package — SwiftUI Shell
│   ├── Package.swift
│   └── Sources/Sarah/
├── docs/
│   ├── 00_ONBOARDING_THE_CAUSE.md
│   ├── ASYNC-STRATEGY-EVALUATION.md
│   ├── adrs/           Architecture Decision Records
│   ├── rfcs/           Request for Comments
│   └── specs/          Formal specifications
├── ROADMAP.md
├── CONTRIBUTING.md     (this file)
├── README.md
└── LICENSE
```

---

## Contribution Types

### Documentation

- All SPECs are versioned (`Version:` front-matter field). Bump the minor version for additions, major for breaking changes.
- All ADRs are immutable once `Status: Accepted`. To reverse a decision, write a new ADR that supersedes the old one.
- RFCs are drafts until promoted to a SPEC or ADR.

### Rust Core (`core/`)

- All public functions exported via UniFFI must have doc comments.
- Follow the tier rules from SPEC-002. Do not add `async fn` to Tier 1 code; use `spawn_blocking` (SPEC-006 §5).
- Run `cargo clippy --all-targets -- -D warnings` before opening a PR.
- Run `cargo test` to verify all doc-tests pass.

### Swift Shell (`shell/`)

- All generated or hand-written ViewModels must conform to `ObservableObject`.
- No SwiftUI view may call `CoreFFI` directly — all Core access goes through a ViewModel.
- Run `swift build` and `swift test` before opening a PR.
- Enable `-strict-concurrency=complete` in `Package.swift` and resolve all warnings.

### Transpiler Engine (Phase 2+)

- Every new diagnostic code must be registered in SPEC-005 §4 before shipping.
- All code generation must be tested with a round-trip test: Swift source → generated Rust → `cargo build` succeeds.
- New async codegen paths must pass the three-zone invariants in SPEC-006 §8.

---

## Pull Request Checklist

- [ ] No existing file removed or overwritten without discussion.
- [ ] SPEC / ADR updated or created if the change affects architecture.
- [ ] New diagnostic codes registered in SPEC-005.
- [ ] `cargo clippy` clean (Rust changes).
- [ ] `swift build` clean (Swift changes).
- [ ] PR description explains *why*, not just *what*.

---

## Commit Style

```
<type>(<scope>): <short imperative summary>

Types: feat, fix, docs, test, refactor, chore
Scopes: core, shell, docs, specs, adrs, rfcs, transpiler

Examples:
  feat(core): add CancellationToken for async FFI bridge
  docs(specs): add SPEC-005 diagnostic system
  fix(shell): resolve strict-concurrency warning in CounterViewModel
```

---

## Questions

Open a GitHub Discussion or file an issue tagged `question`. Architecture questions that reveal a missing decision should be escalated to an RFC.
