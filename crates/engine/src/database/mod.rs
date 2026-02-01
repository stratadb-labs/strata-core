//! Database struct and open/close logic
//!
//! This module provides the main Database struct that orchestrates:
//! - Storage initialization
//! - WAL opening
//! - Automatic recovery on startup
//! - Transaction API
//!
//! ## Transaction API
//!
//! The Database provides two ways to execute transactions:
//!
//! 1. **Closure API** (recommended): `db.transaction(branch_id, |txn| { ... })`
//!    - Automatic commit on success, abort on error
//!    - Returns the closure's return value
//!
//! 2. **Manual API**: `begin_transaction()` + `commit_transaction()`
//!    - For cases requiring external control over commit timing
//!
//! Per spec Section 4: Implicit transactions wrap legacy-style operations.

mod builder;
mod registry;
mod transactions;

pub use builder::DatabaseBuilder;
pub use registry::OPEN_DATABASES;
pub use transactions::RetryConfig;

use crate::coordinator::TransactionCoordinator;
use crate::transaction::TransactionPool;
use dashmap::DashMap;
use parking_lot::Mutex as ParkingMutex;
use std::any::{Any, TypeId};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use strata_concurrency::{RecoveryCoordinator, TransactionContext};
use strata_core::types::{BranchId, Key};
use strata_core::StrataError;
use strata_core::{StrataResult, VersionedValue};
use strata_durability::codec::IdentityCodec;
use strata_durability::wal::{DurabilityMode, WalConfig, WalWriter};
use strata_storage::ShardedStore;
use tracing::info;

// ============================================================================
// Persistence Mode (Storage/Durability Split)
// ============================================================================

/// Controls where data is stored (orthogonal to durability)
///
/// This enum distinguishes between truly in-memory (ephemeral) databases
/// and disk-backed databases. This is orthogonal to `DurabilityMode`,
/// which controls WAL sync behavior.
///
/// # Persistence vs Durability
///
/// | PersistenceMode | DurabilityMode | Behavior |
/// |-----------------|----------------|----------|
/// | Ephemeral | (ignored) | No files, data lost on drop |
/// | Disk | Cache | Files created, no fsync |
/// | Disk | Standard | Files created, periodic fsync |
/// | Disk | Always | Files created, immediate fsync |
///
/// # Use Cases
///
/// - **Ephemeral**: Unit tests, caching, temporary computations
/// - **Disk + Cache**: Integration tests (fast, isolated, but files exist)
/// - **Disk + Standard**: Production workloads
/// - **Disk + Always**: Audit logs, critical data
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistenceMode {
    /// No disk files at all - data exists only in memory
    ///
    /// - No directories created
    /// - No WAL file
    /// - No recovery possible
    /// - Data lost when database is dropped
    ///
    /// Use for unit tests, caching, and truly ephemeral data.
    Ephemeral,

    /// Data stored on disk (temp or user-specified path)
    ///
    /// Creates directories and WAL file. Data can survive crashes
    /// depending on the `DurabilityMode`.
    Disk,
}

impl Default for PersistenceMode {
    fn default() -> Self {
        PersistenceMode::Disk
    }
}

// ============================================================================
// Database Struct
// ============================================================================

/// Main database struct with transaction support
///
/// Orchestrates storage, WAL, recovery, and transactions.
/// Create a database by calling `Database::open()`.
///
/// # Transaction Support
///
/// The Database provides transaction APIs per spec Section 4:
/// - `transaction()`: Execute a closure within a transaction
/// - `begin_transaction()`: Start a manual transaction
/// - `commit_transaction()`: Commit a manual transaction
///
/// # Example
///
/// ```ignore
/// use strata_engine::Database;
/// use strata_core::types::BranchId;
///
/// let db = Database::open("/path/to/data")?;
/// let branch_id = BranchId::new();
///
/// // Closure API (recommended)
/// let result = db.transaction(branch_id, |txn| {
///     txn.put(key, value)?;
///     Ok(())
/// })?;
/// ```
pub struct Database {
    /// Data directory path (empty for ephemeral databases)
    data_dir: PathBuf,

    /// Sharded storage with O(1) lazy snapshots (thread-safe)
    storage: Arc<ShardedStore>,

    /// Segmented WAL writer (protected by mutex for exclusive access)
    /// None for ephemeral databases (no disk I/O)
    /// Using parking_lot::Mutex to avoid lock poisoning on panic
    wal_writer: Option<Arc<ParkingMutex<WalWriter>>>,

    /// Persistence mode (ephemeral vs disk-backed)
    persistence_mode: PersistenceMode,

    /// Transaction coordinator for lifecycle management, version allocation, and metrics
    ///
    /// Per spec Section 6.1: Single monotonic counter for the entire database.
    /// Also owns the commit protocol via TransactionManager, including per-branch
    /// commit locks for TOCTOU prevention.
    coordinator: TransactionCoordinator,

    /// Current durability mode
    durability_mode: DurabilityMode,

    /// Flag to track if database is accepting new transactions
    ///
    /// Set to false during shutdown to reject new transactions.
    accepting_transactions: AtomicBool,

    /// Type-erased extension storage for primitive state
    ///
    /// Allows primitives like VectorStore to store their in-memory backends here,
    /// ensuring all VectorStore instances for the same Database share state.
    ///
    /// Extensions are lazily initialized on first access via `extension<T>()`.
    extensions: DashMap<TypeId, Arc<dyn Any + Send + Sync>>,

