//! Run Lifecycle WAL Operations
//!
//! This module provides WAL entry serialization/deserialization for run lifecycle events.
//!
//! ## Run Lifecycle Events
//!
//! - `RunBegin`: Marks the start of a run
//! - `RunEnd`: Marks the end of a run (normal completion)
//!
//! ## Payload Format
//!
//! ### RunBegin Payload
//! ```text
//! [run_id: 16 bytes][timestamp: 8 bytes]
//! ```
//!
//! ### RunEnd Payload
//! ```text
//! [run_id: 16 bytes][timestamp: 8 bytes][event_count: 8 bytes]
//! ```

use crate::wal_entry_types::WalEntryType;
use crate::wal_types::{TxId, WalEntry, WalEntryError};
use strata_core::types::RunId;

/// Payload size for RunBegin entry
pub const RUN_BEGIN_PAYLOAD_SIZE: usize = 24; // 16 bytes run_id + 8 bytes timestamp

/// Payload size for RunEnd entry
pub const RUN_END_PAYLOAD_SIZE: usize = 32; // 16 bytes run_id + 8 bytes timestamp + 8 bytes event_count

/// Create a RunBegin WAL entry
///
/// This entry is written when a new run is started. It records:
/// - The run ID (16 bytes UUID)
/// - The start timestamp (8 bytes, microseconds since epoch)
pub fn create_run_begin_entry(run_id: RunId, timestamp_micros: u64) -> WalEntry {
    let mut payload = Vec::with_capacity(RUN_BEGIN_PAYLOAD_SIZE);
    payload.extend_from_slice(run_id.as_bytes());
    payload.extend_from_slice(&timestamp_micros.to_le_bytes());

    WalEntry::new(WalEntryType::RunBegin, TxId::nil(), payload)
}

/// Create a RunEnd WAL entry
///
/// This entry is written when a run completes normally. It records:
/// - The run ID (16 bytes UUID)
/// - The end timestamp (8 bytes, microseconds since epoch)
/// - The total event count (8 bytes)
pub fn create_run_end_entry(run_id: RunId, timestamp_micros: u64, event_count: u64) -> WalEntry {
    let mut payload = Vec::with_capacity(RUN_END_PAYLOAD_SIZE);
    payload.extend_from_slice(run_id.as_bytes());
    payload.extend_from_slice(&timestamp_micros.to_le_bytes());
    payload.extend_from_slice(&event_count.to_le_bytes());

    WalEntry::new(WalEntryType::RunEnd, TxId::nil(), payload)
}

/// Parsed RunBegin payload
#[derive(Debug, Clone, PartialEq)]
pub struct RunBeginPayload {
    /// Run ID
    pub run_id: RunId,
    /// Start timestamp (microseconds since epoch)
    pub timestamp_micros: u64,
}

/// Parsed RunEnd payload
#[derive(Debug, Clone, PartialEq)]
pub struct RunEndPayload {
    /// Run ID
    pub run_id: RunId,
    /// End timestamp (microseconds since epoch)
    pub timestamp_micros: u64,
    /// Total number of events recorded during the run
    pub event_count: u64,
}

/// Parse a RunBegin payload
pub fn parse_run_begin_payload(payload: &[u8]) -> Result<RunBeginPayload, WalEntryError> {
    if payload.len() < RUN_BEGIN_PAYLOAD_SIZE {
        return Err(WalEntryError::Deserialization {
            offset: 0,
            message: format!(
                "RunBegin payload too short: {} bytes, expected {}",
                payload.len(),
                RUN_BEGIN_PAYLOAD_SIZE
            ),
        });
    }

    let mut run_id_bytes = [0u8; 16];
    run_id_bytes.copy_from_slice(&payload[0..16]);
    let run_id = RunId::from_bytes(run_id_bytes);

    let mut timestamp_bytes = [0u8; 8];
    timestamp_bytes.copy_from_slice(&payload[16..24]);
    let timestamp_micros = u64::from_le_bytes(timestamp_bytes);

    Ok(RunBeginPayload {
        run_id,
        timestamp_micros,
    })
}

