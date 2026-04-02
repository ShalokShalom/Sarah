# Sarah

A transpiler project and integration protocol to connect Swift to the Rust ecosystem. 

# Or also called: The Rust Takeover of the Swift Ecosystem :wink:

| Field        | Value                                      |
|--------------|--------------------------------------------|
| Status       | Draft                                      |
| Author       | Architecture Team                          |
| Date         | 2026-04-02                                 |
| Bounded Context | Ecosystem Strategy & Migration Roadmap  |
| Related Specs | SPEC-002 Progressive Migration Engine     |
| Related ADRs | ADR-005 Tiered Lowering vs Strict Subset   |

***

## 1. Vision

### 1.1 Long-Term Goal

Over a year-long horizon, we aim to shift the **center of gravity** of the current Swift ecosystem so that:

- The majority of **core business logic, domain models, and cross-platform behavior** lives in Rust crates.
- Swift is used primarily as a **thin platform shell** for UI, OS integration, and platform-specific ergonomics.
- New greenfield projects default to **Rust core + thin Swift shell**, and brownfield Swift projects progressively migrate core logic into Rust.

This is **bigger than a transpiler**. It is an ecosystem replacement strategy, combining:

- A progressive Swift→Rust migration compiler.
- A standardized FFI layer (UniFFI and friends).
- A set of opinionated architectural patterns (Core/Shell, Unidirectional Data Flow).
- Education, tooling, and library support that make Rust feel like a natural evolution of serious Swift projects.

## 2. Phased Roadmap

### Phase 0 — Foundations

**Objective:** Make it technically and ergonomically trivial to combine Rust cores with Swift shells on iOS/macOS.

- Stabilize our **Progressive Migration Engine** (SPEC-002) for real projects.
- Invest in FFI generation using **UniFFI** as the default bridge.
- Document and publish canonical examples:
  - A small iOS app with SwiftUI UI and a Rust core (inspired by Crux and existing case studies).
  - A UIKit app that moves its networking and data layer into Rust.

**Success metrics:**
- Internal projects can share a single Rust core between Swift and Kotlin shells with minimal friction.
- Documentation includes end-to-end guides ("From Swift Model to Rust Core").

### Phase 1 — Targeted Domains

**Objective:** Make Rust the obvious choice for performance-sensitive and security-critical domains currently written in Swift.

Target verticals:
- Cryptography & security-sensitive code.
- Offline-first sync engines & caching layers.
- Data processing, compression, and media handling.

Actions:
- Build a catalog of **domain-specific Rust libraries** with first-class UniFFI bindings for Swift.
- Publish case studies of existing Swift teams migrating these layers to Rust.
- Provide **linting and diagnostics** in the transpiler that highlight where Swift code could be safely and profitably migrated.

### Phase 2 — Core Application Logic Migration 

**Objective:** Move entire feature domains, not just subsystems, into Rust.

- Extend the Progressive Migration Engine to handle more patterns of object graphs (using `Arc<RwLock<T>>` when necessary).
- Start with modules that are **UI-agnostic**: billing flows, onboarding flows, domain models, state machines.
- Introduce a **Swift-R profile** that encourages developers to write new code in an ownership-clean style amenable to idiomatic Rust.

### Phase 3 — Ecosystem Standardization

**Objective:** Normalize the Rust-core + thin Swift-shell architecture.

- Publish an official **"Rust for Swift Developers"** guide that shows the migration path from common Swift idioms to Rust patterns.
- Work with existing efforts (e.g. Crux, UniFFI, Rust mobile tooling) to define:
  - A standard project layout for Swift+Rust apps.
  - Shared build tooling (cargo-xcode, Swift Package Manager plugins).
- Encourage community frameworks around Rust ViewModels backing SwiftUI (either directly or via projects like `lera`).

### Phase 4 — New Projects Default to Rust Core (Years 5+)

**Objective:** For new Swift-centric projects, having a shared Rust core is the default, not the exception.

- Provide templates and scaffolding tools that generate a Rust core + SwiftUI shell by default.
- Educate teams that conventional-yet-complex Swift logic (e.g. large Redux-style stores or complex reactive graphs) is better hosted in Rust.
- At this stage, the ecosystem has effectively shifted: Swift is the shell language; Rust is the place for logic.

## 3. Bounded Contexts in the acquisition of the ecosystem

We explicitly recognize several bounded contexts:

1. **Transpiler Context** — owns the AST analyzer, Tiered Lowering, and code generation.
2. **Core Library Context** — owns Rust crates and their Swift bindings (UniFFI schemas, type mappings).
3. **Shell UI Context** — owns SwiftUI/UIKit code and platform-specific APIs.
4. **Education & Ecosystem Context** — owns docs, examples, templates, and community narratives.

