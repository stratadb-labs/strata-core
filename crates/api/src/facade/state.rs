//! State Facade - Simplified state cell operations
//!
//! This module provides atomic compare-and-swap operations for coordination.
//!
//! ## Use Cases
//!
//! - Leader election
//! - Distributed locks
//! - Single-writer coordination
//!
//! ## Desugaring
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `state_get(cell)` | `state_get(default_run, cell).map(\|v\| v.value)` |
//! | `state_set(cell, val)` | `state_set(default_run, cell, val)` |
//! | `state_cas(cell, exp, val)` | `state_cas(default_run, cell, exp, val)` |

use strata_core::{StrataResult, Value};

/// State value with counter
#[derive(Debug, Clone)]
pub struct StateValue {
    /// The current value
    pub value: Value,
    /// The version counter (increments on each write)
    pub counter: u64,
}

/// State Facade - simplified CAS operations
///
/// State cells provide single-writer coordination with compare-and-swap.
///
/// ## Counter Semantics
///
/// Every state cell has a counter starting at 0. Every write increments
/// the counter by 1, regardless of value change.
pub trait StateFacade {
    /// Get the current value and counter
    ///
    /// Returns `None` if cell doesn't exist.
    fn state_get(&self, cell: &str) -> StrataResult<Option<StateValue>>;

    /// Set the value unconditionally
    ///
    /// Increments the counter and returns the new counter value.
    fn state_set(&self, cell: &str, value: Value) -> StrataResult<u64>;

    /// Compare-and-swap
    ///
    /// Sets the value only if the current counter matches `expected_counter`.
    ///
    /// ## Parameters
    /// - `expected_counter`: `None` means "only if cell doesn't exist"
    /// - `value`: The new value to set
    ///
    /// ## Return Value
    /// - `Some(new_counter)`: CAS succeeded
    /// - `None`: CAS failed (counter mismatch)
    ///
    /// ## Example
    /// ```ignore
    /// // Try to acquire a lock
    /// match facade.state_cas("lock", None, Value::String("owner1".into()))? {
    ///     Some(counter) => println!("Lock acquired at counter {}", counter),
    ///     None => println!("Lock already held"),
    /// }
    ///
    /// // Try to release the lock (only if we own it at expected counter)
    /// match facade.state_cas("lock", Some(1), Value::Null)? {
    ///     Some(_) => println!("Lock released"),
    ///     None => println!("Lock was modified by someone else"),
    /// }
    /// ```
    fn state_cas(&self, cell: &str, expected_counter: Option<u64>, value: Value)
        -> StrataResult<Option<u64>>;

    /// Delete a cell
    ///
    /// Returns `true` if the cell existed.
    fn state_del(&self, cell: &str) -> StrataResult<bool>;

    /// Check if a cell exists
    fn state_exists(&self, cell: &str) -> StrataResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn StateFacade) {}
    }
}
