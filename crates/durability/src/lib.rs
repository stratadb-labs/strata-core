//! Durability layer for Strata
//!
//! This crate implements write-ahead logging and snapshots:
//! - WAL: Append-only write-ahead log
//! - WALEntry types: BeginTxn, Write, Delete, CommitTxn, etc.
//! - Entry encoding/decoding with CRC32 checksums
//! - Durability modes: Strict, Batched (default), Async
//! - Snapshot creation and loading
//! - Recovery: Replay WAL from last snapshot
//!
//! ## Durability Enhancements
//!
//! - WAL entry type registry with extensible ranges
//! - Transaction framing with commit markers
//! - Self-validating entries with CRC32
//! - Snapshot format with envelope, header, and checksums

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod encoding; // Entry encoding/decoding with CRC
pub mod recovery; // WAL replay logic
pub mod recovery_manager; // Crash Recovery
pub mod run_lifecycle; // Run Lifecycle WAL Operations
pub mod snapshot; // Snapshot writer and serialization
pub mod snapshot_types; // Snapshot envelope and header types
pub mod transaction_log; // Cross-Primitive Transaction Grouping
pub mod wal; // WALEntry types, File operations, Durability modes
pub mod wal_entry_types; // WAL Entry Type Registry
pub mod wal_manager; // WAL Truncation
pub mod wal_reader; // WAL Corruption Detection
pub mod wal_types; // WAL Entry Envelope with CRC32
pub mod wal_writer; // Transaction Framing

// Re-export commonly used types
pub use encoding::{decode_entry, encode_entry};
pub use recovery::{
    replay_wal, replay_wal_with_options, validate_transactions, ReplayOptions, ReplayProgress,
    ReplayStats, ValidationResult, ValidationWarning,
};
pub use recovery_manager::{
    CommittedTransactions, RecoveryEngine, RecoveryError, RecoveryOptions, RecoveryResult,
    SnapshotDiscovery, WalReplayResultPublic,
};
pub use run_lifecycle::{
    create_run_begin_entry, create_run_end_entry, now_micros as run_now_micros,
    parse_run_begin_payload, parse_run_end_payload, RunBeginPayload, RunEndPayload,
    RUN_BEGIN_PAYLOAD_SIZE, RUN_END_PAYLOAD_SIZE,
};
pub use snapshot::{
    deserialize_primitives, serialize_all_primitives, SnapshotReader, SnapshotSerializable,
    SnapshotWriter,
};
pub use snapshot_types::{
    now_micros, primitive_ids, PrimitiveSection, SnapshotEnvelope, SnapshotError, SnapshotHeader,
    SnapshotInfo, SNAPSHOT_HEADER_SIZE, SNAPSHOT_MAGIC, SNAPSHOT_VERSION_1,
};
pub use transaction_log::{Transaction, TxEntry};
pub use wal::{DurabilityMode, WALEntry as LegacyWALEntry, WAL};
pub use wal_entry_types::{PrimitiveKind, WalEntryType, WalEntryTypeError};
pub use wal_manager::{WalManager, WalStats};
pub use wal_reader::WalReader;
pub use wal_types::{TxId, WalEntry, WalEntryError, MAX_WAL_ENTRY_SIZE, WAL_FORMAT_VERSION};
pub use wal_writer::WalWriter;
