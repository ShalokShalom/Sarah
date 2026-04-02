# SPEC-002: Tiered Lowering (Progressive Migration Engine)

| Field | Value |
|-------|-------|
| Status | Accepted |
| Author | Architecture Team |
| Date | 2026-04-02 |
| Bounded Context | Transpiler Context |
| Parent RFC | RFC-001 |
| Related Specs | SPEC-001, SPEC-003, SPEC-004 |

---

## Problem Statement

Given a Swift file classified as `Core` by SPEC-001, the transpiler must produce valid, idiomatic Rust code. Swift's type system includes both value types (`struct`, `enum`) and reference types (`class`), each requiring a different translation strategy. A single flat translation approach produces either overly restrictive output (reject all `class`) or unsafe/non-idiomatic output (naively translate `class` to Rust `struct`).

---

## Goals

1. Translate Tier 1 constructs (value types) to idiomatic Rust with `#[derive]` annotations.
2. Translate Tier 2 constructs (reference types) to `Arc<Mutex<T>>`-wrapped Rust objects exposed via UniFFI.
3. Emit diagnostics for constructs that cannot be automatically lowered, guiding developers toward manual migration.
4. Produce Rust output that compiles and passes `cargo test` including embedded doc-tests.

---

## Tier Definitions

### Tier 1 — Idiomatic Value Types

**Trigger:** Swift `struct` or `enum` with:
- No `mutating` methods that capture `self` by reference in a closure.
- No `class`-typed stored properties.
- No `@escaping` closure properties that outlive `self`.

**Translation Rules:**

| Swift | Rust |
|-------|------|
| `struct Foo { var x: Int }` | `#[derive(Debug, Clone, uniffi::Record)] pub struct Foo { pub x: i64 }` |
| `enum Color { case red, green, blue }` | `#[derive(Debug, Clone, uniffi::Enum)] pub enum Color { Red, Green, Blue }` |
| `enum Result<T> { case ok(T), failure(String) }` | `#[derive(Debug, Clone, uniffi::Enum)] pub enum Result { Ok { value: T }, Failure { message: String } }` |
| `func add(_ a: Int, _ b: Int) -> Int` | `pub fn add(a: i64, b: i64) -> i64` |
| `let x: Int` | `pub x: i64` |
| `let s: String` | `pub s: String` |
| `let opt: Int?` | `pub opt: Option<i64>` |

### Tier 2 — Reference Types (Class Compatibility)

**Trigger:** Swift `class` declaration (with or without inheritance).

**Translation Rules:** See SPEC-003 for full class-compatibility rules.

**Summary:**
- Swift `class Foo` → Rust `#[derive(uniffi::Object)] pub struct Foo { ... }`
- Mutable stored properties → wrapped in `std::sync::Mutex<T>`
- Read-heavy properties → wrapped in `std::sync::RwLock<T>`
- `deinit` → `Drop` impl
- `init(...)` → `#[uniffi::constructor] pub fn new(...) -> Arc<Self>`

### Tier 3 — Shell (Pass-Through)

Files classified as Shell by SPEC-001 are not lowered. The tiered lowering pipeline ignores them and passes them to SPEC-004 for boundary generation.

---

## Type Mapping Table

| Swift Type | Rust Type | UniFFI Wire Type |
|-----------|-----------|------------------|
| `Int` | `i64` | `i64` |
| `Int32` | `i32` | `i32` |
| `UInt8` | `u8` | `u8` |
| `Float` | `f32` | `f32` |
| `Double` | `f64` | `f64` |
| `Bool` | `bool` | `bool` |
| `String` | `String` | `string` |
| `[T]` | `Vec<T>` | `sequence<T>` |
| `[K: V]` | `HashMap<K, V>` | `record<K, V>` |
| `T?` | `Option<T>` | `optional<T>` |
| `Result<T, E>` | `Result<T, E>` | `[Throws=E]` |
| `class T` | `Arc<T>` | `object T` |

---

## Diagnostics

The lowering pipeline emits structured diagnostics when it cannot automatically translate a construct:

```json
{
  "level": "warning",
  "code": "T2-INHERITANCE",
  "message": "class 'ViewModel' inherits from 'BaseViewModel'. Inheritance is not supported in Tier 2. Translate to trait composition manually.",
  "file": "Sources/Auth/LoginViewModel.swift",
  "line": 4
}
```

Diagnostic codes:

| Code | Meaning |
|------|---------|
| `T1-CLOSURE` | Escaping closure in struct; may require manual Tier 2 migration |
| `T2-INHERITANCE` | Class inheritance detected; requires manual trait composition |
| `T2-OBJC` | `@objc` attribute; not migratable automatically |
| `T2-DEINIT` | `deinit` with side effects; review `Drop` impl |
| `T3-MANUAL` | Entire file left in shell; no Rust output generated |

---

## Acceptance Criteria

| # | Input | Expected Output |
|---|-------|-----------------|
| 1 | `struct Point { var x: Double }` | Tier 1 Rust struct |
| 2 | `enum Direction { case north, south }` | Tier 1 Rust enum |
| 3 | `class Counter { var count = 0 }` | Tier 2 `Arc<Mutex<i64>>` wrapper |
| 4 | `class A: B { }` | Tier 2 with `T2-INHERITANCE` warning |
| 5 | File with `import SwiftUI` | Tier 3, no Rust output |

---

## References

- RFC-001: Progressive Migration Engine
- SPEC-001: Core/Shell Classification
- SPEC-003: Class Compatibility (Tier 2)
- SPEC-004: UniFFI Boundary Generation
