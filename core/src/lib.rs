//! swift-rust-core — Phase 1 canonical example
//!
//! This crate is the Rust **Core** layer of the Swift-Rust Bridge project.
//! It exposes a simple `Counter` object and related types via UniFFI so that
//! a thin SwiftUI shell can call into this crate without writing any FFI glue
//! by hand.
//!
//! # Architecture
//!
//! ```text
//! SwiftUI Shell  ──UniFFI──▶  Counter (this crate)
//! ```
//!
//! All business logic lives here. The Swift side only handles rendering and
//! user interaction.

// Re-export sub-modules (expanded in later phases).
pub mod counter;
pub mod errors;

// Pull public types into crate root for UniFFI scaffolding.
pub use counter::Counter;
pub use errors::CounterError;

// UniFFI scaffolding macro — must appear exactly once in the crate root.
uniFFI::setup_scaffolding!();