    /// Shutdown signal for the background WAL flush thread (Standard mode only)
    flush_shutdown: Arc<AtomicBool>,

    /// Handle for the background WAL flush thread
    ///
    /// In Standard mode, a background thread periodically calls sync_if_overdue()
    /// to flush WAL data to disk without blocking the write path (#969).
    flush_handle: ParkingMutex<Option<std::thread::JoinHandle<()>>>,
}

impl Database {
    /// Create a new database builder
    ///
    /// Returns a `DatabaseBuilder` for configuring the database before opening.
    /// Use this when you need custom durability settings.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let db = Database::builder()
    ///     .path("/data/mydb")
    ///     .always()
    ///     .open()?;
    /// ```
    pub fn builder() -> DatabaseBuilder {
        DatabaseBuilder::new()
    }

    /// Open database at given path with automatic recovery
    ///
    /// This is the simplest way to open a database. Uses standard durability
    /// by default, which provides a good balance between performance and safety.
    ///
    /// # Thread Safety
    ///
    /// Opening the same path from multiple threads returns the same `Arc<Database>`.
    /// This ensures all threads share the same database instance, which is safe
    /// because Database uses internal synchronization (DashMap, atomics, etc.).
    ///
    /// ```ignore
    /// let db1 = Database::open("/data")?;
    /// let db2 = Database::open("/data")?;  // Same Arc as db1
    /// assert!(Arc::ptr_eq(&db1, &db2));
    /// ```
    ///
    /// # Flow
    ///
    /// 1. Check registry for existing instance at this path
    /// 2. If found, return the existing Arc<Database>
    /// 3. Otherwise: create directory, open WAL, replay, register, return
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for the database
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Database>)` - Ready-to-use database instance (shared if path was already open)
    /// * `Err` - If directory creation, WAL opening, or recovery fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_engine::Database;
    ///
    /// let db = Database::open("/path/to/data")?;
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> StrataResult<Arc<Self>> {
        Self::open_with_mode(path, DurabilityMode::standard_default())
    }

    /// Open database with specific durability mode
    ///
    /// Allows selecting between Cache, Always, or Standard durability modes.
    ///
    /// # Thread Safety
    ///
    /// Uses a global registry to ensure the same path returns the same instance.
    /// If a database at this path is already open, returns the existing Arc.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for the database
    /// * `durability_mode` - Durability mode for WAL operations
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<Database>)` - Ready-to-use database instance
    /// * `Err` - If directory creation, WAL opening, or recovery fails
    ///
    /// # Recovery
    ///
    /// Per spec Section 5: Uses RecoveryCoordinator to replay WAL and
    /// initialize TransactionManager with the recovered version.
    pub(crate) fn open_with_mode<P: AsRef<Path>>(
        path: P,
        durability_mode: DurabilityMode,
    ) -> StrataResult<Arc<Self>> {
        // Create directory first so we can canonicalize the path
        let data_dir = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir).map_err(StrataError::from)?;

        // Canonicalize path for consistent registry keys
        let canonical_path = data_dir.canonicalize().map_err(StrataError::from)?;

        // Hold lock for entire operation to prevent TOCTOU race condition
        // This ensures only one thread creates a database for a given path.
        // For the common case (database already open), this is still fast.
        let mut registry = OPEN_DATABASES.lock().unwrap();

        // Check registry for existing instance
        if let Some(weak) = registry.get(&canonical_path) {
            if let Some(db) = weak.upgrade() {
                info!(path = ?canonical_path, "Returning existing database instance");
                return Ok(db);
            }
        }

        // Not in registry (or expired) - create new instance
        // Create WAL directory
        let wal_dir = data_dir.join("wal");
        std::fs::create_dir_all(&wal_dir).map_err(StrataError::from)?;

        // Use RecoveryCoordinator for proper transaction-aware recovery
        // This reads all WalRecords from the segmented WAL directory
        let recovery = RecoveryCoordinator::new(wal_dir.clone());
        let result = match recovery.recover() {
            Ok(result) => result,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Recovery failed — starting with empty state. Data from WAL may be lost."
                );
                strata_concurrency::RecoveryResult::empty()
            }
        };

        info!(
            txns_replayed = result.stats.txns_replayed,
            writes_applied = result.stats.writes_applied,
            deletes_applied = result.stats.deletes_applied,
            final_version = result.stats.final_version,
            "Recovery complete"
        );

        // Open segmented WAL writer for appending
        let wal_writer = WalWriter::new(
            wal_dir,
            [0u8; 16], // database UUID placeholder
            durability_mode,
            WalConfig::default(),
            Box::new(IdentityCodec),
        )?;

        // Create coordinator from recovery result (preserves version continuity)
        let coordinator = TransactionCoordinator::from_recovery(&result);

        let wal_arc = Arc::new(ParkingMutex::new(wal_writer));
        let flush_shutdown = Arc::new(AtomicBool::new(false));

        // Spawn background WAL flush thread for Standard mode (#969)
        let flush_handle = if let DurabilityMode::Standard { interval_ms, .. } = durability_mode {
            let wal = Arc::clone(&wal_arc);
            let shutdown = Arc::clone(&flush_shutdown);
            let interval = std::time::Duration::from_millis(interval_ms);

            let handle = std::thread::Builder::new()
                .name("strata-wal-flush".to_string())
                .spawn(move || {
                    while !shutdown.load(Ordering::Relaxed) {
                        std::thread::sleep(interval);
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                        let mut wal = wal.lock();
                        let _ = wal.sync_if_overdue();
                    }
                })
                .expect("Failed to spawn WAL flush thread");
            Some(handle)
        } else {
            None
        };

        let db = Arc::new(Self {
            data_dir: canonical_path.clone(),
            storage: Arc::new(result.storage),
            wal_writer: Some(wal_arc),
            persistence_mode: PersistenceMode::Disk,
            coordinator,
            durability_mode,
            accepting_transactions: AtomicBool::new(true),
            extensions: DashMap::new(),
            flush_shutdown,
            flush_handle: ParkingMutex::new(flush_handle),
        });

        // Register in global registry (lock already held)
        registry.insert(canonical_path, Arc::downgrade(&db));

        // Release lock before running primitive recovery (may be slow)
        drop(registry);

        // Run primitive recovery (e.g., VectorStore)
        // This must happen AFTER KV recovery completes, as primitives may
        // depend on config data stored in KV.
        crate::recovery::recover_all_participants(&db)?;

        Ok(db)
    }

    /// Create a cache database with no disk I/O
    ///
    /// This creates a truly in-memory database that:
    /// - Creates no files or directories
    /// - Has no WAL (write-ahead log)
    /// - Cannot recover after crash
    /// - Loses all data when dropped
    /// - Is NOT registered in the global registry (each call creates a new instance)
    ///
    /// Use this for:
    /// - Unit tests that need maximum isolation
    /// - Caching scenarios
    /// - Temporary computations
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_engine::Database;
    /// use strata_core::types::BranchId;
    ///
    /// let db = Database::cache()?;
    /// let branch_id = BranchId::new();
    ///
    /// // All operations work normally
    /// db.transaction(branch_id, |txn| {
    ///     txn.put(key, value)?;
    ///     Ok(())
    /// })?;
    ///
    /// // But data is gone when db is dropped
    /// drop(db);
    /// ```
    ///
    /// # Comparison with disk-backed databases
    ///
    /// | Method | Disk Files | WAL | Recovery |
    /// |--------|------------|-----|----------|
    /// | `cache()` | None | None | No |
    /// | `open(path)` | Yes | Yes (standard) | Yes |
    /// | `builder().path(p).always().open()` | Yes | Yes (always) | Yes |
    pub fn cache() -> StrataResult<Arc<Self>> {
        // Create fresh storage
        let storage = ShardedStore::new();

        // Create coordinator starting at version 1 (no recovery needed)
        let coordinator = TransactionCoordinator::new(1);

        let db = Arc::new(Self {
            data_dir: PathBuf::new(), // Empty path for ephemeral
            storage: Arc::new(storage),
            wal_writer: None, // No WAL for ephemeral
            persistence_mode: PersistenceMode::Ephemeral,
            coordinator,
            durability_mode: DurabilityMode::Cache, // Irrelevant but set for consistency
            accepting_transactions: AtomicBool::new(true),
            extensions: DashMap::new(),
            flush_shutdown: Arc::new(AtomicBool::new(false)),
            flush_handle: ParkingMutex::new(None),
        });

        // Note: Ephemeral databases are NOT registered in the global registry
        // because they have no path and should always be independent instances

        Ok(db)
    }

    // ========================================================================
    // Accessors
    // ========================================================================

    /// Get reference to the storage layer (internal use only)
    ///
    /// This is for internal engine use. External users should use
    /// primitives (KVStore, EventLog, etc.) which go through transactions.
    pub(crate) fn storage(&self) -> &Arc<ShardedStore> {
        &self.storage
    }

    /// Get version history for a key directly from storage.
    ///
    /// History reads bypass the transaction layer because they are
    /// inherently non-transactional: you want all versions, not a
    /// snapshot-consistent subset.
    ///
    /// Returns versions newest-first. Empty if the key does not exist.
    pub(crate) fn get_history(
        &self,
        key: &Key,
        limit: Option<usize>,
        before_version: Option<u64>,
    ) -> StrataResult<Vec<VersionedValue>> {
        use strata_core::Storage;
        self.storage.get_history(key, limit, before_version)
    }

    /// Check if this is an ephemeral (no-disk) database
    pub fn is_ephemeral(&self) -> bool {
        self.persistence_mode == PersistenceMode::Ephemeral
    }

    /// Get current WAL counters snapshot.
    ///
    /// Returns `None` for ephemeral databases (no WAL).
    /// Briefly locks the WAL mutex to read counter values.
    pub fn durability_counters(&self) -> Option<strata_durability::WalCounters> {
        self.wal_writer.as_ref().map(|w| w.lock().counters())
    }

    /// Check if the database is currently open and accepting transactions
    pub fn is_open(&self) -> bool {
        self.accepting_transactions.load(Ordering::SeqCst)
    }

    // ========================================================================
    // Extension API
    // ========================================================================

    /// Get or create a typed extension bound to this Database
    ///
    /// Extensions allow primitives to store in-memory state that is shared
    /// across all instances of that primitive for this Database.
    ///
    /// # Behavior
    ///
    /// - If the extension exists, returns it
    /// - If missing, creates with `Default::default()`, stores, and returns it
    /// - Always returns `Arc<T>` for shared ownership
    ///
    /// # Thread Safety
    ///
    /// This method is safe to call concurrently. The extension is created
    /// at most once, using DashMap's entry API for atomicity.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #[derive(Default)]
    /// struct VectorBackendState {
    ///     backends: RwLock<BTreeMap<CollectionId, Box<dyn VectorIndexBackend>>>,
    /// }
    ///
    /// // All VectorStore instances for this Database share the same state
    /// let state = db.extension::<VectorBackendState>();
    /// ```
    pub(crate) fn extension<T: Any + Send + Sync + Default>(&self) -> Arc<T> {
        let type_id = TypeId::of::<T>();

        // Use entry API for atomic get-or-insert
        let entry = self
            .extensions
            .entry(type_id)
            .or_insert_with(|| Arc::new(T::default()) as Arc<dyn Any + Send + Sync>);

        // Downcast to concrete type - this cannot fail because we control the insertion
        entry
            .value()
            .clone()
            .downcast::<T>()
            .expect("extension type mismatch - this is a bug")
    }

    // ========================================================================
    // Branch Lifecycle Cleanup
    // ========================================================================

    /// Garbage-collect old versions before the given version number.
    ///
    /// Removes old versions from version chains across all entries in the branch.
    /// Returns the number of pruned versions.
    pub fn gc_versions_before(&self, branch_id: BranchId, min_version: u64) -> usize {
        self.storage.gc_branch(branch_id, min_version)
    }

    /// Get the current global version from the coordinator.
    ///
    /// This is the highest version allocated so far and serves as
    /// a safe GC boundary when no active snapshots need older versions.
    pub fn current_version(&self) -> u64 {
        self.coordinator.current_version()
    }

    /// Remove the per-branch commit lock after a branch is deleted.
    ///
    /// This prevents unbounded growth of the commit_locks map in the
    /// TransactionManager when branches are repeatedly created and deleted.
    ///
    /// Should be called after `BranchIndex::delete_branch()` succeeds.
    pub fn remove_branch_lock(&self, branch_id: &BranchId) {
        self.coordinator.remove_branch_lock(branch_id);
    }

    // ========================================================================
    // Flush
    // ========================================================================

    /// Flush WAL to disk
    ///
    /// Forces all buffered WAL entries to be written to disk.
    /// This is automatically done based on durability mode, but can
    /// be called manually to ensure durability at a specific point.
    ///
    /// For ephemeral databases, this is a no-op.
    pub(crate) fn flush(&self) -> StrataResult<()> {
        if let Some(ref wal) = self.wal_writer {
            let mut wal = wal.lock();
            wal.flush().map_err(StrataError::from)
        } else {
            // Ephemeral mode - no-op
            Ok(())
        }
    }

    // ========================================================================
    // Checkpoint & Compaction (future work)
    // ========================================================================

    /// Create a snapshot checkpoint of the current database state.
    ///
    /// Checkpoints serialize all primitive state to a crash-safe snapshot file,
    /// update the manifest watermark, and optionally trigger WAL compaction.
    ///
    /// See: `docs/architecture/STORAGE_DURABILITY_ARCHITECTURE.md` Section 6.3
    ///
    /// TODO: Wire to `DatabaseHandle::checkpoint()` and `CheckpointCoordinator`
    /// once the full checkpoint flow is implemented.
    pub fn checkpoint(&self) -> StrataResult<()> {
        Err(StrataError::internal("checkpoint() not yet implemented"))
    }

    /// Compact WAL segments that are no longer needed for recovery.
    ///
    /// Removes closed WAL segments whose max transaction ID is at or below the
    /// latest snapshot watermark. The active segment is never removed.
    ///
    /// See: `docs/architecture/STORAGE_DURABILITY_ARCHITECTURE.md` Section 5.6
    ///
    /// TODO: Wire to `DatabaseHandle::compact()` and `WalOnlyCompactor`
    /// once the full compaction flow is implemented.
    pub fn compact(&self) -> StrataResult<()> {
        Err(StrataError::internal("compact() not yet implemented"))
    }

    // ========================================================================
    // Transaction API
    // ========================================================================

    /// Check if the database is accepting transactions.
    fn check_accepting(&self) -> StrataResult<()> {
        if !self.accepting_transactions.load(Ordering::SeqCst) {
            return Err(StrataError::invalid_input(
                "Database is shutting down".to_string(),
            ));
        }
        Ok(())
    }

    /// Execute one transaction attempt: commit on success, abort on error.
    ///
    /// Handles the commit-or-abort decision and coordinator bookkeeping.
    /// The caller is responsible for calling `end_transaction()` afterward.
    ///
    /// Returns `(closure_result, commit_version)` on success.
    fn run_single_attempt<T>(
        &self,
        txn: &mut TransactionContext,
        result: StrataResult<T>,
        durability: DurabilityMode,
    ) -> StrataResult<(T, u64)> {
        match result {
            Ok(value) => {
                // Commit on success
                let commit_version = self.commit_internal(txn, durability)?;
                Ok((value, commit_version))
            }
            Err(e) => {
                let _ = txn.mark_aborted(format!("Closure error: {}", e));
                self.coordinator.record_abort();
                Err(e)
            }
        }
    }

    /// Execute a transaction with the given closure
    ///
    /// Per spec Section 4:
    /// - Creates TransactionContext with snapshot
    /// - Executes closure with transaction
    /// - Validates and commits on success
    /// - Aborts on error
    ///
    /// # Arguments
    /// * `branch_id` - BranchId for namespace isolation
    /// * `f` - Closure that performs transaction operations
    ///
    /// # Returns
    /// * `Ok(T)` - Closure return value on successful commit
    /// * `Err` - On validation conflict or closure error
    ///
    /// # Example
    /// ```ignore
    /// let result = db.transaction(branch_id, |txn| {
    ///     let val = txn.get(&key)?;
    ///     txn.put(key, new_value)?;
    ///     Ok(val)
    /// })?;
    /// ```
    pub fn transaction<F, T>(&self, branch_id: BranchId, f: F) -> StrataResult<T>
    where
        F: FnOnce(&mut TransactionContext) -> StrataResult<T>,
    {
        self.check_accepting()?;
        let mut txn = self.begin_transaction(branch_id);
        let result = f(&mut txn);
        let outcome = self.run_single_attempt(&mut txn, result, self.durability_mode);
        self.end_transaction(txn);
        outcome.map(|(value, _)| value)
    }

    /// Execute a transaction and return both the result and commit version
    ///
    /// Like `transaction()` but also returns the commit version assigned to all writes.
    /// Use this when you need to know the version created by write operations.
    ///
    /// # Returns
    /// * `Ok((T, u64))` - Closure result and commit version
    /// * `Err` - On validation conflict or closure error
    ///
    /// # Example
    /// ```ignore
    /// let (result, commit_version) = db.transaction_with_version(branch_id, |txn| {
    ///     txn.put(key, value)?;
    ///     Ok("success")
    /// })?;
    /// // commit_version now contains the version assigned to the put
    /// ```
    pub(crate) fn transaction_with_version<F, T>(
        &self,
        branch_id: BranchId,
        f: F,
    ) -> StrataResult<(T, u64)>
    where
        F: FnOnce(&mut TransactionContext) -> StrataResult<T>,
    {
        self.check_accepting()?;
        let mut txn = self.begin_transaction(branch_id);
        let result = f(&mut txn);
        let outcome = self.run_single_attempt(&mut txn, result, self.durability_mode);
        self.end_transaction(txn);
        outcome
    }

    /// Execute a transaction with automatic retry on conflict
    ///
    /// Per spec Section 4.3: Implicit transactions include automatic retry on conflict.
    /// This method provides explicit retry control for transactions that may conflict.
    ///
    /// The closure is called repeatedly until either:
    /// - The transaction commits successfully
    /// - A non-conflict error occurs (not retried)
    /// - Maximum retries are exceeded
    ///
    /// # Arguments
    /// * `branch_id` - BranchId for namespace isolation
    /// * `config` - Retry configuration (max retries, delays)
    /// * `f` - Closure that performs transaction operations (must be `Fn`, not `FnOnce`)
    ///
    /// # Returns
    /// * `Ok(T)` - Closure return value on successful commit
    /// * `Err` - On non-conflict error or max retries exceeded
    ///
    /// # Example
    /// ```ignore
    /// let config = RetryConfig::default();
    /// let result = db.transaction_with_retry(branch_id, config, |txn| {
    ///     let val = txn.get(&key)?;
    ///     txn.put(key.clone(), Value::Int(val.value + 1))?;
    ///     Ok(())
    /// })?;
    /// ```
    pub(crate) fn transaction_with_retry<F, T>(
        &self,
        branch_id: BranchId,
        config: RetryConfig,
        f: F,
    ) -> StrataResult<T>
    where
        F: Fn(&mut TransactionContext) -> StrataResult<T>,
    {
        self.check_accepting()?;

        let mut last_error = None;

        for attempt in 0..=config.max_retries {
            let mut txn = self.begin_transaction(branch_id);
            let result = f(&mut txn);
            let outcome = self.run_single_attempt(&mut txn, result, self.durability_mode);
            self.end_transaction(txn);

            match outcome {
                Ok((value, _)) => return Ok(value),
                Err(e) if e.is_conflict() && attempt < config.max_retries => {
                    last_error = Some(e);
                    std::thread::sleep(config.calculate_delay(attempt));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_error.unwrap_or_else(|| StrataError::conflict("Max retries exceeded".to_string())))
    }

    /// Begin a new transaction (for manual control)
    ///
    /// Returns a TransactionContext that must be manually committed or aborted.
    /// Prefer `transaction()` closure API for automatic handling.
    ///
    /// Uses thread-local pool to avoid allocation overhead after warmup.
    /// Call `end_transaction()` after commit/abort to return context to pool.
    ///
    /// # Arguments
    /// * `branch_id` - BranchId for namespace isolation
    ///
    /// # Returns
    /// * `TransactionContext` - Active transaction ready for operations
    ///
    /// # Example
    /// ```ignore
    /// let mut txn = db.begin_transaction(branch_id);
    /// txn.put(key, value)?;
    /// db.commit_transaction(&mut txn)?;
    /// db.end_transaction(txn); // Return to pool
    /// ```
    pub fn begin_transaction(&self, branch_id: BranchId) -> TransactionContext {
        let txn_id = self.coordinator.next_txn_id();
        let snapshot = self.storage.create_snapshot();
        self.coordinator.record_start();

        TransactionPool::acquire(txn_id, branch_id, Some(Box::new(snapshot)))
    }

    /// End a transaction (return to pool)
    ///
    /// Returns the transaction context to the thread-local pool for reuse.
    /// This avoids allocation overhead on subsequent transactions.
    ///
    /// Should be called after `commit_transaction()` or after aborting.
    /// The closure API (`transaction()`) calls this automatically.
    ///
    /// # Arguments
    /// * `ctx` - Transaction context to return to pool
    ///
    /// # Example
    /// ```ignore
    /// let mut txn = db.begin_transaction(branch_id);
    /// txn.put(key, value)?;
    /// db.commit_transaction(&mut txn)?;
    /// db.end_transaction(txn); // Return to pool for reuse
    /// ```
    pub fn end_transaction(&self, ctx: TransactionContext) {
        TransactionPool::release(ctx);
    }

    /// Commit a transaction
    ///
    /// Per spec commit sequence:
    /// 1. Validate (conflict detection)
    /// 2. Allocate commit version
    /// 3. Write to WAL (BeginTxn, Writes, CommitTxn)
    /// 4. Apply to storage
    ///
    /// # Arguments
    /// * `txn` - Transaction to commit
    ///
    /// # Returns
    /// * `Ok(commit_version)` - Transaction committed successfully, returns commit version
    /// * `Err(TransactionConflict)` - Validation failed, transaction aborted
    ///
    /// # Errors
    /// - `TransactionConflict` - Read-write or CAS conflict detected
    /// - `InvalidState` - Transaction not in Active state
    ///
    /// # Contract
    /// Returns the commit version (u64) assigned to all writes in this transaction.
    pub fn commit_transaction(&self, txn: &mut TransactionContext) -> StrataResult<u64> {
        self.commit_internal(txn, self.durability_mode)
    }

    /// Internal commit implementation shared by commit_transaction and transaction closures
    ///
    /// Delegates the commit protocol to the concurrency layer (TransactionManager)
    /// via the TransactionCoordinator. The engine is responsible only for:
    /// - Determining whether to pass the WAL (based on durability mode + persistence)
    ///
    /// The concurrency layer handles:
    /// - Per-run commit locking (TOCTOU prevention)
    /// - Validation (first-committer-wins)
    /// - Version allocation
    /// - WAL writing (when WAL reference is provided)
    /// - Storage application
    /// - Fsync (WAL::append handles fsync based on its DurabilityMode)
    fn commit_internal(
        &self,
        txn: &mut TransactionContext,
        durability: DurabilityMode,
    ) -> StrataResult<u64> {
        let mut wal_guard = if durability.requires_wal() {
            self.wal_writer.as_ref().map(|w| w.lock())
        } else {
            None
        };
        let wal_ref = wal_guard.as_deref_mut();

        self.coordinator.commit(txn, self.storage.as_ref(), wal_ref)
    }

    // ========================================================================
    // Graceful Shutdown
    // ========================================================================

    /// Graceful shutdown - ensures all data is persisted
    ///
    /// This method:
    /// 1. Stops accepting new transactions
    /// 2. Waits for pending operations to complete
    /// 3. Flushes WAL based on durability mode
    ///
    /// # Example
    ///
    /// ```ignore
    /// db.shutdown()?;
    /// assert!(!db.is_open());
    /// ```
    pub fn shutdown(&self) -> StrataResult<()> {
        // Stop accepting new transactions
        self.accepting_transactions.store(false, Ordering::SeqCst);

        // Signal the background flush thread to stop
        self.flush_shutdown.store(true, Ordering::SeqCst);

        // Join the flush thread so it releases the WAL lock
        if let Some(handle) = self.flush_handle.lock().take() {
            let _ = handle.join();
        }

        // Wait for in-flight transactions to complete
        // This ensures all transactions that started before shutdown
        // have a chance to commit before we flush the WAL.
        let timeout = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();

        while self.coordinator.active_count() > 0 && start.elapsed() < timeout {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Final flush to ensure all data is persisted
        self.flush()?;

        Ok(())
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // Stop the background flush thread
        self.flush_shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.flush_handle.lock().take() {
            let _ = handle.join();
        }

        // Final flush to persist any remaining data
        let _ = self.flush();

        // Remove from registry if we're disk-backed
        if self.persistence_mode == PersistenceMode::Disk && !self.data_dir.as_os_str().is_empty() {
            if let Ok(mut registry) = OPEN_DATABASES.lock() {
                registry.remove(&self.data_dir);
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_concurrency::TransactionPayload;
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;
    use strata_core::Storage;
    use strata_durability::format::WalRecord;
    use strata_durability::now_micros;
    use tempfile::TempDir;

    /// Helper: write a committed transaction to the segmented WAL
    fn write_wal_txn(
        wal_dir: &std::path::Path,
        txn_id: u64,
        branch_id: BranchId,
        puts: Vec<(Key, Value)>,
        deletes: Vec<Key>,
        version: u64,
    ) {
        let mut wal = WalWriter::new(
            wal_dir.to_path_buf(),
            [0u8; 16],
            DurabilityMode::Always,
            WalConfig::for_testing(),
            Box::new(IdentityCodec),
        )
        .unwrap();

        let payload = TransactionPayload {
            version,
            puts,
            deletes,
        };
        let record = WalRecord::new(
            txn_id,
            *branch_id.as_bytes(),
            now_micros(),
            payload.to_bytes(),
        );
        wal.append(&record).unwrap();
        wal.flush().unwrap();
    }

    #[test]
    fn test_open_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("new_db");

        assert!(!db_path.exists());
        let _db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn test_ephemeral_no_files() {
        let db = Database::cache().unwrap();

        // Should work for operations
        assert!(db.is_ephemeral());
    }

    #[test]
    fn test_wal_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let branch_id = BranchId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            branch_id,
        );

        // Write directly to segmented WAL (simulating a crash recovery scenario)
        {
            let wal_dir = db_path.join("wal");
            std::fs::create_dir_all(&wal_dir).unwrap();
            write_wal_txn(
                &wal_dir,
                1,
                branch_id,
                vec![(
                    Key::new_kv(ns.clone(), "key1"),
                    Value::Bytes(b"value1".to_vec()),
                )],
                vec![],
                1,
            );
        }

        // Open database (should replay WAL)
        let db = Database::open(&db_path).unwrap();

        // Storage should have data from WAL
        let key1 = Key::new_kv(ns, "key1");
        let val = db.storage().get(&key1).unwrap().unwrap();

        if let Value::Bytes(bytes) = val.value {
            assert_eq!(bytes, b"value1");
        } else {
            panic!("Wrong value type");
        }
    }

    #[test]
    fn test_open_close_reopen() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let branch_id = BranchId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            branch_id,
        );

        // Open database, write via transaction, close
        {
            let db = Database::open(&db_path).unwrap();

            db.transaction(branch_id, |txn| {
                txn.put(
                    Key::new_kv(ns.clone(), "persistent"),
                    Value::Bytes(b"data".to_vec()),
                )?;
                Ok(())
            })
            .unwrap();

            db.flush().unwrap();
        }

        // Reopen database
        {
            let db = Database::open(&db_path).unwrap();

            // Data should be restored from WAL
            let key = Key::new_kv(ns, "persistent");
            let val = db.storage().get(&key).unwrap().unwrap();

            if let Value::Bytes(bytes) = val.value {
                assert_eq!(bytes, b"data");
            } else {
                panic!("Wrong value type");
            }
        }
    }

    #[test]
    fn test_partial_record_discarded() {
        // With the segmented WAL, partial records (crash mid-write) are
        // automatically discarded by the reader. There are no "incomplete
        // transactions" since each WalRecord = one committed transaction.
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let branch_id = BranchId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            branch_id,
        );

        // Write one valid record, then append garbage to simulate crash
        {
            let wal_dir = db_path.join("wal");
            std::fs::create_dir_all(&wal_dir).unwrap();
            write_wal_txn(
                &wal_dir,
                1,
                branch_id,
                vec![(Key::new_kv(ns.clone(), "valid"), Value::Int(42))],
                vec![],
                1,
            );

            // Append garbage to simulate crash mid-write of second record
            let segment_path = strata_durability::format::WalSegment::segment_path(&wal_dir, 1);
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&segment_path)
                .unwrap();
            file.write_all(&[0xFF; 20]).unwrap();
        }

        // Open database (should recover valid record, skip garbage)
        let db = Database::open(&db_path).unwrap();

        // Valid transaction should be present
        let key = Key::new_kv(ns.clone(), "valid");
        let val = db.storage().get(&key).unwrap().unwrap();
        assert_eq!(val.value, Value::Int(42));
    }

    #[test]
    fn test_corrupted_wal_handled_gracefully() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Create WAL directory but with no valid segment files
        // (the segmented WAL will find no segments and return empty)
        {
            std::fs::create_dir_all(db_path.join("wal")).unwrap();
        }

        // Open should succeed with empty storage (no segments found)
        let result = Database::open(&db_path);
        assert!(result.is_ok());

        let db = result.unwrap();
        // Storage should be empty since no valid segments exist
        assert_eq!(db.storage().current_version(), 0);
    }

    #[test]
    fn test_open_with_different_durability_modes() {
        let temp_dir = TempDir::new().unwrap();

        // Always mode
        {
            let db =
                Database::open_with_mode(temp_dir.path().join("strict"), DurabilityMode::Always)
                    .unwrap();
            assert!(!db.is_ephemeral());
        }

        // Standard mode
        {
            let db = Database::open_with_mode(
                temp_dir.path().join("batched"),
                DurabilityMode::Standard {
                    interval_ms: 100,
                    batch_size: 1000,
                },
            )
            .unwrap();
            assert!(!db.is_ephemeral());
        }

        // Cache mode
        {
            let db = Database::open_with_mode(temp_dir.path().join("none"), DurabilityMode::Cache)
                .unwrap();
            assert!(!db.is_ephemeral());
        }
    }

    #[test]
    fn test_flush() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let db = Database::open(&db_path).unwrap();

        // Flush should succeed
        assert!(db.flush().is_ok());
    }

    // ========================================================================
    // Transaction API Tests
    // ========================================================================

    fn create_test_namespace(branch_id: BranchId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            branch_id,
        )
    }

    #[test]
    fn test_transaction_closure_api() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let branch_id = BranchId::new();
        let ns = create_test_namespace(branch_id);
        let key = Key::new_kv(ns, "test_key");

        // Execute transaction
        let result = db.transaction(branch_id, |txn| {
            txn.put(key.clone(), Value::Int(42))?;
            Ok(())
        });

        assert!(result.is_ok());

        // Verify data was committed
        let stored = db.storage().get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::Int(42));
    }

    #[test]
    fn test_transaction_returns_closure_value() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let branch_id = BranchId::new();
        let ns = create_test_namespace(branch_id);
        let key = Key::new_kv(ns, "test_key");

        // Pre-populate using transaction
        db.transaction(branch_id, |txn| {
            txn.put(key.clone(), Value::Int(100))?;
            Ok(())
        })
        .unwrap();

        // Transaction returns a value
        let result: StrataResult<i64> = db.transaction(branch_id, |txn| {
            let val = txn.get(&key)?.unwrap();
            if let Value::Int(n) = val {
                Ok(n)
            } else {
                Err(StrataError::invalid_input("wrong type".to_string()))
            }
        });

        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_transaction_read_your_writes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let branch_id = BranchId::new();
        let ns = create_test_namespace(branch_id);
        let key = Key::new_kv(ns, "ryw_key");

        // Per spec Section 2.1: "Its own uncommitted writes - always visible"
        let result: StrataResult<Value> = db.transaction(branch_id, |txn| {
            txn.put(key.clone(), Value::String("written".to_string()))?;

            // Should see our own write
            let val = txn.get(&key)?.unwrap();
            Ok(val)
        });

        assert_eq!(result.unwrap(), Value::String("written".to_string()));
    }

    #[test]
    fn test_transaction_aborts_on_closure_error() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let branch_id = BranchId::new();
        let ns = create_test_namespace(branch_id);
        let key = Key::new_kv(ns, "abort_key");

        // Transaction that errors
        let result: StrataResult<()> = db.transaction(branch_id, |txn| {
            txn.put(key.clone(), Value::Int(999))?;
            Err(StrataError::invalid_input("intentional error".to_string()))
        });

        assert!(result.is_err());

        // Data should NOT be committed
        assert!(db.storage().get(&key).unwrap().is_none());
    }

    #[test]
    fn test_begin_and_commit_manual() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let branch_id = BranchId::new();
        let ns = create_test_namespace(branch_id);
        let key = Key::new_kv(ns, "manual_key");

        // Manual transaction control
        let mut txn = db.begin_transaction(branch_id);
        txn.put(key.clone(), Value::Int(123)).unwrap();

        // Commit manually
        db.commit_transaction(&mut txn).unwrap();

        // Verify committed
        let stored = db.storage().get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::Int(123));
    }

    // ========================================================================
    // Retry Tests
    // ========================================================================

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 10);
        assert_eq!(config.max_delay_ms, 100);
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_retries(5)
            .with_base_delay_ms(20)
            .with_max_delay_ms(200);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 20);
        assert_eq!(config.max_delay_ms, 200);
    }

    #[test]
    fn test_retry_config_no_retry() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_retry_config_delay_calculation() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 10,
            max_delay_ms: 100,
        };

        // Exponential backoff: 10, 20, 40, 80, 100 (capped)
        assert_eq!(config.calculate_delay(0).as_millis(), 10);
        assert_eq!(config.calculate_delay(1).as_millis(), 20);
        assert_eq!(config.calculate_delay(2).as_millis(), 40);
        assert_eq!(config.calculate_delay(3).as_millis(), 80);
        assert_eq!(config.calculate_delay(4).as_millis(), 100); // Capped at max
        assert_eq!(config.calculate_delay(5).as_millis(), 100); // Still capped
    }

    // ========================================================================
    // Graceful Shutdown Tests
    // ========================================================================

    #[test]
    fn test_is_open_initially_true() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        // Database should be open initially
        assert!(db.is_open());
    }

    #[test]
    fn test_shutdown_sets_not_open() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        assert!(db.is_open());

        // Shutdown should succeed
        assert!(db.shutdown().is_ok());

        // Database should no longer be open
        assert!(!db.is_open());
    }

    #[test]
    fn test_shutdown_rejects_new_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let branch_id = BranchId::new();
        let ns = create_test_namespace(branch_id);
        let key = Key::new_kv(ns, "after_shutdown");

        // Shutdown the database
        db.shutdown().unwrap();

        // New transactions should be rejected
        let result = db.transaction(branch_id, |txn| {
            txn.put(key.clone(), Value::Int(42))?;
            Ok(())
        });

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, StrataError::InvalidInput { .. }));
    }

    #[test]
    fn test_shutdown_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        // Multiple shutdowns should be safe
        assert!(db.shutdown().is_ok());
        assert!(db.shutdown().is_ok());
        assert!(db.shutdown().is_ok());

        // Should remain not open
        assert!(!db.is_open());
    }

    // ========================================================================
    // Singleton Registry Tests
    // ========================================================================

    #[test]
    fn test_open_same_path_returns_same_instance() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("singleton_test");

        // Open database twice with same path
        let db1 = Database::open(&db_path).unwrap();
        let db2 = Database::open(&db_path).unwrap();

        // Both should be the same Arc (same pointer)
        assert!(Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_open_different_paths_returns_different_instances() {
        let temp_dir = TempDir::new().unwrap();
        let path1 = temp_dir.path().join("db1");
        let path2 = temp_dir.path().join("db2");

        let db1 = Database::open(&path1).unwrap();
        let db2 = Database::open(&path2).unwrap();

        // Should be different instances
        assert!(!Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_ephemeral_not_registered() {
        // Create two cache databases
        let db1 = Database::cache().unwrap();
        let db2 = Database::cache().unwrap();

        // They should be different instances (not shared via registry)
        assert!(!Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_builder_open_uses_registry() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("builder_singleton");

        // Open via builder
        let db1 = Database::builder()
            .path(&db_path)
            .cache()
            .open()
            .unwrap();

        // Open via Database::open
        let db2 = Database::open(&db_path).unwrap();

        // Should be same instance
        assert!(Arc::ptr_eq(&db1, &db2));
    }

    #[test]
    fn test_database_builder_open_with_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("builder_test");

        let db = Database::builder().path(&db_path).always().open().unwrap();

        assert!(!db.is_ephemeral());
        assert!(db_path.exists());
    }

    #[test]
    fn test_database_builder_open_requires_path() {
        // Builder without path should fail
        let result = Database::builder().open();
        assert!(result.is_err());
    }
}