Each context has its own language and concerns; the roadmap coordinates them without collapsing them.

## 4. Documentation-Driven Development (DDD) for the acquisition of the ecosystem

We apply the Documentation-Driven Development playbook:

- Every major roadmap step gets:
  - An RFC (vision & justification).
  - One or more Specs (precise behavior and interfaces).
  - ADRs for structural decisions (e.g., choosing UniFFI as default bridge).
- **Code follows docs.** The transpiler, example apps, and libraries implement what the specs declare.

### Example: RFC for Mobile Core Architecture

```markdown
# RFC-010: Rust Core + Swift Shell Architecture for Mobile

## Summary
We propose a two-layer architecture for iOS apps: a pure Rust core containing
all business logic and a thin Swift shell for UI and OS integration.

## Motivation
Swift teams want stronger guarantees around memory safety and concurrency,
without giving up SwiftUI or the Apple ecosystem.

## Detailed Design
- Core: Rust crate exposing UniFFI bindings.
- Shell: SwiftUI app referencing the generated Swift package.
- Data flows via unidirectional messages (Action, State, Effect).

## Drawbacks
- Requires teams to learn Rust.
- Build tooling becomes more complex.
```

### Example: Spec for a Shared Login Flow

```markdown
# SPEC-020: Shared Login Flow in Rust

## Problem Statement
We need a login flow that is shared across iOS and Android, yet integrates
with platform-native UI and secure credential storage.

## Goals
1. Implement the login state machine entirely in Rust.
2. Use UniFFI to expose functions to Swift and Kotlin.
3. Keep the Swift/Kotlin side as thin as possible.

## Behaviour
- Input: username, password.
- On `submit`, core validates input and performs a network request.
- On success, emits `State::LoggedIn` and Effect::StoreCredentials.
- On failure, emits `State::Error(message)`.
```

## 5. Examples with Doc Tests

### 5.1 Rust Core: Doc-Testable State Machine

```rust
/// Represents the login state of the application.
#[derive(Debug, PartialEq)]
pub enum LoginState {
    LoggedOut,
    InProgress,
    LoggedIn { user_id: String },
    Error { message: String },
}

/// A simple login state machine.
///
/// # Examples
///
/// ```
/// use core_auth::{LoginState, LoginMachine};
///
/// let mut m = LoginMachine::new();
/// assert_eq!(m.state(), &LoginState::LoggedOut);
///
/// // Submitting invalid credentials yields an error state.
/// m.submit("", "");
/// assert!(matches!(m.state(), LoginState::Error { .. }));
/// ```
pub struct LoginMachine {
    state: LoginState,
}

impl LoginMachine {
    pub fn new() -> Self {
        Self { state: LoginState::LoggedOut }
    }

    pub fn state(&self) -> &LoginState {
        &self.state
    }

    pub fn submit(&mut self, username: &str, password: &str) {
        if username.is_empty() || password.is_empty() {
            self.state = LoginState::Error {
                message: "Missing credentials".into(),
            };
        } else {
            self.state = LoginState::InProgress;
            // Network call elided; assume success
            self.state = LoginState::LoggedIn {
                user_id: username.to_string(),
            };
        }
    }
}
```

This code is **doc-testable**: `cargo test` will execute the example in the documentation, ensuring that our published behavior in docs and actual behavior in code never diverge.

### 5.2 Swift Shell: Thin Wrapper Around Rust Core

```swift
import SwiftUI
import CoreAuth // UniFFI generated from the Rust crate

@MainActor
final class LoginViewModel: ObservableObject {
    @Published private(set) var state: LoginState = .loggedOut
    private var machine = LoginMachine()

    func submit(username: String, password: String) {
        machine.submit(username: username, password: password)
        state = machine.state
    }
}

struct LoginView: View {
    @StateObject var viewModel = LoginViewModel()

    var body: some View {
        VStack {
            // Text fields elided
            Button("Login") {
                viewModel.submit(username: "alice", password: "secret")
            }
        }
    }
}
```

The Swift code is intentionally thin: it delegates all meaningful behavior to the Rust core, aligning with the long-term acquisition of the ecosystem as the primary goal.

## 6. Risks and Mitigations

- **Risk:** Teams may resist Rust due to perceived complexity.
  - *Mitigation:* Provide strong documentation, profiles, and linting that guide Swift developers gradually.

- **Risk:** Tooling (Xcode + Cargo) friction.
  - *Mitigation:* Invest in templates, scripts, and CI examples that hide most of the complexity.

- **Risk:** Partial migrations get stuck in a "mixed" state.
  - *Mitigation:* The roadmap explicitly embraces hybrid states as first-class citizens and offers guidance on progressively moving more code into Rust over time.

***

*This roadmap is a living document. It will be revised with new ADRs and Specs as real-world experience accumulates.*
