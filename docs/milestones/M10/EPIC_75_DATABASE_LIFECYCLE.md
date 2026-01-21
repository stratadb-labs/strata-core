# Epic 75: Database Lifecycle

**Goal**: Implement database open/close and portability features

**Dependencies**: Epic 70 (WAL Infrastructure), Epic 71 (Snapshot System), Epic 72 (Recovery)

---

## Scope

- Database directory structure
- Database open (new and existing)
- Database close with proper cleanup
- DatabaseConfig type
- Export (convenience wrapper)
- Import (open exported artifact)

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #532 | Database Directory Structure | FOUNDATION |
| #533 | Database Open (New and Existing) | CRITICAL |
| #534 | Database Close | CRITICAL |
| #535 | DatabaseConfig Type | HIGH |
| #536 | Export (Convenience Wrapper) | HIGH |
| #537 | Import (Open Exported Artifact) | HIGH |

---

## Story #532: Database Directory Structure

**File**: `crates/storage/src/database.rs` (NEW)

**Deliverable**: Standard database directory layout

### Design

MVP database artifact is a portable directory (SQLite-like portability by copy):

```
strata.db/                    # Database root
├── MANIFEST                  # Physical metadata (atomic updates)
├── WAL/
│   ├── wal-000001.seg       # WAL segment files
│   ├── wal-000002.seg
│   └── ...
├── SNAPSHOTS/
│   ├── snap-000001.chk      # Snapshot checkpoint files
│   └── ...
└── DATA/                     # Reserved for future use
    └── ...
```

### Implementation

```rust
use std::path::{Path, PathBuf};

/// Database directory structure
pub struct DatabasePaths {
    /// Root database directory
    pub root: PathBuf,

    /// MANIFEST file path
    pub manifest: PathBuf,

    /// WAL directory
    pub wal_dir: PathBuf,

    /// Snapshots directory
    pub snapshots_dir: PathBuf,

    /// Data directory (reserved for future)
    pub data_dir: PathBuf,
}

impl DatabasePaths {
    /// Create paths from root directory
    pub fn from_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();

        DatabasePaths {
            manifest: root.join("MANIFEST"),
            wal_dir: root.join("WAL"),
            snapshots_dir: root.join("SNAPSHOTS"),
            data_dir: root.join("DATA"),
            root,
        }
    }

    /// Check if database exists at this path
    pub fn exists(&self) -> bool {
        self.manifest.exists()
    }

    /// Create directory structure
    pub fn create_directories(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(&self.wal_dir)?;
        std::fs::create_dir_all(&self.snapshots_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    /// Validate directory structure
    pub fn validate(&self) -> Result<(), DatabaseError> {
        if !self.root.exists() {
            return Err(DatabaseError::NotFound {
                path: self.root.clone(),
            });
        }

        if !self.manifest.exists() {
            return Err(DatabaseError::MissingManifest {
                path: self.manifest.clone(),
            });
        }

        if !self.wal_dir.exists() {
            return Err(DatabaseError::MissingWalDir {
                path: self.wal_dir.clone(),
            });
        }

        if !self.snapshots_dir.exists() {
            return Err(DatabaseError::MissingSnapshotsDir {
                path: self.snapshots_dir.clone(),
            });
        }

        Ok(())
    }
}

/// Database directory validation errors
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database not found at {path}")]
    NotFound { path: PathBuf },

    #[error("Missing MANIFEST at {path}")]
    MissingManifest { path: PathBuf },

    #[error("Missing WAL directory at {path}")]
    MissingWalDir { path: PathBuf },

    #[error("Missing SNAPSHOTS directory at {path}")]
    MissingSnapshotsDir { path: PathBuf },

    #[error("Database already exists at {path}")]
    AlreadyExists { path: PathBuf },

    #[error("Codec mismatch: database uses {database}, config specifies {config}")]
    CodecMismatch { database: String, config: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Recovery error: {0}")]
    Recovery(#[from] RecoveryError),

    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),
}
```

### Acceptance Criteria

- [ ] `DatabasePaths` struct with root, manifest, wal_dir, snapshots_dir, data_dir
- [ ] `from_root()` constructs paths from root directory
- [ ] `exists()` checks for MANIFEST presence
- [ ] `create_directories()` creates full structure
- [ ] `validate()` checks all required paths exist
- [ ] Error types for missing components

---

## Story #533: Database Open (New and Existing)

**File**: `crates/storage/src/database.rs`

