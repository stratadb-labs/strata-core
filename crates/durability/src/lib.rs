//! Durability layer for in-mem
//!
//! This crate implements write-ahead logging and snapshots:
//! - WAL: Append-only write-ahead log
//! - WALEntry types: BeginTxn, Write, Delete, CommitTxn, etc.
//! - Entry encoding/decoding with CRC32 checksums
//! - Durability modes: Strict, Batched (default), Async
//! - Snapshot creation and loading
//! - Recovery: Replay WAL from last snapshot
//!
//! ## M7 Durability Enhancements
//!
//! - WAL entry type registry with extensible ranges
//! - Transaction framing with commit markers
//! - Self-validating entries with CRC32

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod encoding; // Story #18: Entry encoding/decoding with CRC
pub mod m7_wal_reader; // M7 Story #364: WAL Corruption Detection
pub mod m7_wal_types; // M7 Story #360: WAL Entry Envelope with CRC32
pub mod m7_wal_writer; // M7 Story #361: Transaction Framing
pub mod recovery; // Story #23: WAL replay logic
pub mod wal; // Story #17-20: WALEntry types, File operations, Durability modes
pub mod wal_entry_types; // M7 Story #362: WAL Entry Type Registry

// Stubs for future stories
// pub mod snapshot;   // M4

// Re-export commonly used types
pub use encoding::{decode_entry, encode_entry};
pub use m7_wal_reader::WalReader;
pub use m7_wal_types::{TxId, WalEntry, WalEntryError, M7_FORMAT_VERSION, MAX_WAL_ENTRY_SIZE};
pub use m7_wal_writer::WalWriter;
pub use recovery::{
    replay_wal, replay_wal_with_options, validate_transactions, ReplayOptions, ReplayProgress,
    ReplayStats, ValidationResult, ValidationWarning,
};
pub use wal::{DurabilityMode, WALEntry as LegacyWALEntry, WAL};
pub use wal_entry_types::{PrimitiveKind, WalEntryType, WalEntryTypeError};
