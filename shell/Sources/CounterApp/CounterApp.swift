// CounterApp.swift — App entry point (Shell layer)
//
// This file is part of the SHELL. It contains no business logic.
// All state management is delegated to CounterViewModel, which delegates
// to the Rust Core via UniFFI.

import SwiftUI

@main
struct CounterApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
