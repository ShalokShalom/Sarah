//! Error types for the swift-rust-core crate.
//!
//! All errors derive `uniffi::Error` so they can be thrown as typed Swift
//! `Error` values across the UniFFI boundary.

/// Errors that can occur during counter operations.
///
/// # Examples
///
/// ```
/// use swift_rust_core::errors::CounterError;
///
/// let e = CounterError::Overflow;
/// assert_eq!(format!("{e}"), "Counter overflow");
/// ```
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CounterError {
    /// The counter has reached `i64::MAX` and cannot be incremented further.
    #[error("Counter overflow")]
    Overflow,

    /// The counter has reached `i64::MIN` and cannot be decremented further.
    #[error("Counter underflow")]
    Underflow,
}
