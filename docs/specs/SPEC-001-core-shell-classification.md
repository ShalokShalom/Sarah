# SPEC-001 — Core/Shell Classification

**Status:** Accepted  
**Version:** 1.1.0  
**Date:** 2026-04-02  
**Authors:** Sarah Project  
**Changelog:** v1.1 — Added Async Tier classification (A1, A1-sync, A2); see SPEC-006 for full async strategy.

---

## 1. Purpose

Define the algorithm by which Sarah classifies Swift source files and declarations as belonging to the **Core** layer (transpilable to Rust) or the **Shell** layer (remains Swift, wraps Core), and the tier assigned to each Core declaration.

---

## 2. Tier Taxonomy

| Tier | Description | Key constructs |
|------|-------------|---------------|
| **1** | Golden-core safe Rust | `struct`, `enum`, pure `func`, value types |
| **2** | Shared-ownership | `class` → `Arc<Mutex<T>>` |
| **3** | Manual / unsupported | Protocols with PATs, existentials, ObjC interop |
| **A1** | Async value function | `async func` (no class receiver) → callback + Tokio spawn |
| **A1-sync** | Async wrapper over sync body | `async func` with no internal `await` → `spawn_blocking` |
| **A2** | Async on class receiver | `async func` on `class` → `ASYNC-LOCK-RISK` diagnostic |

Async tiers are orthogonal to synchronous tiers: a Tier 1 struct may have Tier A1 methods. The combined tier is written as e.g. `1/A1`.

---

## 3. Classification Algorithm

### 3.1 File-level

1. Parse the Swift file into an AST.
2. For each top-level declaration, apply declaration-level classification (§3.2).
3. If **all** declarations are Tier 1 or A1/A1-sync → file is **Core**.
4. If **any** declaration is Tier 2 → file is **Core (Tier 2 present)**; emit `T2-` diagnostics.
5. If **any** declaration is Tier 3 → file is **Shell**; emit `T3-` diagnostics.
6. If **any** declaration is A2 → emit `ASYNC-LOCK-RISK`; generation continues with mitigated pattern (see SPEC-006 §4).

### 3.2 Declaration-level

```
classify(decl):
  if decl is struct or enum with value-type stored properties:
    → Tier 1
  if decl is class:
    → Tier 2  (emit T2-CLASS)
  if decl is protocol with associated types:
    → Tier 3  (emit T3-PAT)
  if decl is func:
    if async and receiver is class:
      → Tier A2  (emit ASYNC-LOCK-RISK)
    if async and body contains no await:
      → Tier A1-sync
    if async:
      → Tier A1
    else:
      → Tier 1
  if decl references ObjC / @objc / NSObject:
    → Tier 3  (emit T3-OBJC)
```

---

## 4. JSON Output Schema

```json
{
  "file": "Sources/UserService.swift",
  "file_tier": "Core",
  "declarations": [
    {
      "name": "UserService",
      "kind": "struct",
      "tier": "1",
      "async_tier": null,
      "diagnostics": []
    },
    {
      "name": "fetchUser",
      "kind": "func",
      "tier": "1",
      "async_tier": "A1",
      "combined_tier": "1/A1",
      "diagnostics": []
    },
    {
      "name": "SessionManager",
      "kind": "class",
      "tier": "2",
      "async_tier": null,
      "diagnostics": ["T2-CLASS"]
    },
    {
      "name": "refresh",
      "kind": "func",
      "tier": "2",
      "async_tier": "A2",
      "combined_tier": "2/A2",
      "diagnostics": ["ASYNC-LOCK-RISK"]
    }
  ]
}
```

---

## 5. Diagnostic Codes

| Code | Tier | Meaning |
|------|------|---------|
| `T1-CLOSURE` | 1 | Closure captures non-value type; review ownership |
| `T2-CLASS` | 2 | `class` declaration → `Arc<Mutex<T>>` in output |
| `T2-INHERITANCE` | 2 | Class inheritance present; Tier 2 with struct refactor suggestion |
| `T3-PAT` | 3 | Protocol with associated types; Shell only |
| `T3-OBJC` | 3 | ObjC interop; Shell only |
| `ASYNC-LOCK-RISK` | A2 | Async func on class receiver; lock-before-await pattern applied |

---

## 6. References

- SPEC-002 — Tiered Lowering
- SPEC-003 — Class Compatibility
- SPEC-006 — Async Bridging Strategy
- ADR-005 — Tiered Lowering vs Strict Subset
- ADR-006 — Three-Zone Async Boundary Model
