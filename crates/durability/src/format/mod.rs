//! On-disk byte formats for WAL, snapshots, and MANIFEST.
//!
//! This module centralizes all serialization logic for persistent storage.
//! Keeping serialization separate from operational logic (how WAL/snapshots
//! are managed) makes format evolution easier to manage.
//!
//! # Module Structure
//!
//! - `wal_record`: WAL segment header and record format
//! - `writeset`: Transaction writeset serialization
//! - `manifest`: MANIFEST file format (added in Epic 72)
//! - `snapshot`: Snapshot file format (added in Epic 71)

pub mod manifest;
pub mod primitives;
pub mod snapshot;
pub mod wal_record;
pub mod watermark;
pub mod writeset;

pub use snapshot::{
    parse_snapshot_id, primitive_tags, snapshot_path, find_latest_snapshot, list_snapshots,
    SectionHeader, SnapshotHeader, SnapshotHeaderError, SNAPSHOT_FORMAT_VERSION,
    SNAPSHOT_HEADER_SIZE, SNAPSHOT_MAGIC,
};
pub use wal_record::{
    SegmentHeader, WalRecord, WalRecordError, WalSegment, SEGMENT_FORMAT_VERSION,
    SEGMENT_HEADER_SIZE, SEGMENT_MAGIC, WAL_RECORD_FORMAT_VERSION,
};
pub use writeset::{Mutation, Writeset, WritesetError};

pub use primitives::{
    EventSnapshotEntry, JsonSnapshotEntry, KvSnapshotEntry, PrimitiveSerializeError,
    RunSnapshotEntry, SnapshotSerializer, StateSnapshotEntry,
    VectorCollectionSnapshotEntry, VectorSnapshotEntry,
};
pub use watermark::{CheckpointInfo, SnapshotWatermark, WatermarkError};
pub use manifest::{Manifest, ManifestError, ManifestManager, MANIFEST_FORMAT_VERSION, MANIFEST_MAGIC};
