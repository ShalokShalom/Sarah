// CounterViewModel.swift — Thin bridge between SwiftUI and the Rust Core.
//
// SHELL LAYER — this file contains zero business logic.
//
// Its only responsibilities are:
//   1. Hold a reference to the UniFFI-generated `Counter` object (Rust core).
//   2. Forward user actions to Core methods.
//   3. Publish the resulting state so SwiftUI can re-render.
//
// When UniFFI bindings are generated, replace the stub `Counter` below with
// the real import:
//   import SwiftRustCore

import SwiftUI
import Combine

// ---------------------------------------------------------------------------
// STUB — replace with UniFFI-generated Counter once `cargo build` + bindgen
// has been run. The stub mirrors the exact API the real Counter will expose.
// ---------------------------------------------------------------------------
final class Counter {
    private var _value: Int64

    init(initial: Int64) {
        self._value = initial
    }

    func value() -> Int64 { _value }

    func increment() throws {
        guard _value < Int64.max else { throw CounterError.overflow }
        _value += 1
    }

    func decrement() throws {
        guard _value > Int64.min else { throw CounterError.underflow }
        _value -= 1
    }

    func reset() { _value = 0 }

    func add(delta: Int64) throws {
        let (result, overflow) = _value.addingReportingOverflow(delta)
        if overflow { throw delta > 0 ? CounterError.overflow : CounterError.underflow }
        _value = result
    }
}

enum CounterError: Error, LocalizedError {
    case overflow
    case underflow

    var errorDescription: String? {
        switch self {
        case .overflow:  return "Counter overflow"
        case .underflow: return "Counter underflow"
        }
    }
}
// ---------------------------------------------------------------------------
// END STUB
// ---------------------------------------------------------------------------

/// ViewModel for `ContentView`.
///
/// This is intentionally thin: it delegates all meaningful behaviour to the
/// Rust `Counter` core. The VM owns no business logic.
@MainActor
final class CounterViewModel: ObservableObject {
    @Published private(set) var displayValue: String = "0"
    @Published private(set) var errorMessage: String? = nil
    @Published private(set) var hasError: Bool = false

    private let core: Counter

    init(initialValue: Int64 = 0) {
        self.core = Counter(initial: initialValue)
        syncDisplay()
    }

    // MARK: — Actions (forwarded directly to Rust Core)

    func increment() {
        perform { try self.core.increment() }
    }

    func decrement() {
        perform { try self.core.decrement() }
    }

    func reset() {
        core.reset()
        clearError()
        syncDisplay()
    }

    func add(_ delta: Int64) {
        perform { try self.core.add(delta: delta) }
    }

    // MARK: — Private Helpers

    private func perform(_ action: () throws -> Void) {
        do {
            try action()
            clearError()
        } catch {
            errorMessage = error.localizedDescription
            hasError = true
        }
        syncDisplay()
    }

    private func syncDisplay() {
        displayValue = String(core.value())
    }

    private func clearError() {
        errorMessage = nil
        hasError = false
    }
}