**Deliverable**: Database open for new and existing databases

### Implementation

```rust
/// Database instance
pub struct Database {
    /// Database paths
    paths: DatabasePaths,

    /// Engine (in-memory state)
    engine: Engine,

    /// Manifest manager
    manifest: Arc<Mutex<ManifestManager>>,

    /// WAL writer
    wal_writer: Arc<Mutex<WalWriter>>,

    /// Snapshot writer
    snapshot_writer: Arc<SnapshotWriter>,

    /// Codec
    codec: Arc<dyn StorageCodec>,

    /// Configuration
    config: DatabaseConfig,

    /// Retention policy store
    retention_store: Arc<RetentionPolicyStore>,

    /// Compaction lock
    compaction_lock: Mutex<()>,

    /// Database UUID
    database_uuid: [u8; 16],
}

impl Database {
    /// Create a new database at the specified path
    ///
    /// Fails if database already exists at path.
    pub fn create(
        path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        let paths = DatabasePaths::from_root(path);

        // Check not already exists
        if paths.exists() {
            return Err(DatabaseError::AlreadyExists { path: paths.root });
        }

        // Create directory structure
        paths.create_directories()?;

        // Generate database UUID
        let database_uuid = uuid::Uuid::new_v4().into_bytes();

        // Get codec
        let codec = get_codec(&config.codec_id)
            .map_err(|e| DatabaseError::Storage(StorageError::Codec(e.to_string())))?;

        // Create MANIFEST
        let manifest = ManifestManager::create(
            paths.manifest.clone(),
            database_uuid,
            config.codec_id.clone(),
        )?;

        // Create WAL writer
        let wal_writer = WalWriter::new(
            paths.wal_dir.clone(),
            database_uuid,
            config.durability,
            config.wal_config.clone(),
        )?;

        // Create snapshot writer
        let snapshot_writer = SnapshotWriter::new(
            paths.snapshots_dir.clone(),
            codec.clone(),
            database_uuid,
        )?;

        // Initialize engine
        let engine = Engine::new();

        let db = Database {
            paths,
            engine,
            manifest: Arc::new(Mutex::new(manifest)),
            wal_writer: Arc::new(Mutex::new(wal_writer)),
            snapshot_writer: Arc::new(snapshot_writer),
            codec,
            config,
            retention_store: Arc::new(RetentionPolicyStore::new(/* ... */)),
            compaction_lock: Mutex::new(()),
            database_uuid,
        };

        Ok(db)
    }

    /// Open an existing database at the specified path
    ///
    /// Performs recovery if needed (loads snapshot, replays WAL).
    pub fn open(
        path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        let paths = DatabasePaths::from_root(path);

        // Validate structure
        paths.validate()?;

        // Get codec
        let codec = get_codec(&config.codec_id)
            .map_err(|e| DatabaseError::Storage(StorageError::Codec(e.to_string())))?;

        // Perform recovery
        let recovery = Recovery::new(paths.root.clone(), codec.clone());
        let result = recovery.recover()?;

        // Validate codec matches
        if result.manifest.codec_id != config.codec_id {
            return Err(DatabaseError::CodecMismatch {
                database: result.manifest.codec_id.clone(),
                config: config.codec_id.clone(),
            });
        }

        // Load manifest manager
        let manifest = ManifestManager::load(paths.manifest.clone())?;
        let database_uuid = manifest.manifest().database_uuid;

        // Create WAL writer (continuing from recovered state)
        let wal_writer = WalWriter::new(
            paths.wal_dir.clone(),
            database_uuid,
            config.durability,
            config.wal_config.clone(),
        )?;

        // Create snapshot writer
        let snapshot_writer = SnapshotWriter::new(
            paths.snapshots_dir.clone(),
            codec.clone(),
            database_uuid,
        )?;

        let db = Database {
            paths,
            engine: result.engine,
            manifest: Arc::new(Mutex::new(manifest)),
            wal_writer: Arc::new(Mutex::new(wal_writer)),
            snapshot_writer: Arc::new(snapshot_writer),
            codec,
            config,
            retention_store: Arc::new(RetentionPolicyStore::new(/* ... */)),
            compaction_lock: Mutex::new(()),
            database_uuid,
        };

        Ok(db)
    }

    /// Open or create a database
    ///
    /// If database exists, opens it. Otherwise creates new.
    pub fn open_or_create(
        path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        let paths = DatabasePaths::from_root(path.as_ref());

        if paths.exists() {
            Self::open(path, config)
        } else {
            Self::create(path, config)
        }
    }

    /// Open with platform default path
    ///
    /// Uses `~/.strata/default.db` or similar platform convention.
    pub fn open_default(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        let default_path = Self::default_path()?;
        Self::open_or_create(default_path, config)
    }

    /// Get platform default database path
    fn default_path() -> Result<PathBuf, DatabaseError> {
        let home = dirs::home_dir()
            .ok_or_else(|| DatabaseError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Home directory not found",
            )))?;

        Ok(home.join(".strata").join("default.db"))
    }

    /// Get database UUID
    pub fn uuid(&self) -> [u8; 16] {
        self.database_uuid
    }

    /// Get database path
    pub fn path(&self) -> &Path {
        &self.paths.root
    }
}
```

