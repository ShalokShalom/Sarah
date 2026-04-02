# ADR-005: Tiered Lowering vs Strict Subset

| Field | Value |
|-------|-------|
| Status | Accepted |
| Author | Architecture Team |
| Date | 2026-04-02 |
| Bounded Context | Transpiler Context |

---

## Context

The transpiler needs a strategy for deciding which Swift constructs it will accept and translate to Rust. Two approaches were proposed:

**Option A — Strict Subset:** Define a restricted Swift dialect ("Swift-R") that excludes `class`, reference semantics, closures with captures, and Apple frameworks. The transpiler only processes code written in this subset.

**Option B — Tiered Lowering:** Accept all Swift constructs, classify them by translation difficulty, and handle each tier with the appropriate strategy — full automation, partial automation with warnings, or pass-through to shell.

---

## Decision

**We adopt Option B — Tiered Lowering.**

---

## Rationale

### Why Strict Subset Fails

1. **No real-world applicability.** Every existing Swift codebase uses `class`, ARC, and Combine. A transpiler that rejects these constructs cannot migrate any existing code — the core goal of ROADMAP-001.
2. **Greenfield only.** Strict subset effectively requires developers to write new code in a constrained dialect. This increases friction and reduces adoption.
3. **Fake safety.** Restricting inputs does not guarantee correct output; it merely avoids hard cases by refusing to handle them.

### Why Tiered Lowering Wins

1. **Real-world coverage.** Tier 1 handles the easy cases cleanly. Tier 2 handles the common `class` pattern safely via `Arc<Mutex<T>>`. Tier 3 gracefully handles the genuinely un-migratable shell code.
2. **Progressive.** Teams can migrate incrementally. A file that today is Tier 2 can be manually refactored to Tier 1 over time as the team becomes more comfortable with Rust ownership.
3. **Honest diagnostics.** Rather than silently refusing to compile a file, tiered lowering emits structured diagnostics explaining exactly why a construct requires manual attention.
4. **Aligns with ROADMAP-001.** The roadmap explicitly embraces hybrid states as first-class citizens. Tiered lowering operationalizes this principle.

---

## Consequences

### Positive

- Transpiler is applicable to real Swift projects from day one.
- Teams get actionable migration guidance rather than flat rejections.
- Phase 1 canonical example can use a real counter `class` (Tier 2) alongside value types (Tier 1).

### Negative

- The transpiler is more complex to implement. Three translation pipelines instead of one.
- Tier 2 output (`Arc<Mutex<T>>`) is not always the most idiomatic Rust. Developers should refactor Tier 2 output toward Tier 1 patterns over time.
- Tiered lowering could give teams false confidence that generated Rust code is production-ready without review.

### Mitigations

- All Tier 2 output is annotated with `// TIER-2: review for idiomatic refactor` comments.
- The linter (future SPEC) will flag Tier 2 functions as migration targets.
- Documentation will emphasize that generated code is a starting point, not a final product.

---

## References

- RFC-001: Progressive Migration Engine
- SPEC-001: Core/Shell Classification
- SPEC-002: Tiered Lowering
