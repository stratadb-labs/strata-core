//! RunBundle — portable run archive format (v2)
//!
//! This module implements export and import of Strata runs as portable,
//! immutable `.runbundle.tar.zst` archives.
//!
//! ## Archive Structure
//!
//! ```text
//! archive.runbundle.tar.zst
//! └── runbundle/
//!     ├── MANIFEST.json   — format version, checksums
//!     ├── RUN.json        — run metadata (name, state, timestamps, tags)
//!     └── WAL.runlog      — binary log of RunlogPayload entries (msgpack v2)
//! ```
//!
//! ## Format Version
//!
//! v2 uses `RunlogPayload` records (msgpack-serialized) instead of the legacy
//! `WALEntry` format (bincode). Each payload represents one committed transaction
//! with its puts and deletes.
//!
//! ## Usage
//!
//! Export a completed run:
//! ```ignore
//! let info = db.export_run(&branch_id, Path::new("./my-run.runbundle.tar.zst"))?;
//! ```
//!
//! Verify a bundle:
//! ```ignore
//! let info = db.verify_bundle(Path::new("./my-run.runbundle.tar.zst"))?;
//! ```
//!
//! Import into a database:
//! ```ignore
//! let info = db.import_run(Path::new("./my-run.runbundle.tar.zst"))?;
//! ```
//!
//! ## Design Principles
//!
//! - **Explicit**: All operations are explicit, no background behavior
//! - **Immutable**: Only terminal runs (Completed, Failed, Cancelled, Archived) can be exported
//! - **Portable**: Archives can be moved between machines, stored in VCS
//! - **Inspectable**: Standard tools (tar, jq) can inspect contents
//! - **Deterministic**: Same run exported twice produces identical bundles

pub mod error;
pub mod reader;
pub mod types;
pub mod wal_log;
pub mod writer;

// Re-export public types
pub use error::{RunBundleError, RunBundleResult};
pub use reader::{BundleContents as ReadBundleContents, RunBundleReader};
pub use types::{
    paths, xxh3_hex, BundleContents, BundleManifest, BundleRunInfo, BundleVerifyInfo,
    ExportOptions, ImportedRunInfo, RunExportInfo, RUNBUNDLE_EXTENSION, RUNBUNDLE_FORMAT_VERSION,
    WAL_RUNLOG_MAGIC, WAL_RUNLOG_VERSION,
};
pub use wal_log::{RunlogPayload, WalLogInfo, WalLogIterator, WalLogReader, WalLogWriter};
pub use writer::RunBundleWriter;