### Acceptance Criteria

- [ ] `create(path, config)` creates new database
- [ ] Fails if database already exists
- [ ] `open(path, config)` opens existing database
- [ ] Performs recovery on open
- [ ] Validates codec matches
- [ ] `open_or_create()` for convenience
- [ ] `open_default()` for platform default path

---

## Story #534: Database Close

**File**: `crates/storage/src/database.rs`

**Deliverable**: Clean database shutdown

### Implementation

```rust
impl Database {
    /// Close the database cleanly
    ///
    /// This:
    /// 1. Flushes any buffered WAL data
    /// 2. Updates MANIFEST
    /// 3. Syncs all files
    ///
    /// After close, the database can be safely copied or moved.
    pub fn close(self) -> Result<(), DatabaseError> {
        // Flush WAL
        {
            let mut wal = self.wal_writer.lock().unwrap();
            wal.flush()?;
        }

        // Update MANIFEST with final state
        {
            let mut manifest = self.manifest.lock().unwrap();
            let wal = self.wal_writer.lock().unwrap();
            manifest.set_active_segment(wal.current_segment())?;
        }

        // Engine cleanup
        drop(self.engine);

        Ok(())
    }

    /// Check if the database is open
    pub fn is_open(&self) -> bool {
        // Database is open if we have valid handles
        true // Simplified - actual impl would track state
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // Best-effort cleanup on drop
        // Full cleanup should use close()
        if let Ok(mut wal) = self.wal_writer.lock() {
            let _ = wal.flush();
        }
    }
}
```

### Acceptance Criteria

- [ ] `close()` flushes WAL
- [ ] Updates MANIFEST with final state
- [ ] Syncs all files
- [ ] After close, database is portable
- [ ] Drop impl for safety
- [ ] Consumes self to prevent use after close

---

## Story #535: DatabaseConfig Type

**File**: `crates/storage/src/config.rs` (NEW)

**Deliverable**: Database configuration type

### Implementation

```rust
/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Durability mode for commits
    pub durability: DurabilityMode,

    /// WAL configuration
    pub wal_config: WalConfig,

    /// Codec identifier (default: "identity")
    pub codec_id: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::Strict,
            wal_config: WalConfig::default(),
            codec_id: "identity".to_string(),
        }
    }
}

impl DatabaseConfig {
    /// Create config with strict durability
    pub fn strict() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::Strict,
            ..Default::default()
        }
    }

    /// Create config with buffered durability
    pub fn buffered() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::Buffered,
            ..Default::default()
        }
    }

    /// Create config for in-memory only (no persistence)
    pub fn in_memory() -> Self {
        DatabaseConfig {
            durability: DurabilityMode::InMemory,
            ..Default::default()
        }
    }

    /// Set durability mode
    pub fn with_durability(mut self, mode: DurabilityMode) -> Self {
        self.durability = mode;
        self
    }

    /// Set WAL segment size
    pub fn with_wal_segment_size(mut self, size: u64) -> Self {
        self.wal_config.segment_size = size;
        self
    }

    /// Set codec
    pub fn with_codec(mut self, codec_id: impl Into<String>) -> Self {
        self.codec_id = codec_id.into();
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.wal_config.validate()?;

        // Validate codec exists
        get_codec(&self.codec_id)
            .map_err(|e| ConfigError::InvalidCodec(e.to_string()))?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid WAL config: {0}")]
    InvalidWalConfig(#[from] crate::wal::config::ConfigError),

    #[error("Invalid codec: {0}")]
    InvalidCodec(String),
}
```

### Acceptance Criteria

