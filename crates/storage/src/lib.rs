//! Storage layer for Strata
//!
//! This crate implements the unified storage backend with:
//! - UnifiedStore: BTreeMap-based storage with RwLock
//! - ShardedStore: DashMap + HashMap performance
//! - Secondary indices (run_index, type_index)
//! - TTL index for expiration
//! - TTL cleaner background task
//! - Version management with AtomicU64
//! - ClonedSnapshotView implementation
//!
//!
//!
//! The `ShardedStore` provides improved concurrency:
//! - Lock-free reads via DashMap
//! - Per-RunId sharding (no cross-run contention)
//! - FxHashMap for O(1) lookups
//!
//! # Disk Storage
//!
//! The storage layer includes disk-based persistence:
//! - **WAL**: Write-ahead log with durability modes and segment rotation
//! - **Format**: On-disk byte formats for WAL records, writesets
//! - **Codec**: Codec seam for future encryption-at-rest
//!
//! See the `wal`, `format`, and `codec` modules for disk storage.

#![warn(missing_docs)]
#![warn(clippy::all)]

// In-memory storage 
pub mod cleaner;
pub mod index;
pub mod primitive_ext;
pub mod registry;
pub mod sharded;
pub mod snapshot;
pub mod stored_value;
pub mod ttl;
pub mod unified;

// Disk storage
pub mod codec;
pub mod compaction;
pub mod database;
pub mod disk_snapshot;
pub mod format;
pub mod recovery;
pub mod retention;
pub mod testing;
pub mod wal;

// In-memory storage re-exports
pub use cleaner::TTLCleaner;
pub use index::{RunIndex, TypeIndex};
pub use primitive_ext::{
    is_future_wal_type, is_vector_wal_type, primitive_for_wal_type, primitive_type_ids, wal_ranges,
    PrimitiveExtError, PrimitiveStorageExt,
};
pub use registry::PrimitiveRegistry;
pub use sharded::{Shard, ShardedSnapshot, ShardedStore};
pub use snapshot::ClonedSnapshotView;
pub use ttl::TTLIndex;
pub use unified::UnifiedStore;

// Disk storage re-exports
pub use codec::{get_codec, CodecError, IdentityCodec, StorageCodec};
pub use database::{
    export_database, import_database, ConfigError, DatabaseConfig, DatabaseHandle,
    DatabaseHandleError, DatabasePathError, DatabasePaths, ExportInfo,
};
pub use disk_snapshot::{
    CheckpointCoordinator, CheckpointData, CheckpointError, LoadedSection, LoadedSnapshot,
    SnapshotInfo, SnapshotReadError, SnapshotReader, SnapshotSection, SnapshotWriter,
};
pub use format::{
    // Snapshot format
    find_latest_snapshot,
    list_snapshots,
    parse_snapshot_id,
    primitive_tags,
    snapshot_path,
    // Watermark tracking
    CheckpointInfo,
    // Primitive serialization
    EventSnapshotEntry,
    JsonSnapshotEntry,
    KvSnapshotEntry,
    // MANIFEST format
    Manifest,
    ManifestError,
    ManifestManager,
    // WAL format
    Mutation,
    PrimitiveSerializeError,
    RunSnapshotEntry,
    SectionHeader,
    SegmentHeader,
    SnapshotHeader,
    SnapshotHeaderError,
    SnapshotSerializer,
    SnapshotWatermark,
    StateSnapshotEntry,
    VectorCollectionSnapshotEntry,
    VectorSnapshotEntry,
    WalRecord,
    WalRecordError,
    WalSegment,
    WatermarkError,
    Writeset,
    WritesetError,
    MANIFEST_FORMAT_VERSION,
    MANIFEST_MAGIC,
    SEGMENT_FORMAT_VERSION,
    SEGMENT_HEADER_SIZE,
    SEGMENT_MAGIC,
    SNAPSHOT_FORMAT_VERSION,
    SNAPSHOT_HEADER_SIZE,
    SNAPSHOT_MAGIC,
    WAL_RECORD_FORMAT_VERSION,
};
pub use recovery::{
    RecoveryCoordinator, RecoveryError, RecoveryPlan, RecoveryResult, RecoverySnapshot,
    ReplayStats, WalReplayError, WalReplayer,
};
pub use retention::{CompositeBuilder, RetentionPolicy, RetentionPolicyError};
pub use compaction::{
    CompactInfo, CompactMode, CompactionError, Tombstone, TombstoneError, TombstoneIndex,
    TombstoneReason, WalOnlyCompactor,
};
pub use testing::{
    CorruptionResult, CrashConfig, CrashPoint, CrashTestError, CrashTestResult, CrashType,
    DataState, GarbageResult, Operation, RecoveryVerification, ReferenceModel, StateMismatch,
    TruncationResult, VerificationResult, WalCorruptionTester,
};
pub use wal::{
    DurabilityMode, TruncateInfo, WalConfig, WalConfigError, WalReadResult, WalReader,
    WalReaderError, WalWriter,
};
