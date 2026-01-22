//! EventLog Substrate Operations
//!
//! The EventLog provides append-only event streams for logging and messaging.
//! Events are immutable once appended and use sequence-based versioning.
//!
//! ## Stream Model
//!
//! - Events are organized into named streams
//! - Each stream has independent sequence numbers
//! - Events are immutable (append-only, no updates or deletes)
//!
//! ## Versioning
//!
//! Events use sequence-based versioning (`Version::Sequence`).
//! Each event gets a unique, monotonically increasing sequence number within its stream.
//!
//! ## Payload
//!
//! Event payloads must be `Value::Object`. Empty objects `{}` are allowed.
//! Bytes values are allowed within the payload (encoded via `$bytes` wrapper on wire).

use super::types::ApiRunId;
use strata_core::{StrataResult, Value, Version, Versioned};

/// EventLog substrate operations
///
/// This trait defines the canonical event log operations.
/// All operations require explicit run_id and return versioned results.
///
/// ## Contract
///
/// - Events are append-only (no updates, no deletes)
/// - Payloads must be `Value::Object`
/// - Sequence numbers are unique and monotonically increasing within a stream
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid stream name | `InvalidKey` |
/// | Payload not Object | `ConstraintViolation` |
/// | Run not found | `NotFound` |
/// | Run is closed | `ConstraintViolation` |
pub trait EventLog {
    /// Append an event to a stream
    ///
    /// Appends a new event and returns its sequence number.
    ///
    /// ## Semantics
    ///
    /// - Creates stream if it doesn't exist
    /// - Assigns next sequence number in the stream
    /// - Event is immutable once appended
    ///
    /// ## Return Value
    ///
    /// Returns `Version::Sequence(n)` where `n` is the event's sequence number.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Stream name is invalid
    /// - `ConstraintViolation`: Payload is not Object, or run is closed
    /// - `NotFound`: Run does not exist
    fn event_append(
        &self,
        run: &ApiRunId,
        stream: &str,
        payload: Value,
    ) -> StrataResult<Version>;

    /// Read events from a stream
    ///
    /// Returns events within the specified range, in sequence order.
    ///
    /// ## Parameters
    ///
    /// - `start`: Start sequence (inclusive), `None` = from beginning
    /// - `end`: End sequence (inclusive), `None` = to end
    /// - `limit`: Maximum events to return, `None` = no limit
    ///
    /// ## Return Value
    ///
    /// Vector of `Versioned<Value>` in ascending sequence order (oldest first).
    ///
    /// ## Pagination
    ///
    /// Use `start` and `limit` for pagination:
    /// 1. First page: `range(run, stream, None, None, Some(100))`
    /// 2. Next page: `range(run, stream, Some(last_seq + 1), None, Some(100))`
    ///
    /// ## Performance Note
    ///
    /// Without bounds, this can be expensive for large streams.
    /// Always use `limit` in production.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Stream name is invalid
    /// - `NotFound`: Run does not exist
    fn event_range(
        &self,
        run: &ApiRunId,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    ) -> StrataResult<Vec<Versioned<Value>>>;

    /// Get a specific event by sequence number
    ///
    /// Returns the event at the specified sequence number.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Stream name is invalid
    /// - `NotFound`: Run or event does not exist
    /// - `HistoryTrimmed`: Event has been garbage collected
    fn event_get(
        &self,
        run: &ApiRunId,
        stream: &str,
        sequence: u64,
    ) -> StrataResult<Option<Versioned<Value>>>;

    /// Get the count of events in a stream
    ///
    /// Returns the total number of events in the stream.
    ///
    /// ## Return Value
    ///
    /// - `0` if stream doesn't exist or is empty
    /// - Count of events otherwise
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Stream name is invalid
    /// - `NotFound`: Run does not exist
    fn event_len(&self, run: &ApiRunId, stream: &str) -> StrataResult<u64>;

    /// Get the latest sequence number in a stream
    ///
    /// Returns the highest sequence number in the stream, or `None` if empty.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Stream name is invalid
    /// - `NotFound`: Run does not exist
    fn event_latest_sequence(&self, run: &ApiRunId, stream: &str) -> StrataResult<Option<u64>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn EventLog) {}
    }
}
