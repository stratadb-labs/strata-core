# Epic 75: Database Lifecycle - Implementation Prompts

**Epic Goal**: Implement database open/close and portability features

**GitHub Issue**: [#532](https://github.com/anibjoshi/in-mem/issues/532)
**Status**: Ready to begin
**Dependencies**: Epic 70 (WAL), Epic 71 (Snapshot), Epic 72 (Recovery)
**Phase**: 5 (Database Lifecycle)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Database lifecycle management`
> **WRONG**: `//! M10 Database lifecycle for Strata`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M10_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M10_ARCHITECTURE.md`
2. **Implementation Plan**: `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M10/EPIC_75_DATABASE_LIFECYCLE.md`
4. **Prompt Header**: `docs/prompts/M10/M10_PROMPT_HEADER.md` for the 8 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 75 Overview

### Scope
- Database directory structure
- Database open (new and existing)
- Database close with proper cleanup
- DatabaseConfig type
- Export (convenience wrapper)
- Import (open exported artifact)

### Key Rules for Epic 75

1. **Database is a directory** - SQLite-like portability by copy
2. **Open performs recovery** - Transparent to user
3. **Close ensures durability** - Flush and sync
4. **Export = checkpoint + copy** - No special format

### Success Criteria
- [ ] Database directory structure (`strata.db/`)
- [ ] `Database::create()` for new databases
- [ ] `Database::open()` with recovery
- [ ] `Database::close()` with cleanup
- [ ] `DatabaseConfig` with defaults
- [ ] `export()` and `import()` for portability
- [ ] All tests passing

### Component Breakdown
- **Story #532**: Database Directory Structure - FOUNDATION
- **Story #533**: Database Open (New and Existing) - CRITICAL
- **Story #534**: Database Close - CRITICAL
- **Story #535**: DatabaseConfig Type - HIGH
- **Story #536**: Export (Convenience Wrapper) - HIGH
- **Story #537**: Import (Open Exported Artifact) - HIGH

---

## File Organization

### Directory Structure

```bash
mkdir -p crates/storage/src
```

**Target structure**:
```
crates/storage/src/
├── lib.rs
├── database.rs               # NEW - Database lifecycle
├── config.rs                 # NEW - DatabaseConfig
├── format/
│   └── ...
├── wal/
│   └── ...
├── snapshot/
│   └── ...
├── recovery/
│   └── ...
├── retention/
│   └── ...
├── compaction/
│   └── ...
└── codec/
    └── ...
```

### Database Directory Layout

```
strata.db/                    # Database root
├── MANIFEST                  # Physical metadata
├── WAL/
│   ├── wal-000001.seg
│   ├── wal-000002.seg
│   └── ...
├── SNAPSHOTS/
│   ├── snap-000001.chk
│   └── ...
└── DATA/                     # Reserved for future use
    └── ...
```

---

## Dependency Graph

```
Story #532 (Directory) ──────> Story #533 (Open)
                                     │
Story #535 (Config) ─────────────────┘
                                     │
                              └──> Story #534 (Close)
                                     │
                              └──> Story #536 (Export)
                                     │
                              └──> Story #537 (Import)
```

**Recommended Order**: #532 (Directory) → #535 (Config) → #533 (Open) → #534 (Close) → #536 (Export) → #537 (Import)

---

## Story #532: Database Directory Structure

**GitHub Issue**: [#532](https://github.com/anibjoshi/in-mem/issues/532)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: Story #533

### Start Story

```bash
gh issue view 532
./scripts/start-story.sh 75 532 database-directory
```

### Implementation

Create `crates/storage/src/database.rs`:

```rust
//! Database lifecycle management
//!
//! A database is a portable directory containing all state.

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

### Complete Story

```bash
./scripts/complete-story.sh 532
```

---

## Story #533: Database Open (New and Existing)

**GitHub Issue**: [#533](https://github.com/anibjoshi/in-mem/issues/533)
**Estimated Time**: 4 hours
**Dependencies**: Stories #532, #535
**Blocks**: Stories #534, #536

### Start Story

```bash
gh issue view 533
./scripts/start-story.sh 75 533 database-open
```

### Implementation

Add to `crates/storage/src/database.rs`:

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

        if paths.exists() {
            return Err(DatabaseError::AlreadyExists { path: paths.root });
        }

        paths.create_directories()?;

        let database_uuid = uuid::Uuid::new_v4().into_bytes();

        let codec = get_codec(&config.codec_id)
            .map_err(|e| DatabaseError::Storage(StorageError::Codec(e.to_string())))?;

        let manifest = ManifestManager::create(
            paths.manifest.clone(),
            database_uuid,
            config.codec_id.clone(),
        )?;

        let wal_writer = WalWriter::new(
            paths.wal_dir.clone(),
            database_uuid,
            config.durability,
            config.wal_config.clone(),
        )?;

        let snapshot_writer = SnapshotWriter::new(
            paths.snapshots_dir.clone(),
            codec.clone(),
            database_uuid,
        )?;

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
        paths.validate()?;

        let codec = get_codec(&config.codec_id)
            .map_err(|e| DatabaseError::Storage(StorageError::Codec(e.to_string())))?;

        // Perform recovery
        let recovery = Recovery::new(paths.root.clone(), codec.clone());
        let result = recovery.recover()?;

        if result.manifest.codec_id != config.codec_id {
            return Err(DatabaseError::CodecMismatch {
                database: result.manifest.codec_id.clone(),
                config: config.codec_id.clone(),
            });
        }

        let manifest = ManifestManager::load(paths.manifest.clone())?;
        let database_uuid = manifest.manifest().database_uuid;

        let wal_writer = WalWriter::new(
            paths.wal_dir.clone(),
            database_uuid,
            config.durability,
            config.wal_config.clone(),
        )?;

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
    pub fn open_default(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        let default_path = Self::default_path()?;
        Self::open_or_create(default_path, config)
    }

    fn default_path() -> Result<PathBuf, DatabaseError> {
        let home = dirs::home_dir()
            .ok_or_else(|| DatabaseError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Home directory not found",
            )))?;

        Ok(home.join(".strata").join("default.db"))
    }

    pub fn uuid(&self) -> [u8; 16] {
        self.database_uuid
    }

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

### Complete Story

```bash
./scripts/complete-story.sh 533
```

---

## Story #534: Database Close

**GitHub Issue**: [#534](https://github.com/anibjoshi/in-mem/issues/534)
**Estimated Time**: 2 hours
**Dependencies**: Story #533
**Blocks**: None

### Start Story

```bash
gh issue view 534
./scripts/start-story.sh 75 534 database-close
```

### Implementation

Add to `crates/storage/src/database.rs`:

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
        true // Simplified
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // Best-effort cleanup on drop
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

### Complete Story

```bash
./scripts/complete-story.sh 534
```

---

## Story #535: DatabaseConfig Type

**GitHub Issue**: [#535](https://github.com/anibjoshi/in-mem/issues/535)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Story #533

### Start Story

```bash
gh issue view 535
./scripts/start-story.sh 75 535 database-config
```

### Implementation

Create `crates/storage/src/config.rs`:

```rust
//! Database configuration

use crate::wal::config::WalConfig;
use crate::wal::writer::DurabilityMode;

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

    pub fn with_durability(mut self, mode: DurabilityMode) -> Self {
        self.durability = mode;
        self
    }

    pub fn with_wal_segment_size(mut self, size: u64) -> Self {
        self.wal_config.segment_size = size;
        self
    }

    pub fn with_codec(mut self, codec_id: impl Into<String>) -> Self {
        self.codec_id = codec_id.into();
        self
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.wal_config.validate()?;
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

### Complete Story

```bash
./scripts/complete-story.sh 535
```

---

## Story #536: Export (Convenience Wrapper)

**GitHub Issue**: [#536](https://github.com/anibjoshi/in-mem/issues/536)
**Estimated Time**: 2 hours
**Dependencies**: Story #533
**Blocks**: Story #537

### Start Story

```bash
gh issue view 536
./scripts/start-story.sh 75 536 database-export
```

### Implementation

Add to `crates/storage/src/database.rs`:

```rust
impl Database {
    /// Export database to a specified path
    ///
    /// This is a convenience wrapper around:
    /// 1. checkpoint()
    /// 2. Copy database directory
    ///
    /// The exported database is a complete, portable copy.
    pub fn export(&self, dest: impl AsRef<Path>) -> Result<ExportInfo, DatabaseError> {
        let dest = dest.as_ref();

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
- [ ] Exported database is fully portable

### Complete Story

```bash
./scripts/complete-story.sh 536
```

---

## Story #537: Import (Open Exported Artifact)

**GitHub Issue**: [#537](https://github.com/anibjoshi/in-mem/issues/537)
**Estimated Time**: 1 hour
**Dependencies**: Story #536
**Blocks**: None

### Start Story

```bash
gh issue view 537
./scripts/start-story.sh 75 537 database-import
```

### Implementation

Add to `crates/storage/src/database.rs`:

```rust
impl Database {
    /// Import a database from an exported location
    ///
    /// This is equivalent to `Database::open()` - an exported database
    /// is a valid database that can be opened directly.
    pub fn import(
        path: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, DatabaseError> {
        Self::open(path, config)
    }
}
```

### Acceptance Criteria

- [ ] `import(path, config)` = `open(path, config)`
- [ ] Exported database is directly openable
- [ ] Documentation clarifies import = open

### Complete Story

```bash
./scripts/complete-story.sh 537
```

---

## Epic 75 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `DatabasePaths` struct
- [ ] `Database::create()` and `Database::open()`
- [ ] `Database::close()` with cleanup
- [ ] `DatabaseConfig` with presets
- [ ] `export()` and `import()` for portability

### 3. Run Epic-End Validation

See `docs/prompts/EPIC_END_VALIDATION.md`

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-75-database-lifecycle -m "Epic 75: Database Lifecycle complete

Delivered:
- Database directory structure
- Database open/create/close
- DatabaseConfig with durability modes
- Export and import for portability

Stories: #532, #533, #534, #535, #536, #537
"
git push origin develop
gh issue close 532 --comment "Epic 75: Database Lifecycle - COMPLETE"
```

---

## Summary

Epic 75 establishes the database lifecycle:

- **Directory Structure** provides portable storage
- **Open/Create** initializes database with recovery
- **Close** ensures durability and portability
- **Config** controls durability and codec
- **Export/Import** enables backup and migration

This completes the user-facing database API.