- [ ] `DatabaseConfig` with durability, wal_config, codec_id
- [ ] Default: strict durability, identity codec
- [ ] Preset constructors: `strict()`, `buffered()`, `in_memory()`
- [ ] Builder methods: `with_durability()`, `with_wal_segment_size()`, `with_codec()`
- [ ] `validate()` checks configuration

---

## Story #536: Export (Convenience Wrapper)

**File**: `crates/storage/src/database.rs`

**Deliverable**: Export database to portable artifact

### Implementation

```rust
impl Database {
    /// Export database to a specified path
    ///
    /// This is a convenience wrapper around:
    /// 1. checkpoint()
    /// 2. Copy database directory
    ///
    /// The exported database is a complete, portable copy.
    ///
    /// # Example
    /// ```
    /// // Export to a backup location
    /// db.export("/backup/my-database")?;
    ///
    /// // Export can be opened elsewhere
    /// let backup = Database::open("/backup/my-database", config)?;
    /// ```
    pub fn export(&self, dest: impl AsRef<Path>) -> Result<ExportInfo, DatabaseError> {
        let dest = dest.as_ref();

        // Ensure destination doesn't exist
        if dest.exists() {
            return Err(DatabaseError::AlreadyExists { path: dest.to_path_buf() });
        }

        // Create checkpoint to ensure consistent state
        let checkpoint = self.checkpoint()?;

        // Close WAL cleanly for copy
        {
            let mut wal = self.wal_writer.lock().unwrap();
            wal.flush()?;
        }

        // Copy entire directory
        copy_dir_recursive(&self.paths.root, dest)?;

        Ok(ExportInfo {
            path: dest.to_path_buf(),
            checkpoint_watermark: checkpoint.watermark_txn,
            size_bytes: dir_size(dest)?,
        })
    }

    /// Export to a compressed archive
    pub fn export_archive(
        &self,
        dest: impl AsRef<Path>,
    ) -> Result<ExportInfo, DatabaseError> {
        let temp_dir = tempfile::tempdir()?;
        let temp_path = temp_dir.path().join("export");

        // Export to temp
        self.export(&temp_path)?;

        // Create archive
        let dest = dest.as_ref();
        let archive_file = std::fs::File::create(dest)?;
        let encoder = flate2::write::GzEncoder::new(archive_file, flate2::Compression::default());
        let mut archive = tar::Builder::new(encoder);
        archive.append_dir_all("strata.db", &temp_path)?;
        archive.finish()?;

        Ok(ExportInfo {
            path: dest.to_path_buf(),
            checkpoint_watermark: 0, // Would need to track
            size_bytes: std::fs::metadata(dest)?.len(),
        })
    }
}

/// Export result information
#[derive(Debug, Clone)]
pub struct ExportInfo {
    /// Path to exported database
    pub path: PathBuf,

    /// Checkpoint watermark at time of export
    pub checkpoint_watermark: u64,

    /// Total size in bytes
    pub size_bytes: u64,
}

/// Recursively copy a directory
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

/// Calculate directory size
fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0;

    for entry in walkdir::WalkDir::new(path) {
        let entry = entry.map_err(|e| std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))?;

        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }

    Ok(total)
}
```

### Acceptance Criteria

- [ ] `export(path)` creates checkpoint + copies directory
- [ ] Fails if destination exists
- [ ] Returns ExportInfo with path, watermark, size
- [ ] `export_archive()` creates compressed archive (optional)
- [ ] Exported database is fully portable

---

## Story #537: Import (Open Exported Artifact)

**File**: `crates/storage/src/database.rs`

**Deliverable**: Import is just Database::open

### Implementation

```rust
impl Database {
    /// Import a database from an exported location
    ///
    /// This is equivalent to `Database::open()` - an exported database
    /// is a valid database that can be opened directly.
    ///
    /// # Example
    /// ```
    /// // Export
    /// db.export("/backup/my-database")?;
    ///
    /// // Import is just open
    /// let imported = Database::import("/backup/my-database", config)?;
    ///
    /// // Or equivalently:
    /// let imported = Database::open("/backup/my-database", config)?;
    /// ```
    pub fn import(
        path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        // Import is just open - exported databases are valid databases
        Self::open(path, config)
    }

