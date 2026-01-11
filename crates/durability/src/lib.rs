//! Durability layer for in-mem
//!
//! This crate implements write-ahead logging and snapshots:
//! - WAL: Append-only write-ahead log
//! - WALEntry types: BeginTxn, Write, Delete, CommitTxn, etc.
//! - Entry encoding/decoding with CRC32 checksums
//! - Durability modes: Strict, Batched (default), Async
//! - Snapshot creation and loading
//! - Recovery: Replay WAL from last snapshot

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod encoding; // Story #18: Entry encoding/decoding with CRC
pub mod recovery; // Story #23: WAL replay logic
pub mod wal; // Story #17-20: WALEntry types, File operations, Durability modes

// Stubs for future stories
// pub mod snapshot;   // M4

// Re-export commonly used types
pub use encoding::{decode_entry, encode_entry};
pub use recovery::{
    replay_wal, validate_transactions, ReplayStats, ValidationResult, ValidationWarning,
};
pub use wal::{DurabilityMode, WALEntry, WAL};
