//! Database directory structure
//!
//! A database is a portable directory containing all state.
//! The directory structure is:
//!
//! ```text
//! database.db/
//! ├── MANIFEST         # Physical metadata
//! ├── WAL/             # Write-ahead log segments
//! │   ├── wal-000001.seg
//! │   └── ...
//! ├── SNAPSHOTS/       # Point-in-time snapshots
//! │   ├── snap-000001.chk
//! │   └── ...
//! └── DATA/            # Reserved for future use
//! ```

use std::path::{Path, PathBuf};

/// Database directory paths
///
/// Provides access to all paths within a database directory.
#[derive(Debug, Clone)]
pub struct DatabasePaths {
    /// Root database directory
    root: PathBuf,
}

impl DatabasePaths {
    /// Create paths from root directory
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        DatabasePaths {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Get the root database directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the MANIFEST file path
    pub fn manifest(&self) -> PathBuf {
        self.root.join("MANIFEST")
    }

    /// Get the WAL directory
    pub fn wal_dir(&self) -> PathBuf {
        self.root.join("WAL")
    }

    /// Get the snapshots directory
    pub fn snapshots_dir(&self) -> PathBuf {
        self.root.join("SNAPSHOTS")
    }

    /// Get the data directory (reserved for future use)
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("DATA")
    }

    /// Check if database exists at this path
    ///
    /// A database exists if the MANIFEST file is present.
    pub fn exists(&self) -> bool {
        self.manifest().exists()
    }

    /// Create the full directory structure
    pub fn create_directories(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.wal_dir())?;
        std::fs::create_dir_all(self.snapshots_dir())?;
        std::fs::create_dir_all(self.data_dir())?;
        Ok(())
    }

    /// Validate that all required directories exist
    pub fn validate(&self) -> Result<(), DatabasePathError> {
        if !self.root.exists() {
            return Err(DatabasePathError::NotFound {
                path: self.root.clone(),
            });
        }

        if !self.manifest().exists() {
            return Err(DatabasePathError::MissingManifest {
                path: self.manifest(),
            });
        }

        if !self.wal_dir().exists() {
            return Err(DatabasePathError::MissingWalDir {
                path: self.wal_dir(),
            });
        }

        if !self.snapshots_dir().exists() {
            return Err(DatabasePathError::MissingSnapshotsDir {
                path: self.snapshots_dir(),
            });
        }

        Ok(())
    }
}

/// Database path validation errors
#[derive(Debug, thiserror::Error)]
pub enum DatabasePathError {
    /// Database not found at path
    #[error("Database not found at {path}")]
    NotFound {
        /// Path that was checked
        path: PathBuf,
    },

    /// Missing MANIFEST file
    #[error("Missing MANIFEST at {path}")]
    MissingManifest {
        /// Expected MANIFEST path
        path: PathBuf,
    },

    /// Missing WAL directory
    #[error("Missing WAL directory at {path}")]
    MissingWalDir {
        /// Expected WAL directory path
        path: PathBuf,
    },

    /// Missing SNAPSHOTS directory
    #[error("Missing SNAPSHOTS directory at {path}")]
    MissingSnapshotsDir {
        /// Expected snapshots directory path
        path: PathBuf,
    },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_paths_from_root() {
        let paths = DatabasePaths::from_root("/tmp/test.db");

        assert_eq!(paths.root(), Path::new("/tmp/test.db"));
        assert_eq!(paths.manifest(), PathBuf::from("/tmp/test.db/MANIFEST"));
        assert_eq!(paths.wal_dir(), PathBuf::from("/tmp/test.db/WAL"));
        assert_eq!(
            paths.snapshots_dir(),
            PathBuf::from("/tmp/test.db/SNAPSHOTS")
        );
        assert_eq!(paths.data_dir(), PathBuf::from("/tmp/test.db/DATA"));
    }

    #[test]
    fn test_exists_false() {
        let paths = DatabasePaths::from_root("/nonexistent/path");
        assert!(!paths.exists());
    }

    #[test]
    fn test_exists_true() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let paths = DatabasePaths::from_root(&db_path);

        // Not exists yet
        assert!(!paths.exists());

        // Create directories and manifest
        paths.create_directories().unwrap();
        std::fs::write(paths.manifest(), b"test").unwrap();

        // Now exists
        assert!(paths.exists());
    }

    #[test]
    fn test_create_directories() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let paths = DatabasePaths::from_root(&db_path);

        paths.create_directories().unwrap();

        assert!(paths.root().exists());
        assert!(paths.wal_dir().exists());
        assert!(paths.snapshots_dir().exists());
        assert!(paths.data_dir().exists());
    }

    #[test]
    fn test_validate_not_found() {
        let paths = DatabasePaths::from_root("/nonexistent/path");
        let result = paths.validate();
        assert!(matches!(result, Err(DatabasePathError::NotFound { .. })));
    }

    #[test]
    fn test_validate_missing_manifest() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let paths = DatabasePaths::from_root(&db_path);

        paths.create_directories().unwrap();
        // Don't create MANIFEST

        let result = paths.validate();
        assert!(matches!(
            result,
            Err(DatabasePathError::MissingManifest { .. })
        ));
    }

    #[test]
    fn test_validate_success() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let paths = DatabasePaths::from_root(&db_path);

        paths.create_directories().unwrap();
        std::fs::write(paths.manifest(), b"test").unwrap();

        assert!(paths.validate().is_ok());
    }
}
