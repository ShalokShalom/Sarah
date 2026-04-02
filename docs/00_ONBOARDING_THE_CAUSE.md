# 00 — Onboarding the Cause

## The Mission

We are building the off-ramp from legacy Swift to a Rust-powered future.

**Move business logic from Swift to Rust. Keep Swift as a thin platform shell.**

This is not a rewrite. It is a progressive, spec-driven migration. We do not ask teams to throw away their Swift code overnight. We give them a path — a transpiler, a pattern, and a community — that makes Rust the natural evolution of serious Apple-platform development.

## Why This Matters

- **Safety without GC.** Rust's ownership model eliminates entire classes of bugs (data races, use-after-free) that Swift's ARC only partially mitigates.
- **Cross-platform by default.** A Rust core shared between iOS (Swift shell) and Android (Kotlin shell) eliminates duplication of business logic.
- **Performance.** No runtime overhead, no garbage collection pauses, zero-cost abstractions.
- **Correctness.** The type system catches invariant violations at compile time that Swift leaves to runtime.

## The Architecture

```
Swift Shell  →  UniFFI Bridge  →  Rust Core
(UI, OS APIs)     (generated)     (all logic)
```

## Your First Contribution

1. Read this file (done ✓).
2. Read `../README.md` and `rfcs/RFC-001-progressive-migration-engine.md`.
3. Read `specs/SPEC-001-core-shell-classification.md`.
4. Pick a SPEC to implement or extend.
5. Write the code, making sure all doc-tests pass (`cargo test`).

## Glossary

| Term | Meaning |
|------|--------|
| Core | A Rust crate containing business logic, accessible via UniFFI |
| Shell | A Swift file/module that only handles UI and OS integration |
| Tier 1 | Value-type Swift code that maps cleanly to idiomatic Rust |
| Tier 2 | Reference-type Swift code mapped to `Arc<Mutex<T>>` in Rust |
| Tier 3 | UI-coupled Swift code that stays in the shell and calls Core via UniFFI |
| UDF | Unidirectional Data Flow — Action → State → Effect pattern |

## Principles

- **Docs first.** No SPEC, no code.
- **Doc-tests are contracts.** If the example in a doc comment breaks, the contract is broken.
- **Shell stays thin.** If Swift logic can move to Rust, it should.
- **No big-bang rewrites.** Hybrid states are fine. Move incrementally.
