# SPEC-005 — Diagnostic System

**Status:** Accepted  
**Version:** 1.0.0  
**Date:** 2026-04-02  
**Authors:** Sarah Project  
**Related:** SPEC-001, SPEC-002, SPEC-003, SPEC-006

---

## 1. Purpose

Define a uniform, structured diagnostic format for all warnings, errors, and informational messages emitted by the Sarah transpiler. Every message produced by any SPEC must conform to this format so that:

- Diagnostics are machine-parseable by CI pipelines and editor integrations.
- Developers receive actionable guidance, not just error codes.
- Severity levels are consistent across all classification, lowering, and code-generation passes.

---

## 2. Severity Levels

| Level | Symbol | Meaning |
|-------|--------|--------|
| `ERROR` | `✗` | Transpilation cannot proceed for this declaration. Output is not generated. |
| `WARN` | `⚠` | Transpilation proceeds with a mitigation applied. Developer review required. |
| `INFO` | `ℹ` | Informational. No action required; documents a non-obvious decision made by the transpiler. |
| `HINT` | `→` | Suggested refactor. Not blocking; included when the transpiler can propose a better source pattern. |

---

## 3. Diagnostic Message Format

Every diagnostic is emitted as a structured record in JSON (machine) and as a human-readable block (terminal/editor).

### 3.1 JSON Schema

```json
{
  "code":     "ASYNC-LOCK-RISK",
  "level":    "WARN",
  "message":  "Async function on a class receiver; lock held across await point.",
  "file":     "Sources/SessionManager.swift",
  "line":     42,
  "column":   5,
  "span":     "func refresh() async",
  "hint":     "Extract async logic into a free function. Acquire lock, clone state, release lock, then await.",
  "see":      "SPEC-006 §4, SPEC-003 §5"
}
```

### 3.2 Terminal Format

```
⚠  ASYNC-LOCK-RISK
   Sources/SessionManager.swift:42:5
   Async function on a class receiver; lock held across await point.
   → Extract async logic into a free function.
     Acquire lock, clone state, release lock, then await.
   See: SPEC-006 §4, SPEC-003 §5
```

---

## 4. Diagnostic Code Registry

### 4.1 Classification Diagnostics (SPEC-001)

| Code | Level | Description |
|------|-------|-------------|
| `T2-CLASS` | INFO | `class` declaration translated to `Arc<Mutex<T>>` |
| `T2-INHERITANCE` | WARN | Class inheritance detected; struct refactor suggested |
| `T3-PAT` | ERROR | Protocol with associated types; not transpilable; Shell only |
| `T3-OBJC` | ERROR | ObjC interop present; not transpilable; Shell only |
| `T1-CLOSURE` | WARN | Closure captures non-value type; ownership must be reviewed |

### 4.2 Async Diagnostics (SPEC-001 §3.2, SPEC-006 §4)

| Code | Level | Description |
|------|-------|-------------|
| `ASYNC-LOCK-RISK` | WARN | `async func` on a class receiver; lock-before-await pattern applied by codegen |
| `ASYNC-NO-AWAIT` | INFO | `async func` body contains no `await`; `spawn_blocking` used instead of `async fn` |
| `ASYNC-CANCEL-MISSING` | HINT | Long-running async func has no `@cancellable` annotation; consider adding it |

### 4.3 Lowering Diagnostics (SPEC-002)

| Code | Level | Description |
|------|-------|-------------|
| `LOWER-UNSUPPORTED` | ERROR | Swift construct has no Rust equivalent at this tier |
| `LOWER-PROTOCOL-CONFORMANCE` | WARN | Protocol conformance partially lowered; verify generated trait impl |
| `LOWER-GENERIC-CONSTRAINT` | WARN | Generic constraint lowered with loss of expressiveness; review output |
| `LOWER-OPTIONAL-BOXED` | INFO | `Optional<T>` lowered to `Option<Box<T>>` due to recursive type |

### 4.4 Class Compatibility Diagnostics (SPEC-003)

| Code | Level | Description |
|------|-------|-------------|
| `CLASS-DEINIT` | WARN | `deinit` present; no direct Rust equivalent; `Drop` impl generated |
| `CLASS-COPY-SEMANTIC` | ERROR | Value-copying semantics used on a class type; not safe to lower |
| `CLASS-SUBCLASS` | WARN | Subclass detected; flattening to composition; review generated struct |

### 4.5 UniFFI Boundary Diagnostics (SPEC-004)

| Code | Level | Description |
|------|-------|-------------|
| `FFI-TYPE-UNSUPPORTED` | ERROR | Type is not representable in UniFFI; use a wrapper type |
| `FFI-CALLBACK-UNBOUNDED` | WARN | Callback interface has no clear lifetime; Arc wrapping applied |
| `FFI-DUPLICATE-EXPORT` | ERROR | Two declarations with the same UniFFI export name detected |

---

## 5. Diagnostic Aggregation and Exit Codes

The transpiler exit code reflects the worst diagnostic level encountered:

| Exit code | Meaning |
|-----------|---------|
| `0` | All declarations lowered successfully; no ERRORs or WARNs |
| `1` | One or more `WARN` diagnostics emitted; output generated with mitigations |
| `2` | One or more `ERROR` diagnostics emitted; partial output only |
| `3` | Fatal internal error; no output |

---

## 6. Diagnostic Output Targets

| Flag | Output |
|------|--------|
| *(default)* | Terminal human-readable format |
| `--diagnostics=json` | Machine-readable JSON array to stdout |
| `--diagnostics=sarif` | SARIF 2.1 format for GitHub Advanced Security / editor integrations |

---

## 7. Invariants

1. Every diagnostic must have a `code`, `level`, `message`, `file`, and `line`.
2. Every `WARN` must include a `hint` field with a concrete corrective action.
3. Every `ERROR` must include a `see` reference to the governing SPEC section.
4. No diagnostic may be silenced at a lower severity than its registered level without an explicit `--allow=<CODE>` flag.
5. The transpiler must never produce an `ERROR` silently — all errors must surface to the exit code.
