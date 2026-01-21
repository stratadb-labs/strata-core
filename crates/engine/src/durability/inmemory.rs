//! InMemory durability mode
//!
//! No WAL, no fsync. All data lost on crash.
//! Fastest mode - target <3µs for engine/put_direct.
//!
//! # Use Cases
//!
//! - Unit tests (fast, no cleanup needed)
//! - Caches and ephemeral data
//! - Development and prototyping
//! - Benchmarking storage layer performance
//!
//! # Performance Contract
//!
//! - `persist()`: No-op, immediate return
//! - No syscalls on hot path
//! - No allocations beyond what transaction provides
//! - Target: <3µs for engine/put_direct

use super::Durability;
use strata_concurrency::TransactionContext;
use strata_core::error::Result;

/// InMemory durability - no persistence
///
/// This mode provides maximum performance by completely bypassing
/// the write-ahead log. All data exists only in memory and is lost
/// when the process terminates.
///
/// # Thread Safety
///
/// This struct is stateless and inherently thread-safe.
///
/// # Example
///
/// ```ignore
/// use strata_engine::durability::{Durability, InMemoryDurability};
///
/// let durability = InMemoryDurability::new();
/// assert!(!durability.is_persistent());
/// assert!(!durability.requires_wal());
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct InMemoryDurability;

impl InMemoryDurability {
    /// Create new InMemory durability mode
    ///
    /// This is a zero-cost operation - InMemoryDurability is stateless.
    pub fn new() -> Self {
        Self
    }
}

impl Durability for InMemoryDurability {
    /// No-op persist - InMemory mode doesn't write to WAL
    ///
    /// # Performance
    ///
    /// This method is designed to be syscall-free and allocation-free.
    /// It should complete in nanoseconds.
    ///
    /// # Arguments
    ///
    /// * `_txn` - Transaction context (ignored)
    /// * `_commit_version` - Commit version (ignored)
    #[inline]
    fn persist(&self, _txn: &TransactionContext, _commit_version: u64) -> Result<()> {
        // Hot path - no WAL, no fsync, no logging
        // This must be as fast as possible
        Ok(())
    }

    /// No-op shutdown - nothing to flush
    ///
    /// InMemory mode has no buffered data to persist.
    #[inline]
    fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// InMemory mode does not persist data
    #[inline]
    fn is_persistent(&self) -> bool {
        false
    }

    /// Returns "InMemory"
    #[inline]
    fn mode_name(&self) -> &'static str {
        "InMemory"
    }

    /// InMemory mode does not require WAL
    #[inline]
    fn requires_wal(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inmemory_not_persistent() {
        let durability = InMemoryDurability::new();
        assert!(!durability.is_persistent());
    }

    #[test]
    fn test_inmemory_does_not_require_wal() {
        let durability = InMemoryDurability::new();
        assert!(!durability.requires_wal());
    }

    #[test]
    fn test_inmemory_mode_name() {
        let durability = InMemoryDurability::new();
        assert_eq!(durability.mode_name(), "InMemory");
    }

    #[test]
    fn test_inmemory_shutdown_succeeds() {
        let durability = InMemoryDurability::new();
        assert!(durability.shutdown().is_ok());
    }

    #[test]
    fn test_inmemory_default() {
        let durability = InMemoryDurability::default();
        assert!(!durability.is_persistent());
    }

    #[test]
    fn test_inmemory_is_copy() {
        let d1 = InMemoryDurability::new();
        let d2 = d1; // Copy
        assert_eq!(d1.mode_name(), d2.mode_name());
    }
}
