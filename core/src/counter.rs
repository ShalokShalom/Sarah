//! Counter — the Phase 1 canonical Rust core type.
//!
//! `Counter` is a Tier-2-style object (SPEC-003) exposed via UniFFI.
//! It demonstrates:
//!
//! - `#[derive(uniffi::Object)]` for reference-typed objects.
//! - `Mutex<T>` for interior mutability without `&mut self` on exported methods.
//! - `Result<_, CounterError>` for typed error propagation.
//! - Doc-tests as living contracts (run with `cargo test`).

use std::sync::Mutex;
use crate::errors::CounterError;

/// A thread-safe, incrementable counter.
///
/// `Counter` is intentionally simple so that the entire Swift-Rust bridge
/// pattern (Core/Shell + UniFFI) can be demonstrated without domain noise.
///
/// # Examples
///
/// ```
/// use swift_rust_core::counter::Counter;
///
/// let c = Counter::new(0);
/// assert_eq!(c.value(), 0);
///
/// c.increment().unwrap();
/// c.increment().unwrap();
/// assert_eq!(c.value(), 2);
///
/// c.decrement().unwrap();
/// assert_eq!(c.value(), 1);
///
/// c.reset();
/// assert_eq!(c.value(), 0);
/// ```
#[derive(uniffi::Object)]
pub struct Counter {
    // TIER-2: Mutex wraps the mutable state so `&self` methods can mutate
    // it safely. In a Tier-1 refactor this would become a plain i64 with
    // value semantics and no shared ownership.
    value: Mutex<i64>,
}

#[uniffi::export]
impl Counter {
    /// Create a new `Counter` starting at `initial`.
    ///
    /// # Examples
    ///
    /// ```
    /// use swift_rust_core::counter::Counter;
    ///
    /// let c = Counter::new(42);
    /// assert_eq!(c.value(), 42);
    /// ```
    #[uniffi::constructor]
    pub fn new(initial: i64) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            value: Mutex::new(initial),
        })
    }

    /// Returns the current counter value.
    ///
    /// # Examples
    ///
    /// ```
    /// use swift_rust_core::counter::Counter;
    /// let c = Counter::new(7);
    /// assert_eq!(c.value(), 7);
    /// ```
    pub fn value(&self) -> i64 {
        *self.value.lock().unwrap()
    }

    /// Increments the counter by 1.
    ///
    /// Returns [`CounterError::Overflow`] if the counter is already at `i64::MAX`.
    ///
    /// # Examples
    ///
    /// ```
    /// use swift_rust_core::counter::Counter;
    ///
    /// let c = Counter::new(0);
    /// c.increment().unwrap();
    /// assert_eq!(c.value(), 1);
    /// ```
    ///
    /// ```
    /// use swift_rust_core::{counter::Counter, errors::CounterError};
    ///
    /// let c = Counter::new(i64::MAX);
    /// let err = c.increment().unwrap_err();
    /// assert!(matches!(err, CounterError::Overflow));
    /// ```
    pub fn increment(&self) -> Result<(), CounterError> {
        let mut v = self.value.lock().unwrap();
        *v = v.checked_add(1).ok_or(CounterError::Overflow)?;
        Ok(())
    }

    /// Decrements the counter by 1.
    ///
    /// Returns [`CounterError::Underflow`] if the counter is already at `i64::MIN`.
    ///
    /// # Examples
    ///
    /// ```
    /// use swift_rust_core::counter::Counter;
    ///
    /// let c = Counter::new(3);
    /// c.decrement().unwrap();
    /// assert_eq!(c.value(), 2);
    /// ```
    ///
    /// ```
    /// use swift_rust_core::{counter::Counter, errors::CounterError};
    ///
    /// let c = Counter::new(i64::MIN);
    /// let err = c.decrement().unwrap_err();
    /// assert!(matches!(err, CounterError::Underflow));
    /// ```
    pub fn decrement(&self) -> Result<(), CounterError> {
        let mut v = self.value.lock().unwrap();
        *v = v.checked_sub(1).ok_or(CounterError::Underflow)?;
        Ok(())
    }

    /// Resets the counter to zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use swift_rust_core::counter::Counter;
    ///
    /// let c = Counter::new(99);
    /// c.reset();
    /// assert_eq!(c.value(), 0);
    /// ```
    pub fn reset(&self) {
        *self.value.lock().unwrap() = 0;
    }

    /// Adds `delta` to the counter.
    ///
    /// Returns [`CounterError::Overflow`] or [`CounterError::Underflow`] on
    /// integer boundary violations.
    ///
    /// # Examples
    ///
    /// ```
    /// use swift_rust_core::counter::Counter;
    ///
    /// let c = Counter::new(10);
    /// c.add(5).unwrap();
    /// assert_eq!(c.value(), 15);
    ///
    /// c.add(-3).unwrap();
    /// assert_eq!(c.value(), 12);
    /// ```
    pub fn add(&self, delta: i64) -> Result<(), CounterError> {
        let mut v = self.value.lock().unwrap();
        *v = v.checked_add(delta)
            .ok_or(if delta > 0 { CounterError::Overflow } else { CounterError::Underflow })?;
        Ok(())
    }
}
