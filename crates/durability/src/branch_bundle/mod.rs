//! BranchBundle — portable branch archive format (v2)
//!
//! This module implements export and import of Strata branches as portable,
//! immutable `.branchbundle.tar.zst` archives.
//!
//! ## Archive Structure
//!
//! ```text
//! archive.branchbundle.tar.zst
//! └── branchbundle/
//!     ├── MANIFEST.json   — format version, checksums
//!     ├── BRANCH.json     — branch metadata (name, state, timestamps)
//!     └── WAL.branchlog   — binary log of BranchlogPayload entries (msgpack v2)
//! ```
//!
//! ## Format Version
//!
//! v2 uses `BranchlogPayload` records (msgpack-serialized) instead of the legacy
//! `WALEntry` format (bincode). Each payload represents one committed transaction
//! with its puts and deletes.
//!
//! ## Usage
//!
//! Export a completed branch:
//! ```ignore
//! let info = db.export_branch(&branch_id, Path::new("./my-branch.branchbundle.tar.zst"))?;
//! ```
//!
//! Verify a bundle:
//! ```ignore
//! let info = db.verify_bundle(Path::new("./my-branch.branchbundle.tar.zst"))?;
//! ```
//!
//! Import into a database:
//! ```ignore
//! let info = db.import_branch(Path::new("./my-branch.branchbundle.tar.zst"))?;
//! ```
//!
//! ## Design Principles
//!
//! - **Explicit**: All operations are explicit, no background behavior
//! - **Immutable**: Only terminal branches (Completed, Failed, Cancelled, Archived) can be exported
//! - **Portable**: Archives can be moved between machines, stored in VCS
//! - **Inspectable**: Standard tools (tar, jq) can inspect contents
//! - **Deterministic**: Same branch exported twice produces identical bundles

pub mod error;
pub mod reader;
pub mod types;
pub mod wal_log;
pub mod writer;

// Re-export public types
pub use error::{BranchBundleError, BranchBundleResult};
pub use reader::{BranchBundleReader, BundleContents as ReadBundleContents};
pub use types::{
    paths, xxh3_hex, BranchExportInfo, BundleBranchInfo, BundleContents, BundleManifest,
    BundleVerifyInfo, ExportOptions, ImportedBranchInfo, BRANCHBUNDLE_EXTENSION,
    BRANCHBUNDLE_FORMAT_VERSION, WAL_BRANCHLOG_MAGIC, WAL_BRANCHLOG_VERSION,
};
pub use wal_log::{BranchlogPayload, WalLogInfo, WalLogIterator, WalLogReader, WalLogWriter};
pub use writer::BranchBundleWriter;
