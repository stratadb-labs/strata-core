//! Durability layer for Strata
//!
//! This crate implements write-ahead logging and snapshots:
//!
//! - WAL: Append-only write-ahead log with MVCC support
//! - WALEntry types: BeginTxn, Write, Delete, CommitTxn, etc.
//! - Entry encoding/decoding with CRC32 checksums
//! - Durability modes: Strict, Batched (default), None
//! - Snapshot creation and loading
//! - Recovery: Replay WAL from last snapshot
//!
//! ## WAL Entry Types
//!
//! The `WalEntryType` enum provides a standardized registry of entry types
//! organized by primitive (KV, JSON, Event, State, Run, Vector).

// Allow deprecated SnapshotSerializable usage (will be removed in future refactor)
#![allow(deprecated)]
#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod encoding; // Entry encoding/decoding with CRC
pub mod recovery; // WAL replay logic
pub mod run_bundle; // Portable execution artifacts (RunBundle)
pub mod snapshot; // Snapshot writer and serialization
pub mod snapshot_types; // Snapshot envelope and header types
pub mod wal; // WALEntry types, File operations, Durability modes
pub mod wal_entry_types; // WAL Entry Type Registry

// Re-export commonly used types
pub use encoding::{decode_entry, encode_entry};
pub use recovery::{
    replay_wal, replay_wal_with_options, validate_transactions, ReplayOptions, ReplayProgress,
    ReplayStats, ValidationResult, ValidationWarning,
};
pub use snapshot::{
    deserialize_primitives, serialize_all_primitives, SnapshotReader, SnapshotSerializable,
    SnapshotWriter,
};
pub use snapshot_types::{
    now_micros, primitive_ids, PrimitiveSection, SnapshotEnvelope, SnapshotError, SnapshotHeader,
    SnapshotInfo, SNAPSHOT_HEADER_SIZE, SNAPSHOT_MAGIC, SNAPSHOT_VERSION_1,
};
pub use wal::{DurabilityMode, WalCorruptionInfo, WalReadResult, WALEntry, WAL};
pub use wal_entry_types::{WalEntryType, WalEntryTypeError};

// RunBundle types
pub use run_bundle::{
    filter_wal_for_run, BundleContents, BundleManifest, BundleRunInfo, BundleVerifyInfo,
    ExportOptions, ImportedRunInfo, ReadBundleContents, RunBundleError, RunBundleReader,
    RunBundleResult, RunBundleWriter, RunExportInfo, WalLogInfo, WalLogIterator, WalLogReader,
    WalLogWriter, RUNBUNDLE_EXTENSION, RUNBUNDLE_FORMAT_VERSION,
};
