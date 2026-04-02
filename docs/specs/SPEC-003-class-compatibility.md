# SPEC-003: Class Compatibility — Tier 2 Lowering Rules

| Field | Value |
|-------|-------|
| Status | Draft |
| Author | Architecture Team |
| Date | 2026-04-02 |
| Bounded Context | Transpiler Context |
| Parent RFC | RFC-001 |
| Related Specs | SPEC-002, SPEC-004 |

---

## Problem Statement

Swift `class` types carry reference semantics, shared ownership, and ARC lifecycle management. Rust has no equivalent built-in mechanism. Naively translating a Swift `class` to a Rust `struct` changes ownership semantics and breaks code that relies on shared mutable references. SPEC-003 defines the precise rules for translating Swift classes into Rust in a way that preserves reference semantics via `Arc<Mutex<T>>` while producing code that compiles, passes doc-tests, and exposes correctly through UniFFI.

---

## Goals

1. Produce safe, correct Rust for every Swift `class` pattern handled automatically (Tier 2).
2. Preserve shared ownership via `Arc`.
3. Preserve thread safety via `Mutex` (default) or `RwLock` (for read-heavy properties).
4. Emit UniFFI `#[derive(uniffi::Object)]` annotations so generated bindings work in Swift.
5. Emit clear diagnostics for patterns that require manual migration (inheritance, `@objc`, etc.).

---

## Translation Rules

### Rule 1 — Class → UniFFI Object

```swift
// Swift
class Foo { }
```

```rust
// Rust
#[derive(uniffi::Object)]
pub struct Foo { }

#[uniffi::export]
impl Foo {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self { })
    }
}
```

### Rule 2 — Stored Properties

Each stored `var` property becomes a `Mutex<T>`-wrapped field. `let` properties become plain fields (immutable after construction).

```swift
// Swift
class Counter {
    var count: Int = 0
    let id: String
}
```

```rust
// Rust
#[derive(uniffi::Object)]
pub struct Counter {
    count: std::sync::Mutex<i64>,
    pub id: String,
}
```

### Rule 3 — Computed Properties → Methods

Swift computed properties have no direct Rust equivalent. They become `get_<name>()` methods (and optionally `set_<name>()` for settable computed properties).

```swift
// Swift
var doubled: Int { count * 2 }
```

```rust
// Rust
pub fn doubled(&self) -> i64 {
    *self.count.lock().unwrap() * 2
}
```

### Rule 4 — Instance Methods

Mutating methods acquire a `Mutex` lock internally. Non-mutating methods acquire a read lock or clone the value.

```swift
// Swift
func increment() { count += 1 }
```

```rust
// Rust
pub fn increment(&self) {
    *self.count.lock().unwrap() += 1;
}
```

### Rule 5 — init → new (UniFFI Constructor)

```swift
// Swift
init(startingAt value: Int) { self.count = value }
```

```rust
// Rust
#[uniffi::constructor]
pub fn new(starting_at: i64) -> Arc<Self> {
    Arc::new(Self { count: std::sync::Mutex::new(starting_at) })
}
```

### Rule 6 — deinit → Drop

```swift
// Swift
deinit { logger.log("Counter deallocated") }
```

```rust
// Rust (manual — emitted as TODO comment in output)
impl Drop for Counter {
    fn drop(&mut self) {
        // TODO: translate deinit body
        // logger.log("Counter deallocated");
    }
}
```

---

## Unsupported Patterns (Tier 2 Limitations)

| Pattern | Reason | Guidance |
|---------|--------|----------|
| Class inheritance (`class A: B`) | Rust has no class inheritance | Refactor to trait composition; parent logic moves to a separate `B` struct |
| `@objc` / `@objc dynamic` | Objective-C bridging is Apple-platform-specific | Must stay in shell; expose via UniFFI if Core calls are needed |
| Weak references (`weak var`) | ARC weak refs → `Weak<T>` in Rust, but often indicates a design smell | Prefer unidirectional ownership; consider restructuring |
| `override` methods | Requires inheritance | Refactor using trait default methods |
| KVO / `@Published` | SwiftUI/Combine coupling | Move to shell; Rust core emits state changes via return values or effects |

---

## Read-Heavy Properties — RwLock Hint

If a property is documented as read-heavy (or the transpiler detects more reads than writes via heuristic), it may generate `RwLock<T>` instead of `Mutex<T>`:

```rust
// RwLock variant (read-heavy hint)
count: std::sync::RwLock<i64>,

// read
*self.count.read().unwrap()

// write
*self.count.write().unwrap() += 1;
```

---

## Acceptance Criteria

| # | Input | Output |
|---|-------|--------|
| 1 | `class C { var x: Int }` | `Arc<Mutex<i64>>` field, `uniffi::Object` |
| 2 | `class C { let id: String }` | Plain `String` field, no Mutex |
| 3 | `class C { var double: Int { x * 2 } }` | `get_double()` method |
| 4 | `class A: B { }` | Partial output + `T2-INHERITANCE` diagnostic |
| 5 | `init(x: Int)` | `#[uniffi::constructor] fn new(x: i64) -> Arc<Self>` |

---

## References

- RFC-001: Progressive Migration Engine
- SPEC-002: Tiered Lowering
- SPEC-004: UniFFI Boundary Generation
