//! Write-Ahead Log (WAL) module.
//!
//! This module provides the WAL infrastructure for M10 storage:
//!
//! - **Segment-based storage**: WAL is split into segments (`wal-NNNNNN.seg`)
//! - **Self-delimiting records**: Each record has length prefix and CRC32 checksum
//! - **Durability modes**: InMemory, Batched, Strict, Async
//! - **Crash recovery**: Reader handles partial/corrupt records gracefully
//!
//! # Architecture
//!
//! ```text
//! WAL Directory Structure:
//! wal/
//! ├── wal-000001.seg   (closed, immutable)
//! ├── wal-000002.seg   (closed, immutable)
//! └── wal-000003.seg   (active, writable)
//! ```
//!
//! # Key Invariants (from M10 Architecture)
//!
//! - **S1**: WAL is append-only - records can only be appended, never modified
//! - **S2**: WAL segments are immutable once closed - only active segment is writable
//! - **S3**: WAL records are self-delimiting - each record contains length and checksum
//! - **S9**: Storage never assigns versions - versions come from engine
//!
//! # Usage
//!
//! ```ignore
//! use strata_storage::wal::{WalWriter, WalReader, WalConfig, DurabilityMode};
//!
//! // Write records
//! let mut writer = WalWriter::new(
//!     wal_dir,
//!     database_uuid,
//!     DurabilityMode::Strict,
//!     WalConfig::default(),
//!     codec,
//! )?;
//! writer.append(&record)?;
//!
//! // Read records (for recovery)
//! let reader = WalReader::new(codec);
//! let result = reader.read_all(&wal_dir)?;
//! ```

pub mod config;
mod durability;
pub mod reader;
pub mod writer;

pub use config::{WalConfig, WalConfigError};
pub use durability::DurabilityMode;
pub use reader::{TruncateInfo, WalReadResult, WalReader, WalReaderError};
pub use writer::WalWriter;
