//! WAL (Write-Ahead Log) module
//!
//! This module contains both the legacy single-file WAL and the new segmented WAL:
//!
//! - `legacy`: Original single-file WAL (WALEntry, WAL, DurabilityMode)
//! - `config`: WAL configuration (WalConfig, WalConfigError)
//! - `durability`: Extended durability modes (InMemory/Strict/Batched/Async)
//! - `writer`: Segmented WAL writer (WalWriter)
//! - `reader`: Segmented WAL reader (WalReader)

pub mod legacy;
pub mod config;
pub mod durability;
pub mod reader;
pub mod writer;

// Backward-compatible re-exports (unchanged API)
pub use legacy::{DurabilityMode, WalCorruptionInfo, WalReadResult, WALEntry, WAL};

// New segmented WAL types (no name conflicts at this level)
pub use config::{WalConfig, WalConfigError};
pub use reader::{TruncateInfo, WalReader, WalReaderError};
pub use writer::WalWriter;
// Note: reader::WalReadResult and durability::DurabilityMode NOT re-exported
// here to avoid conflicts. Access via full path: crate::wal::reader::WalReadResult
// or crate::wal::durability::DurabilityMode
