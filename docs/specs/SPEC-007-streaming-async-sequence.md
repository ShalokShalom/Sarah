# SPEC-007 — Streaming / AsyncSequence Strategy (Stub)

**Status:** Draft  
**Phase:** 3 (planned)  
**Authors:** Sarah Architecture Working Group  
**Date:** 2026-04-02  
**Prerequisite:** SPEC-006 (async strategy)

---

## 1. Context

SPEC-006 covers request–response async (`async func f() -> T`). A second
class of async pattern exists in Swift: **streaming**, expressed as
`AsyncSequence`. The canonical Rust equivalent is a `tokio::sync::mpsc`
channel or a `futures::Stream`.

This SPEC is a design stub. It reserves the SPEC number, names the open
questions, and sketches the three candidate strategies so Phase 3
implementation work can start from a shared vocabulary.

---

## 2. Swift Patterns in Scope

| Pattern | Example |
|---------|--------|
| `AsyncStream<T>` | Sensor readings, UI events |
| `AsyncThrowingStream<T, E>` | Fallible streams (network, file) |
| `for await item in sequence` | Consumer loop |
| Combine `Publisher` (bridged) | Legacy interop |

---

## 3. Rust Equivalents

| Rust pattern | Characteristics |
|---|---|
| `tokio::sync::mpsc::channel` | Bounded or unbounded; Tokio-native |
| `tokio_stream::wrappers::ReceiverStream` | Wraps `mpsc::Receiver` as a `Stream` |
| `futures::stream::Stream` trait | General; requires `tokio-stream` or `async-stream` |
| `async-stream` macro | Clean syntax; compiles to a state machine |

---

## 4. Candidate Strategies

### Strategy S1 — Callback-Per-Item (extends SPEC-006 Strategy A)

Extend the callback interface with an `on_item` method. The Swift Shell
constructs an `AsyncStream` that resumes with each item. Completion is
signalled by `on_done` / `on_error`.

```rust
#[uniffi::export(callback_interface)]
pub trait FeedStreamCallback: Send + Sync {
    fn on_item  (&self, item:  FeedItem);
    fn on_done  (&self);
    fn on_error (&self, error: String);
}
```

```swift
public func feedStream() -> AsyncThrowingStream<FeedItem, Error> {
    AsyncThrowingStream { continuation in
        let cb = FeedStreamCallbackImpl(
            onItem:  { continuation.yield($0) },
            onDone:  { continuation.finish() },
            onError: { continuation.finish(throwing: SarahError($0)) }
        )
        CoreFFI.feedStream(callback: cb)
    }
}
```

**Pros:** Extends Strategy A; no new UniFFI mechanisms needed.  
**Cons:** One `Arc<dyn Callback>` allocation per stream; back-pressure not automatic.

---

### Strategy S2 — Channel-as-Object

Expose a `StreamHandle` as a `uniffi::Object`. The Swift side calls
`handle.next()` as an `async func` (SPEC-006 Strategy B) to pull items.

```rust
#[derive(uniffi::Object)]
pub struct StreamHandle { rx: Mutex<mpsc::Receiver<FeedItem>> }

#[uniffi::export]
impl StreamHandle {
    pub async fn next(&self) -> Option<FeedItem> {
        self.rx.lock().await.recv().await
    }
}
```

**Pros:** Natural pull model; back-pressure via channel capacity.  
**Cons:** Requires native UniFFI async (SPEC-006 Strategy B); polling in Swift.

---

### Strategy S3 — Combine Publisher Bridge (Legacy Interop Only)

Generate a `Combine.PassthroughSubject` wrapper for Combine-using code.
Not recommended for new code; emits a T2-COMBINE diagnostic.

---

## 5. Open Questions

1. **Back-pressure.** How does a slow Swift consumer signal to the Rust
   producer that it should pause? Strategy S1 has no back-pressure; S2
   has channel-capacity back-pressure.
2. **Cancellation.** When the Swift `for await` loop is cancelled, how
   does the Rust side learn to stop producing items?
3. **Error mid-stream.** `AsyncThrowingStream` allows one terminal error.
   Rust streams can produce multiple errors. How is this mapped?
4. **Typed errors.** Same open question as SPEC-006 §8.

---

## 6. Recommendation (Provisional)

Default to **Strategy S1** for Phase 3, consistent with SPEC-006's
preference for `callback_interface`. Add **Strategy S2** as
`--stream-mode pull` once native UniFFI async is stable.

---

*This stub will be expanded to a full SPEC before Phase 3 implementation begins.*
