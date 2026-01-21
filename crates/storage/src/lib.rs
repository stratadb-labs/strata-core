//! Storage layer for in-mem
//!
//! This crate implements the unified storage backend with:
//! - UnifiedStore: BTreeMap-based storage with RwLock (M3)
//! - ShardedStore: DashMap + HashMap for M4 performance
//! - Secondary indices (run_index, type_index)
//! - TTL index for expiration
//! - TTL cleaner background task
//! - Version management with AtomicU64
//! - ClonedSnapshotView implementation
//!
//! # M4 Performance
//!
//! The `ShardedStore` provides improved concurrency:
//! - Lock-free reads via DashMap
//! - Per-RunId sharding (no cross-run contention)
//! - FxHashMap for O(1) lookups
//!
//! # M10 Disk Storage
//!
//! The storage layer includes disk-based persistence:
//! - **WAL**: Write-ahead log with durability modes and segment rotation
//! - **Format**: On-disk byte formats for WAL records, writesets
//! - **Codec**: Codec seam for future encryption-at-rest
//!
//! See the `wal`, `format`, and `codec` modules for disk storage.

#![warn(missing_docs)]
#![warn(clippy::all)]

// In-memory storage (M3/M4)
pub mod cleaner;
pub mod index;
pub mod primitive_ext;
pub mod registry;
pub mod sharded;
pub mod snapshot;
pub mod stored_value;
pub mod ttl;
pub mod unified;

// Disk storage (M10)
pub mod codec;
pub mod disk_snapshot;
pub mod format;
pub mod recovery;
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

// Disk storage re-exports (M10)
pub use codec::{get_codec, CodecError, IdentityCodec, StorageCodec};
pub use format::{
    // Snapshot format
    find_latest_snapshot, list_snapshots, parse_snapshot_id, primitive_tags, snapshot_path,
    SectionHeader, SnapshotHeader, SnapshotHeaderError, SNAPSHOT_FORMAT_VERSION,
    SNAPSHOT_HEADER_SIZE, SNAPSHOT_MAGIC,
    // Primitive serialization
    EventSnapshotEntry, JsonSnapshotEntry, KvSnapshotEntry, PrimitiveSerializeError,
    RunSnapshotEntry, SnapshotSerializer, SpanSnapshotEntry, StateSnapshotEntry,
    TraceSnapshotEntry, VectorCollectionSnapshotEntry, VectorSnapshotEntry,
    // Watermark tracking
    CheckpointInfo, SnapshotWatermark, WatermarkError,
    // WAL format
    Mutation, SegmentHeader, WalRecord, WalRecordError, WalSegment, Writeset, WritesetError,
    SEGMENT_FORMAT_VERSION, SEGMENT_HEADER_SIZE, SEGMENT_MAGIC, WAL_RECORD_FORMAT_VERSION,
    // MANIFEST format
    Manifest, ManifestError, ManifestManager, MANIFEST_FORMAT_VERSION, MANIFEST_MAGIC,
};
pub use wal::{
    DurabilityMode, TruncateInfo, WalConfig, WalConfigError, WalReadResult, WalReader,
    WalReaderError, WalWriter,
};
pub use disk_snapshot::{
    CheckpointCoordinator, CheckpointData, CheckpointError, LoadedSection, LoadedSnapshot,
    SnapshotInfo, SnapshotReadError, SnapshotReader, SnapshotSection, SnapshotWriter,
};
pub use recovery::{
    RecoveryCoordinator, RecoveryError, RecoveryPlan, RecoveryResult, RecoverySnapshot,
    ReplayStats, WalReplayError, WalReplayer,
};
