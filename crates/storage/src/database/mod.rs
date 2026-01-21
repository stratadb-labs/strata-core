//! Database lifecycle management
//!
//! This module provides storage-layer building blocks for database lifecycle:
//!
//! - `DatabasePaths`: Directory structure management
//! - `DatabaseConfig`: Configuration for durability and codecs
//! - `DatabaseHandle`: Coordinates WAL, snapshots, and manifest
//!
//! # Example
//!
//! ```ignore
//! use strata_storage::database::{DatabaseHandle, DatabaseConfig};
//!
//! // Create a new database
//! let handle = DatabaseHandle::create("my.db", DatabaseConfig::default())?;
//!
//! // Write some data via WAL
//! handle.append_wal(&record)?;
//! handle.flush_wal()?;
//!
//! // Create a checkpoint
//! handle.checkpoint(watermark_txn, data)?;
//!
//! // Close cleanly
//! handle.close()?;
//! ```

pub mod config;
pub mod handle;
pub mod paths;

pub use config::{ConfigError, DatabaseConfig};
pub use handle::{DatabaseHandle, DatabaseHandleError};
pub use paths::{DatabasePathError, DatabasePaths};

use std::path::Path;

/// Export a database to a new location
///
/// This creates a checkpoint and copies the entire database directory.
/// The exported database is a complete, portable copy.
///
/// # Example
///
/// ```ignore
/// export_database("source.db", "backup.db", &config)?;
/// ```
pub fn export_database(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    config: &DatabaseConfig,
) -> Result<ExportInfo, DatabaseHandleError> {
    let dst = dst.as_ref();

    if dst.exists() {
        return Err(DatabaseHandleError::AlreadyExists {
            path: dst.to_path_buf(),
        });
    }

    // Open source database
    let handle = DatabaseHandle::open(src, config.clone())?;

    // Flush WAL to ensure consistency
    handle.flush_wal()?;

    // Get current watermark
    let watermark = handle.watermark().unwrap_or(0);

    // Copy entire directory
    copy_dir_recursive(handle.path(), dst)?;

    // Calculate size
    let size_bytes = dir_size(dst)?;

    // Close source handle
    handle.close()?;

    Ok(ExportInfo {
        path: dst.to_path_buf(),
        watermark,
        size_bytes,
    })
}

/// Import a database from an exported location
///
/// This is equivalent to `DatabaseHandle::open()` - an exported database
/// is a valid database that can be opened directly.
pub fn import_database(
    path: impl AsRef<Path>,
    config: DatabaseConfig,
) -> Result<DatabaseHandle, DatabaseHandleError> {
    DatabaseHandle::open(path, config)
}

/// Export result information
#[derive(Debug, Clone)]
pub struct ExportInfo {
    /// Path to exported database
    pub path: std::path::PathBuf,
    /// Watermark at time of export
    pub watermark: u64,
    /// Total size in bytes
    pub size_bytes: u64,
}

/// Copy a directory recursively
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Calculate total size of a directory
fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0;

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;

        if metadata.is_file() {
            total += metadata.len();
        } else if metadata.is_dir() {
            total += dir_size(&entry.path())?;
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_export_import() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let dst_path = dir.path().join("backup.db");

        // Create source database
        {
            let handle =
                DatabaseHandle::create(&src_path, DatabaseConfig::for_testing()).unwrap();
            handle.close().unwrap();
        }

        // Export
        let export_info =
            export_database(&src_path, &dst_path, &DatabaseConfig::for_testing()).unwrap();

        assert!(dst_path.exists());
        assert_eq!(export_info.path, dst_path);

        // Import (open)
        let handle = import_database(&dst_path, DatabaseConfig::for_testing()).unwrap();
        assert!(handle.path().exists());
        handle.close().unwrap();
    }

    #[test]
    fn test_export_already_exists() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let dst_path = dir.path().join("backup.db");

        // Create source
        {
            let handle =
                DatabaseHandle::create(&src_path, DatabaseConfig::for_testing()).unwrap();
            handle.close().unwrap();
        }

        // Create destination
        std::fs::create_dir_all(&dst_path).unwrap();

        // Export should fail
        let result = export_database(&src_path, &dst_path, &DatabaseConfig::for_testing());
        assert!(matches!(
            result,
            Err(DatabaseHandleError::AlreadyExists { .. })
        ));
    }

    #[test]
    fn test_dir_size() {
        let dir = tempdir().unwrap();
        let test_dir = dir.path().join("test");
        std::fs::create_dir_all(&test_dir).unwrap();

        // Create files of known sizes
        std::fs::write(test_dir.join("file1"), vec![0u8; 100]).unwrap();
        std::fs::write(test_dir.join("file2"), vec![0u8; 200]).unwrap();

        let size = dir_size(&test_dir).unwrap();
        assert_eq!(size, 300);
    }

    #[test]
    fn test_copy_dir_recursive() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");

        // Create source structure
        std::fs::create_dir_all(src.join("subdir")).unwrap();
        std::fs::write(src.join("file1.txt"), b"hello").unwrap();
        std::fs::write(src.join("subdir/file2.txt"), b"world").unwrap();

        // Copy
        copy_dir_recursive(&src, &dst).unwrap();

        // Verify
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir/file2.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dst.join("file1.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("subdir/file2.txt")).unwrap(),
            "world"
        );
    }
}
