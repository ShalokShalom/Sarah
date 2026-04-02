# Async Strategy Evaluation for Sarah

**Status:** Accepted guidance  
**Date:** 2026-04-02  
**Scope:** Evaluate all practical async-bridging options for Sarah's Swift-to-Rust transpilation pipeline and rank them by suitability.

---

## Executive Ranking

| Rank | Strategy | Suitability | Short verdict |
|------|----------|------------|---------------|
| **1** | Three-zone hybrid (Tokio Core / callback bridge / generated Swift `async` façade) | **Excellent — recommended default** | Best balance of ergonomics, correctness, migration safety, and transpiler control |
| **2** | Native UniFFI async end-to-end | **Strong later-stage option** | Cleanest API surface; best after Swift concurrency rough edges settle |
| **3** | Callback-only public API | **Acceptable internally, poor externally** | Stable FFI seam but breaks Swift structured concurrency |
| **4** | Job handle / polling model | **Weak fit** | Useful for constrained runtimes; unnatural for SwiftUI flows |
| **5** | Blocking / synchronous façade | **Avoid** | Fights both Swift and Rust concurrency models |

---

## Option 1 — Three-Zone Hybrid *(Recommended Default)*

### Shape

```
┌─────────────────────────────────────────────────────┐
│  SwiftUI Layer  (hand-written)                      │
│  await viewModel.loadUser(id: id)                   │  ← native Swift async/await
└───────────────────────┬─────────────────────────────┘
                        │  async func  (Sarah-generated)
┌───────────────────────▼─────────────────────────────┐
│  Shell ViewModel  (Sarah-generated)                 │
│  func loadUser(id: String) async throws -> User {   │
│    try await withCheckedThrowingContinuation { k in │
│      CoreFFI.loadUser(id: id, callback:             │
│        ResultCallback { r in k.resume(with: r) })   │
│    }                                                │
│  }                                                  │
└───────────────────────┬─────────────────────────────┘
                        │  UniFFI callback interface  (stable)
┌───────────────────────▼─────────────────────────────┐
│  Rust Core  (Sarah-transpiled)                      │
│  pub fn load_user(id: String,                       │
│                   cb: Box<dyn FetchCallback>) {     │
│      tokio::spawn(async move {                      │
│          let r = do_load_user(id).await;            │
│          cb.on_result(r);                           │
│      });                                            │
│  }                                                  │
└─────────────────────────────────────────────────────┘
```

### Strengths

- Preserves a **native Swift async surface** for all SwiftUI consumers.
- **No Sendable leakage**: the FFI boundary is synchronous; UniFFI's current Sendable gap in native async bindings never surfaces.
- **Rust stays Rust**: Tokio is fully inside the Core. Swift never touches the executor.
- **Transpiler owns the seam**: cancellation wrappers, error mapping, and lock discipline are all generated in one place and never hand-maintained.
- **Tier 1 code stays golden**: `spawn_blocking` keeps sync-body Rust free of `async`, preserving compatibility with formal verification tools.
- **Migration path is one layer**: if native UniFFI async later becomes preferable, only the generated Shell bridge changes. Rust Core and SwiftUI are untouched.

### Downsides

- **Generated bridge is more complex** than a direct call path — `withCheckedThrowingContinuation`, result enums, and callback type definitions all appear in Shell output.
- **Two concurrency mental models** for Sarah contributors: Tokio on the Rust side, Swift structured concurrency on the Swift side, plus the seam between them.
- **Cancellation is not free**: Sarah must generate and own `withTaskCancellationHandler` + Rust `CancellationToken` pairs. It is automatable but not automatic.
- **Tier A2 risk still exists**: async methods on Tier 2 class receivers (`Arc<Mutex<T>>`) can deadlock if a lock is held across an `await` point. Sarah's codegen must apply the lock-before-await mitigation and emit an `ASYNC-LOCK-RISK` diagnostic.

### Verdict

**Best default architecture for Sarah.** The complexity cost is paid once, in generated code, and is invisible to both Shell developers and SwiftUI developers.

---

## Option 2 — Native UniFFI Async

### Shape

Rust `async fn` annotated with `#[uniffi::export]`. UniFFI generates a native Swift `async func`. The generated code uses a poll/complete/free future lifecycle internally.

### Strengths

- **Best surface ergonomics**: pure `async func` with no wrapping of any kind.
- **Swift structured concurrency composes naturally**: `async let`, `TaskGroup`, cancellation all work.
- **Less bridge code** for Sarah to generate in the Shell layer.
- **Cancellation opt-in** is already supported in recent UniFFI versions.
- **Correct long-term direction** as UniFFI and Swift concurrency both mature.

### Downsides