/// Parse a RunEnd payload
pub fn parse_run_end_payload(payload: &[u8]) -> Result<RunEndPayload, WalEntryError> {
    if payload.len() < RUN_END_PAYLOAD_SIZE {
        return Err(WalEntryError::Deserialization {
            offset: 0,
            message: format!(
                "RunEnd payload too short: {} bytes, expected {}",
                payload.len(),
                RUN_END_PAYLOAD_SIZE
            ),
        });
    }

    let mut run_id_bytes = [0u8; 16];
    run_id_bytes.copy_from_slice(&payload[0..16]);
    let run_id = RunId::from_bytes(run_id_bytes);

    let mut timestamp_bytes = [0u8; 8];
    timestamp_bytes.copy_from_slice(&payload[16..24]);
    let timestamp_micros = u64::from_le_bytes(timestamp_bytes);

    let mut event_count_bytes = [0u8; 8];
    event_count_bytes.copy_from_slice(&payload[24..32]);
    let event_count = u64::from_le_bytes(event_count_bytes);

    Ok(RunEndPayload {
        run_id,
        timestamp_micros,
        event_count,
    })
}

/// Get current timestamp in microseconds since Unix epoch
pub fn now_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_begin_entry_roundtrip() {
        let run_id = RunId::new();
        let timestamp = 1234567890123456u64;

        let entry = create_run_begin_entry(run_id, timestamp);
        assert_eq!(entry.entry_type, WalEntryType::RunBegin);
        assert!(entry.tx_id.is_nil());
        assert_eq!(entry.payload.len(), RUN_BEGIN_PAYLOAD_SIZE);

        let parsed = parse_run_begin_payload(&entry.payload).unwrap();
        assert_eq!(parsed.run_id, run_id);
        assert_eq!(parsed.timestamp_micros, timestamp);
    }

    #[test]
    fn test_run_end_entry_roundtrip() {
        let run_id = RunId::new();
        let timestamp = 1234567890123456u64;
        let event_count = 42u64;

        let entry = create_run_end_entry(run_id, timestamp, event_count);
        assert_eq!(entry.entry_type, WalEntryType::RunEnd);
        assert!(entry.tx_id.is_nil());
        assert_eq!(entry.payload.len(), RUN_END_PAYLOAD_SIZE);

        let parsed = parse_run_end_payload(&entry.payload).unwrap();
        assert_eq!(parsed.run_id, run_id);
        assert_eq!(parsed.timestamp_micros, timestamp);
        assert_eq!(parsed.event_count, event_count);
    }

    #[test]
    fn test_run_begin_payload_too_short() {
        let short_payload = vec![0u8; 10];
        let result = parse_run_begin_payload(&short_payload);
        assert!(matches!(result, Err(WalEntryError::Deserialization { .. })));
    }

    #[test]
    fn test_run_end_payload_too_short() {
        let short_payload = vec![0u8; 20];
        let result = parse_run_end_payload(&short_payload);
        assert!(matches!(result, Err(WalEntryError::Deserialization { .. })));
    }

    #[test]
    fn test_now_micros() {
        let t1 = now_micros();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let t2 = now_micros();

        // t2 should be greater than t1
        assert!(t2 > t1);

        // Should be reasonable (after year 2020)
        let year_2020_micros = 1577836800_000_000u64;
        assert!(t1 > year_2020_micros);
    }

    #[test]
    fn test_wal_entry_serialize_roundtrip() {
        let run_id = RunId::new();
        let timestamp = now_micros();

        let entry = create_run_begin_entry(run_id, timestamp);
        let serialized = entry.serialize().unwrap();

        let (deserialized, consumed) = WalEntry::deserialize(&serialized, 0).unwrap();
        assert_eq!(consumed, serialized.len());
        assert_eq!(deserialized.entry_type, WalEntryType::RunBegin);

        let parsed = parse_run_begin_payload(&deserialized.payload).unwrap();
        assert_eq!(parsed.run_id, run_id);
        assert_eq!(parsed.timestamp_micros, timestamp);
    }
}
