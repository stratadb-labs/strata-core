//! Transaction extension traits for cross-primitive operations
//!
//! ## Design Principle
//!
//! Extension traits allow multiple primitives to participate in a single
//! transaction. Each trait provides domain-specific methods that operate
//! on a `TransactionContext`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use in_mem_primitives::extensions::*;
//!
//! db.transaction(run_id, |txn| {
//!     // KV operation
//!     txn.kv_put("key", value)?;
//!
//!     // Event operation
//!     txn.event_append("type", payload)?;
//!
//!     // State operation
//!     txn.state_cas("cell", version, new_value)?;
//!
//!     // Trace operation
//!     txn.trace_record("ToolCall", metadata)?;
//!
//!     Ok(())
//! })?;
//! ```
//!
//! ## Implementation Note
//!
//! Extension traits DELEGATE to primitive internals - they do NOT
//! reimplement logic. Each trait implementation calls the same
//! internal functions used by the standalone primitive API.
//!
//! This ensures:
//! - Single source of truth for each primitive's logic
//! - Consistent behavior between standalone and transaction APIs
//! - Easier maintenance and testing

use in_mem_core::{Result, Value};

// Forward declarations - traits are defined here, implementations
// are added in their respective primitive modules.

/// KV operations within a transaction
///
/// Implemented in `kv.rs` (Story #173)
pub trait KVStoreExt {
    /// Get a value by key
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>>;

    /// Put a value
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()>;

    /// Delete a key
    fn kv_delete(&mut self, key: &str) -> Result<()>;
}

/// Event log operations within a transaction
///
/// Implemented in `event_log.rs` (Story #179)
pub trait EventLogExt {
    /// Append an event and return sequence number
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64>;

    /// Read an event by sequence number
    fn event_read(&mut self, sequence: u64) -> Result<Option<Value>>;
}

/// State cell operations within a transaction
///
/// Implemented in `state_cell.rs` (Story #184)
pub trait StateCellExt {
    /// Read current state
    fn state_read(&mut self, name: &str) -> Result<Option<Value>>;

    /// Compare-and-swap update, returns new version
    fn state_cas(&mut self, name: &str, expected_version: u64, new_value: Value) -> Result<u64>;

    /// Unconditional set, returns new version
    fn state_set(&mut self, name: &str, value: Value) -> Result<u64>;
}

/// Trace store operations within a transaction
///
/// Implemented in `trace.rs` (Story #190)
pub trait TraceStoreExt {
    /// Record a trace and return trace ID
    fn trace_record(&mut self, trace_type: &str, metadata: Value) -> Result<String>;

    /// Record a child trace
    fn trace_record_child(
        &mut self,
        parent_id: &str,
        trace_type: &str,
        metadata: Value,
    ) -> Result<String>;
}

// Note: RunIndex does not have an extension trait because run operations
// are typically done outside of cross-primitive transactions. Run lifecycle
// operations (create, complete, fail) are usually standalone operations
// that bookend a series of primitive operations.

#[cfg(test)]
mod tests {
    use super::*;

    // These tests verify trait definitions compile correctly.
    // Implementation tests will be in their respective primitive stories.

    #[test]
    fn test_traits_are_object_safe() {
        // Verify traits can be used as trait objects if needed
        fn _accepts_kv_ext(_ext: &dyn KVStoreExt) {}
        fn _accepts_event_ext(_ext: &dyn EventLogExt) {}
        fn _accepts_state_ext(_ext: &dyn StateCellExt) {}
        fn _accepts_trace_ext(_ext: &dyn TraceStoreExt) {}
    }
}
