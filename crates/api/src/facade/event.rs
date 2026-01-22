//! Event Facade - Simplified event log operations
//!
//! This module provides Redis Streams-like operations for event logs.
//!
//! ## Desugaring
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `xadd(stream, payload)` | `event_append(default_run, stream, payload)` |
//! | `xrange(stream, start, end)` | `event_range(default_run, stream, start, end, None)` |
//! | `xlen(stream)` | `event_len(default_run, stream)` |

use strata_core::{StrataResult, Value};

/// Event entry returned from reads
#[derive(Debug, Clone)]
pub struct EventEntry {
    /// Sequence number
    pub sequence: u64,
    /// Event payload
    pub payload: Value,
    /// Timestamp (microseconds since epoch)
    pub timestamp: u64,
}

/// Event Facade - simplified stream operations
///
/// Mirrors Redis Streams-style operations with implicit default run.
///
/// ## Note
/// Events are immutable once appended (append-only log).
pub trait EventFacade {
    /// Append an event to a stream
    ///
    /// Returns the sequence number of the new event.
    ///
    /// ## Payload
    /// Must be a `Value::Object`. Empty objects `{}` are allowed.
    ///
    /// ## Example
    /// ```ignore
    /// use std::collections::HashMap;
    ///
    /// let payload = Value::Object(HashMap::from([
    ///     ("action".to_string(), Value::String("login".to_string())),
    ///     ("user_id".to_string(), Value::Int(42)),
    /// ]));
    ///
    /// let seq = facade.xadd("events", payload)?;
    /// ```
    fn xadd(&self, stream: &str, payload: Value) -> StrataResult<u64>;

    /// Read events in a range
    ///
    /// Returns events between `start` and `end` (inclusive).
    /// Use `None` for open-ended ranges.
    ///
    /// ## Example
    /// ```ignore
    /// // Get all events
    /// let all = facade.xrange("events", None, None)?;
    ///
    /// // Get events from sequence 100 onwards
    /// let recent = facade.xrange("events", Some(100), None)?;
    ///
    /// // Get first 10 events
    /// let first10 = facade.xrange_count("events", None, None, 10)?;
    /// ```
    fn xrange(&self, stream: &str, start: Option<u64>, end: Option<u64>)
        -> StrataResult<Vec<EventEntry>>;

    /// Read events with limit
    ///
    /// Like `xrange` but with a maximum count.
    fn xrange_count(
        &self,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
        count: u64,
    ) -> StrataResult<Vec<EventEntry>>;

    /// Read events in reverse order
    ///
    /// Like `xrange` but returns newest first.
    fn xrevrange(
        &self,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
    ) -> StrataResult<Vec<EventEntry>>;

    /// Get the length of a stream
    ///
    /// Returns 0 if stream doesn't exist.
    fn xlen(&self, stream: &str) -> StrataResult<u64>;

    /// Get the latest sequence number
    ///
    /// Returns `None` if stream is empty.
    fn xlast(&self, stream: &str) -> StrataResult<Option<u64>>;

    /// Get a single event by sequence
    fn xget(&self, stream: &str, sequence: u64) -> StrataResult<Option<EventEntry>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn EventFacade) {}
    }
}