    /// Import from compressed archive
    pub fn import_archive(
        archive_path: impl AsRef<Path>,
        dest_path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        let archive_path = archive_path.as_ref();
        let dest_path = dest_path.as_ref();

        // Extract archive
        let archive_file = std::fs::File::open(archive_path)?;
        let decoder = flate2::read::GzDecoder::new(archive_file);
        let mut archive = tar::Archive::new(decoder);
        archive.unpack(dest_path)?;

        // Find extracted database directory
        let db_path = dest_path.join("strata.db");
        if !db_path.exists() {
            return Err(DatabaseError::NotFound { path: db_path });
        }

        Self::open(db_path, config)
    }
}
```

### Acceptance Criteria

- [ ] `import(path, config)` = `open(path, config)`
- [ ] Exported database is directly openable
- [ ] `import_archive()` extracts and opens (optional)
- [ ] Documentation clarifies import = open

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_create_and_open() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create
        {
            let db = Database::create(&db_path, DatabaseConfig::default()).unwrap();
            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key", b"value").unwrap();
            db.close().unwrap();
        }

        // Open
        {
            let db = Database::open(&db_path, DatabaseConfig::default()).unwrap();
            let run_id = db.resolve_run("test-run").unwrap();
            let value = db.kv_get(run_id, "key").unwrap();
            assert_eq!(value.unwrap().value, b"value");
        }
    }

    #[test]
    fn test_database_create_fails_if_exists() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create first
        Database::create(&db_path, DatabaseConfig::default()).unwrap();

        // Second create should fail
        let result = Database::create(&db_path, DatabaseConfig::default());
        assert!(matches!(result, Err(DatabaseError::AlreadyExists { .. })));
    }

    #[test]
    fn test_database_open_fails_if_not_exists() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("nonexistent.db");

        let result = Database::open(&db_path, DatabaseConfig::default());
        assert!(matches!(result, Err(DatabaseError::NotFound { .. })));
    }

    #[test]
    fn test_database_open_or_create() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // First call creates
        {
            let db = Database::open_or_create(&db_path, DatabaseConfig::default()).unwrap();
            db.create_run("test-run").unwrap();
            db.close().unwrap();
        }

        // Second call opens
        {
            let db = Database::open_or_create(&db_path, DatabaseConfig::default()).unwrap();
            assert!(db.resolve_run("test-run").is_ok());
        }
    }

    #[test]
    fn test_database_export_import() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let dst_path = dir.path().join("export.db");

        // Create and populate
        {
            let db = Database::create(&src_path, DatabaseConfig::default()).unwrap();
            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
            db.kv_put(run_id, "key2", b"value2").unwrap();

            // Export
            let info = db.export(&dst_path).unwrap();
            assert!(info.path.exists());
            db.close().unwrap();
        }

        // Import and verify
        {
            let db = Database::import(&dst_path, DatabaseConfig::default()).unwrap();
            let run_id = db.resolve_run("test-run").unwrap();

            let v1 = db.kv_get(run_id, "key1").unwrap();
            let v2 = db.kv_get(run_id, "key2").unwrap();

            assert_eq!(v1.unwrap().value, b"value1");
            assert_eq!(v2.unwrap().value, b"value2");
        }
    }

    #[test]
    fn test_database_portability_by_copy() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let copy_path = dir.path().join("copy.db");

        // Create and close
        {
            let db = Database::create(&src_path, DatabaseConfig::default()).unwrap();
            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key", b"value").unwrap();
            db.checkpoint().unwrap();
            db.close().unwrap();
        }

        // Manual copy
        copy_dir_recursive(&src_path, &copy_path).unwrap();

        // Open copy
        {
            let db = Database::open(&copy_path, DatabaseConfig::default()).unwrap();
            let run_id = db.resolve_run("test-run").unwrap();
            let value = db.kv_get(run_id, "key").unwrap();
            assert_eq!(value.unwrap().value, b"value");
        }
    }

    #[test]
    fn test_database_config_validation() {
        let valid = DatabaseConfig::default();
        assert!(valid.validate().is_ok());

        let invalid = DatabaseConfig {
            codec_id: "nonexistent-codec".to_string(),
            ..Default::default()
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_codec_mismatch_on_open() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create with identity codec
        {
            let config = DatabaseConfig::default().with_codec("identity");
            let db = Database::create(&db_path, config).unwrap();
            db.close().unwrap();
        }

        // Try to open with different codec
        let config = DatabaseConfig::default().with_codec("nonexistent");
        let result = Database::open(&db_path, config);

        // Should fail - codec doesn't exist
        assert!(result.is_err());
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/database.rs` | CREATE - Database lifecycle |
| `crates/storage/src/config.rs` | CREATE - DatabaseConfig |
| `crates/storage/src/lib.rs` | MODIFY - Export database module |
