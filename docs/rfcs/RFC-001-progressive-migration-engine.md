# RFC-001: Progressive Migration Engine

| Field | Value |
|-------|-------|
| Status | Accepted |
| Author | Architecture Team |
| Date | 2026-04-02 |
| Bounded Context | Transpiler Context |
| Replaces | (none — initial RFC) |
| Related Specs | SPEC-001, SPEC-002, SPEC-003, SPEC-004 |
| Related ADRs | ADR-005 |

---

## Summary

Replace the originally proposed strict Swift-R subset model with a **tiered lowering** approach. Instead of requiring Swift source to conform to an artificially restricted subset, the transpiler classifies each Swift file and construct into one of three tiers and handles each tier with the most appropriate translation strategy.

---

## Motivation

A strict-subset approach (accepting only value types, no classes, no reference semantics) makes the transpiler easy to reason about but impossible to apply to any real-world Swift codebase. Real Swift projects use:

- `class` types with shared mutable state.
- Protocol conformances with associated types.
- Closures capturing `self`.
- Platform frameworks (UIKit, SwiftUI) woven throughout business logic.

Refusing to handle these patterns means the transpiler is only useful for greenfield toy projects, not for the "brownfield migration" goal that is central to ROADMAP-001.

The tiered lowering approach accepts the full complexity of real Swift code and translates it at the level of fidelity that is appropriate for each construct, flagging what cannot be automatically migrated and guiding developers toward idiomatic Rust manually.

---

## Detailed Design

### Three Tiers

| Tier | Swift Pattern | Rust Translation | Automation |
|------|---------------|------------------|------------|
| **Tier 1 — Idiomatic** | `struct`, `enum`, pure `func` with no reference semantics | Idiomatic Rust `struct`/`enum`/`fn` | Fully automated |
| **Tier 2 — Compatibility** | `class` with ARC, stored/computed properties, simple inheritance | `Arc<Mutex<T>>` or `Arc<RwLock<T>>` wrapper | Automated with lint warnings |
| **Tier 3 — Shell** | Any file importing UIKit / SwiftUI / AppKit etc. | Stays in Swift; generates UniFFI call stubs | FFI boundary generated, logic stays in Swift |

### Classification Pipeline

```
Swift Source File
      │
      ▼
┌─────────────────────────────┐
│  SPEC-001: Import Analyzer  │──── imports SwiftUI/UIKit? ──▶ Tier 3 (Shell)
└────────────┬────────────────┘
             │ no platform import
             ▼
┌─────────────────────────────┐
│  SPEC-002: Type Classifier  │──── contains `class`? ────────▶ Tier 2
└────────────┬────────────────┘
             │ only value types
             ▼
             Tier 1 (Idiomatic)
```

### Tier 1 Lowering

Swift `struct` and `enum` with value semantics map directly:

```swift
// Swift (Tier 1)
struct Point { var x: Double; var y: Double }
```

```rust
// Generated Rust
#[derive(Debug, Clone, uniffi::Record)]
pub struct Point { pub x: f64, pub y: f64 }
```

### Tier 2 Lowering

Swift `class` with reference semantics maps to a `Mutex`-wrapped struct exposed as a UniFFI `Object`:

```swift
// Swift (Tier 2)
class Counter { private var value: Int = 0 }
```

```rust
// Generated Rust
#[derive(uniffi::Object)]
pub struct Counter { value: std::sync::Mutex<i64> }
```

### Tier 3 (Shell Boundary)

Shell files are **not** transpiled. Instead, the compiler emits UniFFI call stubs so the Swift shell can reach Core functions. See SPEC-004 for the boundary generation contract.

---

## Alternatives Considered

| Option | Reason Rejected |
|--------|-----------------|
| Strict subset (Tier 1 only) | Unusable on real codebases; see Motivation |
| Full ARC emulation in Rust | Produces unsafe, non-idiomatic Rust; defeats the purpose |
| Direct `unsafe` FFI without UniFFI | Fragile, not maintainable, no type safety across boundary |

---

## Open Questions

- How to handle Swift protocols with associated types in Tier 1? (Tracked in future SPEC-005.)
- Async/await bridging strategy across the UniFFI boundary. (Tracked in future SPEC-006.)

---

## References

- [UniFFI Book](https://mozilla.github.io/uniffi-rs/)
- [Crux — Cross-platform app framework in Rust](https://redbadger.github.io/crux/)
- ADR-005: Tiered Lowering vs Strict Subset