- UniFFI's async FFI uses a non-trivial poll/complete/free future handle lifecycle that operates across the language boundary. This is more complex to debug than a simple callback fire.
- UniFFI's own documentation notes that Swift async bindings do not yet fully conform to `Sendable`, which causes warnings under `-strict-concurrency=complete` — the strictness level Apple is moving toward as the default.
- For a transpiler, this reduces local control over the most failure-prone part of the generated output.
- If the upstream Sendable issue or other edge cases produce compiler warnings in generated code, Sarah's output looks low-quality through no fault of its own.

### Verdict

**Strong future option**. Revisit when the UniFFI Sendable gap is resolved (see ADR-006 review trigger).

---

## Option 3 — Callback-Only Public API

### Shape

UniFFI callback interfaces as the primary public async mechanism. Completion handlers or delegate objects are delivered directly to SwiftUI code.

### Strengths

- Very explicit; easy to reason about per function.
- Stable, battle-tested UniFFI mechanism used in Mozilla production.
- No concurrency model assumptions on the Swift side.

### Downsides

- Poor fit for modern Swift application architecture: `async`/`await` is the dominant idiom since Swift 5.5.
- Breaks composition with `Task`, `async let`, and `TaskGroup`.
- Pushes callback-to-continuation glue onto app developers instead of into Sarah-generated code.
- Makes the transpiled result feel legacy even when the Rust internals are modern.

### Verdict

**Good as internal transport** (which is exactly how Option 1 uses it). **Poor as a public API surface.**

---

## Option 4 — Job Handle / Polling

### Shape

Start an operation, receive a handle or request ID, then poll for status or subscribe to a result channel.

### Strengths

- Works in runtimes with minimal async integration.
- Useful for durable workflows, background jobs, or progress streams.

### Downsides

- Adds state machine complexity on both sides of the boundary.
- Much worse fit for typical SwiftUI request-response flows.
- Harder for a transpiler to synthesize clean idiomatic surface code.
- Introduces cancellation and timeout bookkeeping as explicit app-developer concerns.

### Verdict

**Possible for special cases** (e.g. long-running background tasks with progress). **Not a general transpilation strategy.**

---

## Option 5 — Blocking / Synchronous Façade

### Shape

Hide async work by blocking a thread until Rust completes.

### Strengths

- Superficially simple.
- Requires no async understanding on either side.

### Downsides

- Unsafe from a UI responsiveness perspective.
- Works against Swift concurrency rather than with it.
- Fights Rust's async model too — blocking inside a Tokio task starves the executor.
- Makes cancellation and backpressure invisible.
- Trivially deadlocks if misused with callbacks or completion handlers.

### Verdict

**Do not use** except in narrowly isolated, non-UI tooling paths.

---

## Best Techniques Inside the Three-Zone Hybrid

These are the specific implementation practices that make Option 1 robust:

| Technique | Rule |
|-----------|------|
| **Tokio ownership** | Rust Core initialises and owns the Tokio runtime. The Shell never imports or references Tokio. |
| **`spawn_blocking` for sync bodies** | If a Swift `async func` body contains no `await`, generate `spawn_blocking` rather than `async fn`. Keeps Tier 1 Rust free of async. |
| **`withCheckedThrowingContinuation`** | Default wrapper for A1 functions that `throws`. |
| **`withCheckedContinuation`** | For A1-sync functions that do not `throws`. |
| **`withTaskCancellationHandler`** | Generated when source is annotated `@cancellable`. Drives a `CancellationToken` on the Rust side via `tokio::select!`. |
| **Lock-before-await pattern** | For Tier A2, acquire the `Mutex`, clone needed state, release the lock, *then* `.await`. Never hold a lock across an `await` point. |
| **`ASYNC-LOCK-RISK` diagnostic** | Emitted by SPEC-001 classifier on any `async func` whose receiver is a Tier 2 class. |
| **`--async-mode` flag** | Transpiler flag (`bridge` / `native`) to gate migration to native UniFFI async when ready. |

---

## Decision Rule

```
if goal == "best current transpilation safety and control":
    → Three-zone hybrid (Option 1)

elif uniffi_sendable_gap_resolved and apple_strict_concurrency_stable:
    → Consider native UniFFI async (Option 2) via --async-mode=native

elif special_case == "durable background workflow":
    → Job handle pattern (Option 4) for that specific callsite only

else:
    → Option 1
```

---

## Summary

For Sarah, the correct posture is:

> **Generate a modern async Swift surface, keep the Rust executor private, and use the callback seam only as an internal transport layer.**

The callback bridge is not a compromise — it is the stable, explicit, controlled mechanism that lets both sides of the boundary be fully idiomatic in their own language, today, without waiting on upstream toolchain issues to resolve.

---

## Cross-References

- `docs/specs/SPEC-006-async-bridging-strategy.md` — full specification of the three-zone model
- `docs/adrs/ADR-006-three-zone-async-boundary-model.md` — architecture decision record
- `docs/specs/SPEC-001-core-shell-classification.md` v1.1 — async tier taxonomy (A1, A1-sync, A2)
