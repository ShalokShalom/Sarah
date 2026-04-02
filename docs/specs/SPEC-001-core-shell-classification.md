# SPEC-001: Core vs Shell Classification

| Field | Value |
|-------|-------|
| Status | Accepted |
| Author | Architecture Team |
| Date | 2026-04-02 |
| Bounded Context | Transpiler Context |
| Parent RFC | RFC-001 |
| Related ADRs | ADR-005 |

---

## Problem Statement

The transpiler must decide, for every Swift source file, whether it belongs to the **Core** (business logic, safe to migrate to Rust) or to the **Shell** (platform/UI-coupled, must stay in Swift). This decision drives the entire downstream pipeline: Core files enter the tiered lowering pipeline; Shell files receive UniFFI call-site generation.

An incorrect classification—treating a Shell file as Core—will produce Rust code that references Apple-framework types, which will not compile. The inverse mistake—treating a Core file as Shell—will leave migratable business logic in Swift unnecessarily.

---

## Goals

1. Detect all Apple-platform shell imports (UIKit, SwiftUI, AppKit, WatchKit, StoreKit, ARKit, etc.).
2. Mark each Swift source file as `Core` or `Shell` with high precision.
3. For Shell files, identify the public API surface that Core functions call, and flag it for UniFFI stub generation (SPEC-004).
4. Produce machine-readable classification output (JSON) consumable by downstream pipeline stages.

---

## Non-Goals

- This SPEC does not define how Core files are lowered (see SPEC-002, SPEC-003).
- This SPEC does not define the UniFFI binding format (see SPEC-004).
- This SPEC does not handle Swift Package Manager dependency graphs (future SPEC).

---

## Behaviour

### Shell Import Trigger List (v1)

A file is classified as **Shell** if it contains a top-level `import` of any framework in this list:

```
SwiftUI, UIKit, AppKit, WatchKit, TVUIKit,
StoreKit, ARKit, RealityKit, SceneKit,
MapKit, CoreLocation (when used with CLLocationManagerDelegate),
Combine (when used with @Published / ObservableObject),
XCTest
```

> **Note:** `Foundation` is NOT a Shell trigger. It is allowed in Core files.  
> `Combine` used purely for `Future`/`Publisher` in business logic is a yellow flag, not an automatic Shell classification; the classifier emits a warning.

### Classification Algorithm

```
for each file F in project:
    imports = parse_top_level_imports(F)
    if imports ∩ SHELL_TRIGGER_LIST ≠ ∅:
        classify(F, Shell)
        record_public_api_surface(F)   // for SPEC-004
    else:
        classify(F, Core)
        enqueue_for_tiered_lowering(F) // for SPEC-002
```

### Output Schema

```json
{
  "file": "Sources/Auth/LoginViewModel.swift",
  "classification": "Shell",
  "shell_triggers": ["SwiftUI", "Combine"],
  "public_api_surface": [
    { "name": "submit", "params": ["username: String", "password: String"], "returns": "Void" }
  ]
}
```

```json
{
  "file": "Sources/Auth/LoginStateMachine.swift",
  "classification": "Core",
  "shell_triggers": [],
  "tier_hint": null
}
```

---

## Acceptance Criteria

| # | Scenario | Expected Classification |
|---|----------|------------------------|
| 1 | File imports `SwiftUI` | Shell |
| 2 | File imports `Foundation` only | Core |
| 3 | File imports `UIKit` and `Foundation` | Shell |
| 4 | File has zero imports | Core |
| 5 | File imports `Combine` with `ObservableObject` | Shell (with warning) |
| 6 | File imports `XCTest` | Shell |

---

## References

- RFC-001: Progressive Migration Engine
- SPEC-002: Tiered Lowering
- SPEC-004: UniFFI Boundary Generation
