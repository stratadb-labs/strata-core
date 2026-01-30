//! WAL Replay Logic for Recovery
//!
//! This module implements WAL replay to restore storage state from the write-ahead log.
//! It scans WAL entries, groups them by transaction ID, and applies only committed
//! transactions to storage.
//!
//! ## Replay Process
//!
//! 1. Scan WAL entries from the beginning
//! 2. Group entries by txn_id into Transaction structs
//! 3. Identify committed transactions (those with CommitTxn entry)
//! 4. Discard incomplete transactions (BeginTxn without CommitTxn - crashed mid-transaction)
//! 5. Apply committed transactions in order to storage
//! 6. Preserve version numbers from WAL (don't allocate new versions)
//!
//! ## Version Preservation
//!
//! CRITICAL: Replay must preserve the exact version numbers from WAL entries.
//! This ensures that after replay, the database state is identical to what it was
//! before the crash. We use `put_with_version()` and `delete_with_version()` to
//! bypass normal version allocation.

use crate::wal::{WALEntry, WAL};
use strata_core::StrataResult;
use strata_core::primitives::json::{delete_at_path, set_at_path, JsonValue};
use strata_core::traits::Storage;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::warn;

/// Statistics from WAL replay
///
/// Tracks how many transactions and operations were applied during replay,
/// useful for debugging and monitoring recovery performance.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReplayStats {
    /// Number of committed transactions that were applied
    pub txns_applied: usize,
    /// Number of Write operations applied
    pub writes_applied: usize,
    /// Number of Delete operations applied
    pub deletes_applied: usize,
    /// Final version after replay (highest version seen in WAL)
    pub final_version: u64,
    /// Maximum transaction ID seen in WAL
    ///
    /// This is critical for initializing the TransactionManager after recovery
    /// to ensure new transactions get unique IDs that don't conflict with
    /// transactions in the WAL.
    pub max_txn_id: u64,
    /// Number of incomplete transactions discarded (no CommitTxn)
    pub incomplete_txns: usize,
    /// Number of aborted transactions discarded
    pub aborted_txns: usize,
    /// Number of orphaned entries discarded (Write/Delete without BeginTxn)
    pub orphaned_entries: usize,
    /// Number of transactions skipped by filter
    pub txns_filtered: usize,
    /// Number of corrupted entries detected during WAL read
    ///
    /// When the WAL reader encounters CRC mismatches or deserialization failures,
    /// it stops reading and records the count. This helps diagnose disk corruption
    /// or partial write issues.
    pub corrupted_entries: usize,
    // JSON operations
    /// Number of JSON Create operations applied
    pub json_creates_applied: usize,
    /// Number of JSON Set operations applied
    pub json_sets_applied: usize,
    /// Number of JSON Delete operations applied
    pub json_deletes_applied: usize,
    /// Number of JSON Destroy operations applied
    pub json_destroys_applied: usize,
}

/// JSON document structure for recovery
///
/// This mirrors the JsonDoc struct in primitives but is defined here to avoid
/// circular dependencies. Uses msgpack serialization for compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryJsonDoc {
    /// Document identifier (user-provided string key)
    id: String,
    /// JSON value
    value: JsonValue,
    /// Document version (increments on any change)
    version: u64,
    /// Creation timestamp (millis since epoch)
    created_at: i64,
    /// Last update timestamp (millis since epoch)
    updated_at: i64,
}

impl RecoveryJsonDoc {
    /// Create a new document with initial value
    fn new(id: impl Into<String>, value: JsonValue, version: u64, timestamp: i64) -> Self {
        Self {
            id: id.into(),
            value,
            version,
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    /// Serialize to msgpack bytes
    fn to_bytes(&self) -> StrataResult<Vec<u8>> {
        rmp_serde::to_vec(self)
            .map_err(|e| strata_core::StrataError::serialization(e.to_string()))
    }

    /// Deserialize from msgpack bytes
    fn from_bytes(bytes: &[u8]) -> StrataResult<Self> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| strata_core::StrataError::serialization(e.to_string()))
    }
}

/// Options for WAL replay
///
/// Allows filtering and controlling replay behavior.
#[derive(Default, Clone)]
pub struct ReplayOptions {
    /// Only replay transactions for this run_id (None = all)
    pub filter_run_id: Option<RunId>,
    /// Stop replay at this version (None = replay all)
    ///
    /// Transactions with commit_version > stop_at_version will not be applied.
    pub stop_at_version: Option<u64>,
    /// Callback for progress reporting (called after each transaction)
    ///
    /// Note: Using Arc<dyn Fn> for thread-safe callback support.
    pub progress_callback: Option<std::sync::Arc<dyn Fn(ReplayProgress) + Send + Sync>>,
}

impl std::fmt::Debug for ReplayOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayOptions")
            .field("filter_run_id", &self.filter_run_id)
            .field("stop_at_version", &self.stop_at_version)
            .field(
                "progress_callback",
                &self.progress_callback.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

/// Progress information during replay
///
/// Provided to progress callbacks after each transaction is processed.
#[derive(Debug, Clone)]
pub struct ReplayProgress {
    /// Transaction ID being processed
    pub current_txn_id: u64,
    /// Total transactions found so far (committed + incomplete + aborted)
    pub total_txns_found: usize,
    /// Transactions applied so far
    pub txns_applied: usize,
    /// Current max version seen
    pub current_version: u64,
    /// Whether this transaction was applied (true) or skipped (false)
    pub was_applied: bool,
}

/// Transaction state during replay
///
/// Groups WAL entries belonging to a single transaction.
/// A transaction is committed only if it has a CommitTxn entry.
#[derive(Debug)]
struct Transaction {
    /// Transaction identifier (stored for debugging via Debug trait)
    #[allow(dead_code)]
    txn_id: u64,
    /// Run this transaction belongs to (stored for debugging via Debug trait)
    #[allow(dead_code)]
    run_id: RunId,
    /// Entries in this transaction (in order)
    entries: Vec<WALEntry>,
    /// Whether this transaction was committed
    committed: bool,
    /// Whether this transaction was aborted
    aborted: bool,
}

impl Transaction {
    /// Create a new transaction from a BeginTxn entry
    fn new(txn_id: u64, run_id: RunId) -> Self {
        Self {
            txn_id,
            run_id,
            entries: Vec::new(),
            committed: false,
            aborted: false,
        }
    }
}

// ============================================================================
// Validation Types and Functions
// ============================================================================

/// Result of validating WAL entries before replay
///
/// Contains information about incomplete transactions and orphaned entries
/// that will be discarded during replay.
#[derive(Debug, Default)]
pub struct ValidationResult {
    /// Transaction IDs that are incomplete (BeginTxn without CommitTxn/AbortTxn)
    pub incomplete_txns: Vec<u64>,
    /// Number of orphaned entries (Write/Delete without matching BeginTxn)
    pub orphaned_entries: usize,
    /// Warnings generated during validation
    pub warnings: Vec<ValidationWarning>,
}

/// A warning generated during WAL validation
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// Index of the entry in the WAL
    pub entry_index: usize,
    /// Description of the warning
    pub message: String,
}

impl ValidationResult {
    /// Returns true if no issues were found during validation
    pub fn is_clean(&self) -> bool {
        self.incomplete_txns.is_empty() && self.orphaned_entries == 0
    }

    /// Log all warnings to the tracing subsystem
    pub fn log_warnings(&self) {
        if !self.incomplete_txns.is_empty() {
            warn!(
                count = self.incomplete_txns.len(),
                txn_ids = ?self.incomplete_txns,
                "Discarding incomplete transactions (no CommitTxn)"
            );
        }

        if self.orphaned_entries > 0 {
            warn!(
                count = self.orphaned_entries,
                "Discarding orphaned entries (Write/Delete without BeginTxn)"
            );
        }

        for warning in &self.warnings {
            warn!(
                entry_index = warning.entry_index,
                message = %warning.message,
                "Validation warning"
            );
        }
    }
}

/// Validate WAL entries before replay
///
/// Scans entries to identify:
/// - Incomplete transactions (BeginTxn without CommitTxn/AbortTxn)
/// - Orphaned entries (Write/Delete without matching BeginTxn)
/// - Duplicate BeginTxn for the same txn_id
/// - CommitTxn without BeginTxn
///
/// This validation is informational - it doesn't prevent replay but logs
/// warnings about data that will be discarded.
///
/// # Arguments
///
/// * `entries` - The WAL entries to validate
///
/// # Returns
///
/// A `ValidationResult` containing information about issues found
pub fn validate_transactions(entries: &[WALEntry]) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Track which transactions we've seen
    let mut begun_txns: HashSet<u64> = HashSet::new();
    let mut committed_txns: HashSet<u64> = HashSet::new();
    let mut aborted_txns: HashSet<u64> = HashSet::new();

    // Track active transaction per run_id (for orphan detection)
    let mut active_txn_per_run: HashMap<RunId, u64> = HashMap::new();

    for (idx, entry) in entries.iter().enumerate() {
        match entry {
            WALEntry::BeginTxn { txn_id, run_id, .. } => {
                if begun_txns.contains(txn_id) {
                    result.warnings.push(ValidationWarning {
                        entry_index: idx,
                        message: format!("Duplicate BeginTxn for txn_id {}", txn_id),
                    });
                }
                begun_txns.insert(*txn_id);
                active_txn_per_run.insert(*run_id, *txn_id);
            }
            WALEntry::Write { run_id, .. }
            | WALEntry::Delete { run_id, .. }
            | WALEntry::JsonCreate { run_id, .. }
            | WALEntry::JsonSet { run_id, .. }
            | WALEntry::JsonDelete { run_id, .. }
            | WALEntry::JsonDestroy { run_id, .. }
            // Vector operations
            | WALEntry::VectorCollectionCreate { run_id, .. }
            | WALEntry::VectorCollectionDelete { run_id, .. }
            | WALEntry::VectorUpsert { run_id, .. }
            | WALEntry::VectorDelete { run_id, .. } => {
                // Check if there's an active transaction for this run_id
                if !active_txn_per_run.contains_key(run_id) {
                    result.warnings.push(ValidationWarning {
                        entry_index: idx,
                        message: format!(
                            "Orphaned entry: no active transaction for run_id {:?}",
                            run_id
                        ),
                    });
                    result.orphaned_entries += 1;
                }
            }
            WALEntry::CommitTxn { txn_id, run_id } => {
                if !begun_txns.contains(txn_id) {
                    result.warnings.push(ValidationWarning {
                        entry_index: idx,
                        message: format!("CommitTxn without BeginTxn for txn_id {}", txn_id),
                    });
                }
                committed_txns.insert(*txn_id);
                // Clear active transaction for this run_id
                if active_txn_per_run.get(run_id) == Some(txn_id) {
                    active_txn_per_run.remove(run_id);
                }
            }
            WALEntry::AbortTxn { txn_id, run_id } => {
                if !begun_txns.contains(txn_id) {
                    result.warnings.push(ValidationWarning {
                        entry_index: idx,
                        message: format!("AbortTxn without BeginTxn for txn_id {}", txn_id),
                    });
                }
                aborted_txns.insert(*txn_id);
                // Clear active transaction for this run_id
                if active_txn_per_run.get(run_id) == Some(txn_id) {
                    active_txn_per_run.remove(run_id);
                }
            }
            WALEntry::Checkpoint { .. } => {
                // Checkpoints are always valid
            }
        }
    }

    // Find incomplete transactions (begun but neither committed nor aborted)
    for txn_id in &begun_txns {
        if !committed_txns.contains(txn_id) && !aborted_txns.contains(txn_id) {
            result.incomplete_txns.push(*txn_id);
        }
    }

    // Sort for deterministic output
    result.incomplete_txns.sort();

    result
}

/// Replay WAL entries to restore storage state
///
/// This is the main recovery function. It scans all WAL entries, groups them
/// by transaction, and applies only committed transactions to storage.
///
/// # Arguments
///
/// * `wal` - The WAL to replay from
/// * `storage` - The storage to apply transactions to (must be empty or at checkpoint)
///
/// # Returns
///
/// * `Ok(ReplayStats)` - Statistics about the replay
/// * `Err` - If reading WAL or applying transactions fails
///
/// # Example
///
/// ```ignore
/// use strata_durability::recovery::replay_wal;
/// use strata_durability::wal::{WAL, DurabilityMode};
/// use strata_storage::ShardedStore;
///
/// let wal = WAL::open("data/wal/segment.wal", DurabilityMode::default())?;
/// let storage = ShardedStore::new();
/// let stats = replay_wal(&wal, &storage)?;
/// println!("Applied {} transactions, {} writes", stats.txns_applied, stats.writes_applied);
/// ```
pub fn replay_wal<S: Storage + ?Sized>(wal: &WAL, storage: &S) -> StrataResult<ReplayStats> {
    replay_wal_with_options(wal, storage, ReplayOptions::default())
}

/// Replay WAL entries with filtering and progress options
///
/// Extended version of `replay_wal` that supports:
/// - Filtering by run_id (only replay transactions for a specific run)
/// - Stopping at a specific version
/// - Progress callbacks for monitoring replay progress
///
/// # Arguments
///
/// * `wal` - The WAL to replay from
/// * `storage` - The storage to apply transactions to
/// * `options` - Replay options for filtering and callbacks
///
/// # Returns
///
/// * `Ok(ReplayStats)` - Statistics about the replay including filtered count
/// * `Err` - If reading WAL or applying transactions fails
///
/// # Example
///
/// ```ignore
/// use strata_durability::recovery::{replay_wal_with_options, ReplayOptions};
/// use strata_durability::wal::{WAL, DurabilityMode};
/// use strata_storage::ShardedStore;
/// use std::sync::Arc;
///
/// let wal = WAL::open("data/wal/segment.wal", DurabilityMode::default())?;
/// let storage = ShardedStore::new();
///
/// let options = ReplayOptions {
///     filter_run_id: Some(my_run_id),
///     stop_at_version: Some(100),
///     progress_callback: Some(Arc::new(|progress| {
///         println!("Replayed txn {}", progress.current_txn_id);
///     })),
/// };
///
/// let stats = replay_wal_with_options(&wal, &storage, options)?;
/// println!("Applied {} transactions, filtered {}", stats.txns_applied, stats.txns_filtered);
/// ```
pub fn replay_wal_with_options<S: Storage + ?Sized>(
    wal: &WAL,
    storage: &S,
    options: ReplayOptions,
) -> StrataResult<ReplayStats> {
    // Read all entries from WAL
    let entries = wal.read_all()?;

    // Validate entries and log warnings about discarded data
    let validation = validate_transactions(&entries);
    validation.log_warnings();

    // Group entries by transaction using internal sequential IDs
    // This handles the case where txn_ids are reused across database sessions
    // (e.g., if the TransactionManager was restarted and started from 1 again)
    let mut transactions: HashMap<u64, Transaction> = HashMap::new();

    // Internal counter for unique transaction grouping
    // This ensures each BeginTxn gets a unique slot even if txn_ids are reused
    let mut internal_id_counter: u64 = 0;

    // Track the currently active transaction's internal ID for each run_id
    // Maps run_id -> internal_id of the currently active transaction
    let mut active_txn_per_run: HashMap<RunId, u64> = HashMap::new();

    // Map from (run_id, original_txn_id) to internal_id for looking up commits/aborts
    // This helps match CommitTxn/AbortTxn to the correct transaction when there are
    // duplicate txn_ids
    let mut txn_id_to_internal: HashMap<(RunId, u64), u64> = HashMap::new();

    let mut max_version: u64 = 0;
    let mut max_txn_id: u64 = 0;
    let mut orphaned_count: usize = 0;

    for entry in entries {
        // Track max version for final_version stat
        if let Some(version) = entry.version() {
            max_version = max_version.max(version);
        }

        match &entry {
            WALEntry::BeginTxn { txn_id, run_id, .. } => {
                // Track max txn_id for TransactionManager initialization
                max_txn_id = max_txn_id.max(*txn_id);

                // Allocate a new internal ID for this transaction
                internal_id_counter += 1;
                let internal_id = internal_id_counter;

                // Create the transaction with the internal ID
                transactions.insert(internal_id, Transaction::new(*txn_id, *run_id));

                // Track this as the active transaction for this run_id
                active_txn_per_run.insert(*run_id, internal_id);

                // Map (run_id, txn_id) -> internal_id for commit/abort lookup
                txn_id_to_internal.insert((*run_id, *txn_id), internal_id);
            }
            WALEntry::Write { run_id, .. }
            | WALEntry::Delete { run_id, .. }
            | WALEntry::JsonCreate { run_id, .. }
            | WALEntry::JsonSet { run_id, .. }
            | WALEntry::JsonDelete { run_id, .. }
            | WALEntry::JsonDestroy { run_id, .. }
            // Vector operations
            | WALEntry::VectorCollectionCreate { run_id, .. }
            | WALEntry::VectorCollectionDelete { run_id, .. }
            | WALEntry::VectorUpsert { run_id, .. }
            | WALEntry::VectorDelete { run_id, .. } => {
                // Add to the currently active transaction for this run_id
                if let Some(&internal_id) = active_txn_per_run.get(run_id) {
                    if let Some(txn) = transactions.get_mut(&internal_id) {
                        txn.entries.push(entry.clone());
                    }
                } else {
                    // Orphaned entry - no active transaction for this run_id
                    orphaned_count += 1;
                }
            }
            WALEntry::CommitTxn { txn_id, run_id } => {
                // Look up the internal ID for this (run_id, txn_id) pair
                if let Some(&internal_id) = txn_id_to_internal.get(&(*run_id, *txn_id)) {
                    // Mark transaction as committed
                    if let Some(txn) = transactions.get_mut(&internal_id) {
                        txn.committed = true;
                    }
                    // Clear active transaction for this run_id if it matches
                    if active_txn_per_run.get(run_id) == Some(&internal_id) {
                        active_txn_per_run.remove(run_id);
                    }
                }
            }
            WALEntry::AbortTxn { txn_id, run_id } => {
                // Look up the internal ID for this (run_id, txn_id) pair
                if let Some(&internal_id) = txn_id_to_internal.get(&(*run_id, *txn_id)) {
                    // Mark transaction as aborted
                    if let Some(txn) = transactions.get_mut(&internal_id) {
                        txn.aborted = true;
                    }
                    // Clear active transaction for this run_id if it matches
                    if active_txn_per_run.get(run_id) == Some(&internal_id) {
                        active_txn_per_run.remove(run_id);
                    }
                }
            }
            WALEntry::Checkpoint { version, .. } => {
                // Track checkpoint version
                max_version = max_version.max(*version);
            }
        }
    }

    // Apply committed transactions and collect stats
    let mut stats = ReplayStats {
        final_version: max_version,
        max_txn_id,
        ..Default::default()
    };

    // Sort transactions by txn_id to ensure deterministic replay order
    let mut txn_ids: Vec<u64> = transactions.keys().copied().collect();
    txn_ids.sort();

    let total_txns = txn_ids.len();

    for txn_id in txn_ids {
        // Safety: txn_id comes from transactions.keys(), so it must exist
        let txn = transactions
            .get(&txn_id)
            .expect("txn_id from keys() must exist in map");

        // Determine if this transaction should be applied
        let mut was_applied = false;

        if txn.committed {
            // Check run_id filter
            if let Some(filter_run_id) = &options.filter_run_id {
                if txn.run_id != *filter_run_id {
                    stats.txns_filtered += 1;
                    // Still call progress callback even for filtered transactions
                    if let Some(ref callback) = options.progress_callback {
                        callback(ReplayProgress {
                            current_txn_id: txn_id,
                            total_txns_found: total_txns,
                            txns_applied: stats.txns_applied,
                            current_version: stats.final_version,
                            was_applied: false,
                        });
                    }
                    continue;
                }
            }

            // Check stop_at_version - get max version from transaction entries
            let txn_max_version = get_transaction_max_version(txn);
            if let Some(stop_version) = options.stop_at_version {
                if txn_max_version > stop_version {
                    stats.txns_filtered += 1;
                    // Call progress callback
                    if let Some(ref callback) = options.progress_callback {
                        callback(ReplayProgress {
                            current_txn_id: txn_id,
                            total_txns_found: total_txns,
                            txns_applied: stats.txns_applied,
                            current_version: stats.final_version,
                            was_applied: false,
                        });
                    }
                    continue;
                }
            }

            // Apply the transaction
            apply_transaction(storage, txn, &mut stats)?;
            was_applied = true;
        } else if txn.aborted {
            stats.aborted_txns += 1;
        } else {
            // Incomplete transaction (no CommitTxn or AbortTxn)
            stats.incomplete_txns += 1;
        }

        // Call progress callback
        if let Some(ref callback) = options.progress_callback {
            callback(ReplayProgress {
                current_txn_id: txn_id,
                total_txns_found: total_txns,
                txns_applied: stats.txns_applied,
                current_version: stats.final_version,
                was_applied,
            });
        }
    }

    // Record orphaned entries count
    stats.orphaned_entries = orphaned_count;

    Ok(stats)
}

/// Get the maximum version from a transaction's entries
fn get_transaction_max_version(txn: &Transaction) -> u64 {
    txn.entries
        .iter()
        .filter_map(|entry| entry.version())
        .max()
        .unwrap_or(0)
}

/// Apply a committed transaction to storage
///
/// Applies all Write, Delete, and JSON operations from a transaction to storage,
/// preserving the version numbers from the WAL entries.
///
/// # Arguments
///
/// * `storage` - The storage to apply to
/// * `txn` - The committed transaction to apply
/// * `stats` - Statistics to update
///
/// # Returns
///
/// * `Ok(())` - If all operations succeeded
/// * `Err` - If any operation fails
fn apply_transaction<S: Storage + ?Sized>(
    storage: &S,
    txn: &Transaction,
    stats: &mut ReplayStats,
) -> StrataResult<()> {
    for entry in &txn.entries {
        match entry {
            WALEntry::Write {
                key,
                value,
                version,
                ..
            } => {
                // Apply write, preserving version from WAL
                storage.put_with_version(key.clone(), value.clone(), *version, None)?;
                stats.writes_applied += 1;
                stats.final_version = stats.final_version.max(*version);
            }
            WALEntry::Delete { key, version, .. } => {
                // Apply delete, preserving version from WAL
                storage.delete_with_version(key, *version)?;
                stats.deletes_applied += 1;
                stats.final_version = stats.final_version.max(*version);
            }

            // ================================================================
            // JSON Operations
            // ================================================================
            WALEntry::JsonCreate {
                run_id,
                doc_id,
                value_bytes,
                version,
                timestamp,
            } => {
                // Deserialize the JSON value from msgpack bytes
                let value: JsonValue = rmp_serde::from_slice(value_bytes).map_err(|e| {
                    strata_core::StrataError::serialization(format!(
                        "Failed to deserialize JSON value during recovery: {}",
                        e
                    ))
                })?;

                // Create the document
                // Convert Timestamp (u64 microseconds) to i64 seconds for recovery format
                let timestamp_secs = (timestamp.as_micros() / 1_000_000) as i64;
                let doc = RecoveryJsonDoc::new(doc_id.clone(), value, *version, timestamp_secs);
                let doc_bytes = doc.to_bytes()?;

                // Store using JSON key
                let key = Key::new_json(Namespace::for_run(*run_id), doc_id);
                storage.put_with_version(key, Value::Bytes(doc_bytes), *version, None)?;

                stats.json_creates_applied += 1;
                stats.final_version = stats.final_version.max(*version);
            }

            WALEntry::JsonSet {
                run_id,
                doc_id,
                path,
                value_bytes,
                version,
            } => {
                let key = Key::new_json(Namespace::for_run(*run_id), doc_id);

                // Load existing document
                let existing = storage.get(&key)?;
                if let Some(vv) = existing {
                    let mut doc = match &vv.value {
                        Value::Bytes(bytes) => RecoveryJsonDoc::from_bytes(bytes)?,
                        _ => {
                            return Err(strata_core::StrataError::invalid_input(
                                "Expected bytes for JSON document".to_string(),
                            ))
                        }
                    };

                    // Deserialize the new value
                    let new_value: JsonValue = rmp_serde::from_slice(value_bytes).map_err(|e| {
                        strata_core::StrataError::serialization(format!(
                            "Failed to deserialize JSON value during recovery: {}",
                            e
                        ))
                    })?;

                    // Apply the path mutation
                    set_at_path(&mut doc.value, path, new_value).map_err(|e| {
                        strata_core::StrataError::invalid_input(format!(
                            "Failed to set path during recovery: {}",
                            e
                        ))
                    })?;

                    // Update version and timestamp
                    doc.version = *version;
                    doc.updated_at = std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;

                    // Store updated document
                    let doc_bytes = doc.to_bytes()?;
                    storage.put_with_version(key, Value::Bytes(doc_bytes), *version, None)?;

                    stats.json_sets_applied += 1;
                    stats.final_version = stats.final_version.max(*version);
                } else {
                    warn!(
                        "JsonSet for non-existent document {:?} during recovery, skipping",
                        doc_id
                    );
                }
            }

            WALEntry::JsonDelete {
                run_id,
                doc_id,
                path,
                version,
            } => {
                let key = Key::new_json(Namespace::for_run(*run_id), doc_id);

                // Load existing document
                let existing = storage.get(&key)?;
                if let Some(vv) = existing {
                    let mut doc = match &vv.value {
                        Value::Bytes(bytes) => RecoveryJsonDoc::from_bytes(bytes)?,
                        _ => {
                            return Err(strata_core::StrataError::invalid_input(
                                "Expected bytes for JSON document".to_string(),
                            ))
                        }
                    };

                    // Apply the path deletion
                    delete_at_path(&mut doc.value, path).map_err(|e| {
                        strata_core::StrataError::invalid_input(format!(
                            "Failed to delete path during recovery: {}",
                            e
                        ))
                    })?;

                    // Update version and timestamp
                    doc.version = *version;
                    doc.updated_at = std::time::SystemTime::now()
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;

                    // Store updated document
                    let doc_bytes = doc.to_bytes()?;
                    storage.put_with_version(key, Value::Bytes(doc_bytes), *version, None)?;

                    stats.json_deletes_applied += 1;
                    stats.final_version = stats.final_version.max(*version);
                } else {
                    warn!(
                        "JsonDelete for non-existent document {:?} during recovery, skipping",
                        doc_id
                    );
                }
            }

            WALEntry::JsonDestroy { run_id, doc_id } => {
                let key = Key::new_json(Namespace::for_run(*run_id), doc_id);

                // Delete the document (use version 0 since JsonDestroy doesn't carry version)
                // The version doesn't matter much for deletes as long as it's applied
                if storage.get(&key)?.is_some() {
                    storage.delete_with_version(&key, 0)?;
                    stats.json_destroys_applied += 1;
                } else {
                    // Document already doesn't exist - idempotent
                    stats.json_destroys_applied += 1;
                }
            }

            _ => {
                // BeginTxn, CommitTxn, etc. are not applied to storage
            }
        }
    }

    stats.txns_applied += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::DurabilityMode;
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;
    use strata_core::Timestamp;
    use strata_core::Storage; // Need trait in scope for .get() and .current_version()
    use strata_storage::ShardedStore; // Used in tests (still implements Storage)
    use tempfile::TempDir;

    /// Helper to get current timestamp
    fn now() -> Timestamp {
        Timestamp::now()
    }

    #[test]
    fn test_replay_empty_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();

        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.deletes_applied, 0);
        assert_eq!(stats.incomplete_txns, 0);
        assert_eq!(stats.aborted_txns, 0);
        assert_eq!(stats.orphaned_entries, 0);
    }

    #[test]
    fn test_replay_committed_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("committed.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write committed transaction to WAL
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"value1".to_vec()),
                version: 1,
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::String("value2".to_string()),
                version: 2,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay to storage
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.writes_applied, 2);
        assert_eq!(stats.deletes_applied, 0);
        assert_eq!(stats.incomplete_txns, 0);
        assert_eq!(stats.final_version, 2);

        // Verify storage has data with correct versions
        let key1 = Key::new_kv(ns.clone(), "key1");
        let val1 = store.get(&key1).unwrap().unwrap();
        assert_eq!(val1.value, Value::Bytes(b"value1".to_vec()));
        assert_eq!(val1.version.as_u64(), 1);

        let key2 = Key::new_kv(ns.clone(), "key2");
        let val2 = store.get(&key2).unwrap().unwrap();
        assert_eq!(val2.value, Value::String("value2".to_string()));
        assert_eq!(val2.version.as_u64(), 2);
    }

    #[test]
    fn test_replay_discards_incomplete_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("incomplete.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write incomplete transaction (no CommitTxn - simulates crash)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"value1".to_vec()),
                version: 1,
            })
            .unwrap();

            // NO CommitTxn - simulates crash
        }

        // Replay to storage
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.incomplete_txns, 1);

        // Storage should be empty
        let key1 = Key::new_kv(ns, "key1");
        assert!(store.get(&key1).unwrap().is_none());
    }

    #[test]
    fn test_replay_discards_aborted_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("aborted.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write aborted transaction
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"value1".to_vec()),
                version: 1,
            })
            .unwrap();

            wal.append(&WALEntry::AbortTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay to storage
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.aborted_txns, 1);
        assert_eq!(stats.incomplete_txns, 0);

        // Storage should be empty
        let key1 = Key::new_kv(ns, "key1");
        assert!(store.get(&key1).unwrap().is_none());
    }

    #[test]
    fn test_replay_multiple_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("multi.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write 3 transactions: 2 committed, 1 incomplete
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1 - committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"v1".to_vec()),
                version: 1,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2 - incomplete (no commit)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::Bytes(b"v2".to_vec()),
                version: 2,
            })
            .unwrap();
            // NO CommitTxn

            // Txn 3 - committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::Bytes(b"v3".to_vec()),
                version: 3,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 2); // Txn 1 and 3
        assert_eq!(stats.writes_applied, 2);
        assert_eq!(stats.incomplete_txns, 1); // Txn 2

        // Verify key1 and key3 exist, key2 doesn't
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key1"))
            .unwrap()
            .is_some());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key2"))
            .unwrap()
            .is_none());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key3"))
            .unwrap()
            .is_some());
    }

    #[test]
    fn test_replay_with_deletes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("deletes.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write transaction with write then delete
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Write then delete same key
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"v1".to_vec()),
                version: 1,
            })
            .unwrap();

            wal.append(&WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                version: 2,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.writes_applied, 1);
        assert_eq!(stats.deletes_applied, 1);

        // Key should be deleted (final state)
        assert!(store.get(&Key::new_kv(ns, "key1")).unwrap().is_none());
    }

    #[test]
    fn test_replay_preserves_versions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("versions.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write with specific versions (not 1, 2, 3 but 100, 200, 300)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Int(100),
                version: 100, // Non-sequential version
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::Int(200),
                version: 200,
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::Int(300),
                version: 300,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.final_version, 300);

        // Verify versions are preserved exactly
        let key1 = Key::new_kv(ns.clone(), "key1");
        let val1 = store.get(&key1).unwrap().unwrap();
        assert_eq!(val1.version.as_u64(), 100); // Version preserved, not re-allocated

        let key2 = Key::new_kv(ns.clone(), "key2");
        let val2 = store.get(&key2).unwrap().unwrap();
        assert_eq!(val2.version.as_u64(), 200);

        let key3 = Key::new_kv(ns.clone(), "key3");
        let val3 = store.get(&key3).unwrap().unwrap();
        assert_eq!(val3.version.as_u64(), 300);

        // Global version should reflect max version from WAL
        assert_eq!(store.current_version(), 300);
    }

    // ========================================
    // Validation Tests
    // ========================================

    #[test]
    fn test_validate_complete_transaction() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"v1".to_vec()),
                version: 1,
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
        ];

        let result = validate_transactions(&entries);
        assert!(result.is_clean());
        assert!(result.incomplete_txns.is_empty());
        assert_eq!(result.orphaned_entries, 0);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_incomplete_transaction() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns, "key1"),
                value: Value::Bytes(b"v1".to_vec()),
                version: 1,
            },
            // NO CommitTxn
        ];

        let result = validate_transactions(&entries);
        assert!(!result.is_clean());
        assert_eq!(result.incomplete_txns.len(), 1);
        assert_eq!(result.incomplete_txns[0], 1);
        assert_eq!(result.orphaned_entries, 0);
    }

    #[test]
    fn test_validate_orphaned_entry() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write without BeginTxn
        let entries = vec![WALEntry::Write {
            run_id,
            key: Key::new_kv(ns, "key1"),
            value: Value::Bytes(b"v1".to_vec()),
            version: 1,
        }];

        let result = validate_transactions(&entries);
        assert!(!result.is_clean());
        assert_eq!(result.orphaned_entries, 1);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("Orphaned"));
    }

    #[test]
    fn test_validate_duplicate_begin_txn() {
        let run_id = RunId::new();

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }, // Duplicate
            WALEntry::CommitTxn { txn_id: 1, run_id },
        ];

        let result = validate_transactions(&entries);
        // Transaction is complete, but there's a warning about duplicate
        assert!(result.incomplete_txns.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("Duplicate BeginTxn"));
    }

    #[test]
    fn test_validate_commit_without_begin() {
        let run_id = RunId::new();

        let entries = vec![WALEntry::CommitTxn { txn_id: 99, run_id }];

        let result = validate_transactions(&entries);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0]
            .message
            .contains("CommitTxn without BeginTxn"));
    }

    #[test]
    fn test_validate_abort_without_begin() {
        let run_id = RunId::new();

        let entries = vec![WALEntry::AbortTxn { txn_id: 99, run_id }];

        let result = validate_transactions(&entries);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0]
            .message
            .contains("AbortTxn without BeginTxn"));
    }

    #[test]
    fn test_validate_multiple_issues() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let entries = vec![
            // Orphaned write (no BeginTxn yet)
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "orphan1"),
                value: Value::Int(1),
                version: 1,
            },
            // Valid complete transaction
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "valid"),
                value: Value::Int(2),
                version: 2,
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
            // Incomplete transaction
            WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete"),
                value: Value::Int(3),
                version: 3,
            },
            // No CommitTxn for txn 2
            // Orphaned write (txn 2 ended implicitly with txn 3)
            WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            },
            // No CommitTxn for txn 3 either
        ];

        let result = validate_transactions(&entries);
        assert!(!result.is_clean());
        assert_eq!(result.orphaned_entries, 1); // The first write
        assert_eq!(result.incomplete_txns.len(), 2); // txn 2 and 3
    }

    // ========================================
    // Replay with Options Tests
    // ========================================

    #[test]
    fn test_replay_with_run_id_filter() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("run_filter.wal");

        let run_id_1 = RunId::new();
        let run_id_2 = RunId::new();

        let ns1 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id_1,
        );
        let ns2 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id_2,
        );

        // Write transactions for two different runs
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1 - run_id_1
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_id_1,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_1,
                key: Key::new_kv(ns1.clone(), "key1"),
                value: Value::Int(1),
                version: 1,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_id_1,
            })
            .unwrap();

            // Txn 2 - run_id_2
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_id_2,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_2,
                key: Key::new_kv(ns2.clone(), "key2"),
                value: Value::Int(2),
                version: 2,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 2,
                run_id: run_id_2,
            })
            .unwrap();

            // Txn 3 - run_id_1 again
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id: run_id_1,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_1,
                key: Key::new_kv(ns1.clone(), "key3"),
                value: Value::Int(3),
                version: 3,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 3,
                run_id: run_id_1,
            })
            .unwrap();
        }

        // Replay with filter for run_id_1 only
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();

        let options = ReplayOptions {
            filter_run_id: Some(run_id_1),
            ..Default::default()
        };

        let stats = replay_wal_with_options(&wal, &store, options).unwrap();

        // Should apply txn 1 and 3, filter out txn 2
        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.txns_filtered, 1);
        assert_eq!(stats.writes_applied, 2);

        // Verify only run_id_1 keys exist
        assert!(store
            .get(&Key::new_kv(ns1.clone(), "key1"))
            .unwrap()
            .is_some());
        assert!(store.get(&Key::new_kv(ns2, "key2")).unwrap().is_none()); // Filtered out
        assert!(store.get(&Key::new_kv(ns1, "key3")).unwrap().is_some());
    }

    #[test]
    fn test_replay_with_stop_at_version() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("stop_version.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write transactions with increasing versions
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1 - version 10
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Int(1),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2 - version 20
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::Int(2),
                version: 20,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();

            // Txn 3 - version 30
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::Int(3),
                version: 30,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
                .unwrap();
        }

        // Replay stopping at version 25 (should include txn 1 and 2, not 3)
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();

        let options = ReplayOptions {
            stop_at_version: Some(25),
            ..Default::default()
        };

        let stats = replay_wal_with_options(&wal, &store, options).unwrap();

        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.txns_filtered, 1);
        assert_eq!(stats.writes_applied, 2);

        // Verify only key1 and key2 exist
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key1"))
            .unwrap()
            .is_some());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key2"))
            .unwrap()
            .is_some());
        assert!(store.get(&Key::new_kv(ns, "key3")).unwrap().is_none()); // Stopped before
    }

    #[test]
    fn test_replay_with_progress_callback() {
        use std::sync::{Arc, Mutex};

        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("progress.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write 3 committed transactions
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            for i in 1..=3 {
                wal.append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("key{}", i)),
                    value: Value::Int(i as i64),
                    version: i,
                })
                .unwrap();
                wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                    .unwrap();
            }
        }

        // Replay with progress callback
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();

        let progress_log: Arc<Mutex<Vec<ReplayProgress>>> = Arc::new(Mutex::new(Vec::new()));
        let log_clone = progress_log.clone();

        let options = ReplayOptions {
            progress_callback: Some(Arc::new(move |progress| {
                log_clone.lock().unwrap().push(progress);
            })),
            ..Default::default()
        };

        let stats = replay_wal_with_options(&wal, &store, options).unwrap();

        assert_eq!(stats.txns_applied, 3);

        // Verify progress callback was called for each transaction
        let log = progress_log.lock().unwrap();
        assert_eq!(log.len(), 3);

        // First callback
        assert_eq!(log[0].current_txn_id, 1);
        assert_eq!(log[0].total_txns_found, 3);
        assert_eq!(log[0].txns_applied, 1);
        assert!(log[0].was_applied);

        // Second callback
        assert_eq!(log[1].current_txn_id, 2);
        assert_eq!(log[1].txns_applied, 2);
        assert!(log[1].was_applied);

        // Third callback
        assert_eq!(log[2].current_txn_id, 3);
        assert_eq!(log[2].txns_applied, 3);
        assert!(log[2].was_applied);
    }

    #[test]
    fn test_replay_combined_filters() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("combined.wal");

        let run_id_target = RunId::new();
        let run_id_other = RunId::new();

        let ns_target = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id_target,
        );
        let ns_other = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id_other,
        );

        // Write mix of transactions
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1 - target run, version 10 (should apply)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_id_target,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_target,
                key: Key::new_kv(ns_target.clone(), "key1"),
                value: Value::Int(1),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_id_target,
            })
            .unwrap();

            // Txn 2 - other run, version 15 (filtered by run_id)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_id_other,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_other,
                key: Key::new_kv(ns_other.clone(), "key2"),
                value: Value::Int(2),
                version: 15,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 2,
                run_id: run_id_other,
            })
            .unwrap();

            // Txn 3 - target run, version 20 (should apply)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id: run_id_target,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_target,
                key: Key::new_kv(ns_target.clone(), "key3"),
                value: Value::Int(3),
                version: 20,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 3,
                run_id: run_id_target,
            })
            .unwrap();

            // Txn 4 - target run, version 30 (filtered by stop_at_version)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 4,
                run_id: run_id_target,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_id_target,
                key: Key::new_kv(ns_target.clone(), "key4"),
                value: Value::Int(4),
                version: 30,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 4,
                run_id: run_id_target,
            })
            .unwrap();
        }

        // Replay with both filters
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();

        let options = ReplayOptions {
            filter_run_id: Some(run_id_target),
            stop_at_version: Some(25),
            ..Default::default()
        };

        let stats = replay_wal_with_options(&wal, &store, options).unwrap();

        // Txn 1 and 3 applied, txn 2 filtered by run_id, txn 4 filtered by version
        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.txns_filtered, 2);

        // Verify only key1 and key3 exist
        assert!(store
            .get(&Key::new_kv(ns_target.clone(), "key1"))
            .unwrap()
            .is_some());
        assert!(store.get(&Key::new_kv(ns_other, "key2")).unwrap().is_none());
        assert!(store
            .get(&Key::new_kv(ns_target.clone(), "key3"))
            .unwrap()
            .is_some());
        assert!(store
            .get(&Key::new_kv(ns_target, "key4"))
            .unwrap()
            .is_none());
    }

    // ========================================
    // Transaction Recovery Tests
    // ========================================

    #[test]
    fn test_interleaved_transaction_recovery() {
        // Per spec: WAL entries may be interleaved from concurrent transactions.
        // Recovery groups by txn_id correctly.
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("interleaved.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write interleaved sequence
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // BeginTxn for both transactions
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Interleaved writes
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "from_txn1"),
                value: Value::Int(1),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "from_txn2"),
                value: Value::Int(2),
                version: 200,
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "also_from_txn1"),
                value: Value::Int(11),
                version: 100,
            })
            .unwrap();

            // Commits
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // Both transactions should be applied
        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.writes_applied, 3);
        assert_eq!(stats.final_version, 200);

        // All keys should exist
        assert!(store
            .get(&Key::new_kv(ns.clone(), "from_txn1"))
            .unwrap()
            .is_some());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "from_txn2"))
            .unwrap()
            .is_some());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "also_from_txn1"))
            .unwrap()
            .is_some());
    }

    #[test]
    fn test_multiple_runs_independent_recovery() {
        // Transactions from different runs are independent.
        // Even if one run's transaction is incomplete, other run's should apply.
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("multi_run.wal");

        let run_a = RunId::new();
        let run_b = RunId::new();

        let ns_a = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_a,
        );
        let ns_b = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_b,
        );

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Run A: Committed transaction
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_a,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_a,
                key: Key::new_kv(ns_a.clone(), "key_a"),
                value: Value::String("from_run_a".to_string()),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_a,
            })
            .unwrap();

            // Run B: Incomplete transaction (simulates crash)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_b,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_b,
                key: Key::new_kv(ns_b.clone(), "key_b"),
                value: Value::String("from_run_b".to_string()),
                version: 20,
            })
            .unwrap();
            // NO CommitTxn - incomplete
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // Run A should be applied, Run B discarded
        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.incomplete_txns, 1);
        assert_eq!(stats.writes_applied, 1);

        // key_a should exist, key_b should NOT
        assert!(store.get(&Key::new_kv(ns_a, "key_a")).unwrap().is_some());
        assert!(store.get(&Key::new_kv(ns_b, "key_b")).unwrap().is_none());
    }

    #[test]
    fn test_delete_version_preserved_in_storage() {
        // Per spec: Delete operations preserve versions during recovery.
        // After delete, the storage should update its current_version to reflect the delete.
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("delete_version.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new_kv(ns.clone(), "to_be_deleted");

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Write at version 100
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("initial".to_string()),
                version: 100,
            })
            .unwrap();

            // Delete at version 150 (higher than write)
            wal.append(&WALEntry::Delete {
                run_id,
                key: key.clone(),
                version: 150,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.writes_applied, 1);
        assert_eq!(stats.deletes_applied, 1);
        // Final version should be max of all operations (150 from delete)
        assert_eq!(stats.final_version, 150);

        // Key should be deleted
        assert!(store.get(&key).unwrap().is_none());

        // Storage current_version should be 150
        assert_eq!(store.current_version(), 150);
    }

    #[test]
    fn test_interleaved_with_one_incomplete() {
        // Interleaved transactions from different runs where one is incomplete.
        // The complete transaction should apply, incomplete should be discarded.
        // NOTE: Each run_id can only have one active transaction at a time,
        // so we use different run_ids to test true interleaving.
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("interleaved_incomplete.wal");

        let run_complete = RunId::new();
        let run_incomplete = RunId::new();

        let ns_complete = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_complete,
        );
        let ns_incomplete = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_incomplete,
        );

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Start both transactions (from different runs)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_complete,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_incomplete,
                timestamp: now(),
            })
            .unwrap();

            // Interleaved writes
            wal.append(&WALEntry::Write {
                run_id: run_complete,
                key: Key::new_kv(ns_complete.clone(), "complete_txn_key"),
                value: Value::Int(100),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_incomplete,
                key: Key::new_kv(ns_incomplete.clone(), "incomplete_txn_key"),
                value: Value::Int(200),
                version: 200,
            })
            .unwrap();

            // Only txn 1 commits
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_complete,
            })
            .unwrap();
            // txn 2 never commits - incomplete
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.incomplete_txns, 1);
        // Only the write from txn 1 should be applied
        // The write from txn 2 should NOT be applied despite being in WAL
        assert_eq!(stats.writes_applied, 1);

        // complete_txn_key should exist (from txn 1)
        assert!(store
            .get(&Key::new_kv(ns_complete, "complete_txn_key"))
            .unwrap()
            .is_some());
        // incomplete_txn_key should NOT exist (from incomplete txn 2)
        assert!(store
            .get(&Key::new_kv(ns_incomplete, "incomplete_txn_key"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_multiple_writes_same_key_different_transactions() {
        // Multiple transactions write to the same key.
        // Final state should reflect all writes in order.
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("multi_write_same_key.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new_kv(ns.clone(), "contested_key");

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1: Write initial value
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("first".to_string()),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2: Overwrite with new value
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("second".to_string()),
                version: 20,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();

            // Txn 3: Final value
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("final".to_string()),
                version: 30,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 3);
        assert_eq!(stats.writes_applied, 3);
        assert_eq!(stats.final_version, 30);

        // Final value should be "final" with version 30
        let stored = store.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::String("final".to_string()));
        assert_eq!(stored.version.as_u64(), 30);
    }

    // ========================================================================
    // JSON Crash Recovery Tests
    // ========================================================================

    use strata_core::primitives::json::{JsonPath, JsonValue};

    /// Serialize a JsonValue to msgpack bytes (for WAL entry construction)
    fn json_to_msgpack(value: &JsonValue) -> Vec<u8> {
        rmp_serde::to_vec(value).unwrap()
    }

    #[test]
    fn test_json_create_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_create.wal");

        let run_id = RunId::new();
        let doc_id = "test-doc";
        let initial_value: JsonValue = serde_json::json!({
            "name": "Alice",
            "age": 30
        })
        .into();

        // Write WAL entries
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&initial_value),
                version: 1,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay to storage (simulating recovery)
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.json_creates_applied, 1);
        assert_eq!(stats.final_version, 1);

        // Verify document exists in storage
        let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let stored = store.get(&key).unwrap().expect("Document should exist");

        // Deserialize and verify
        let doc = match &stored.value {
            Value::Bytes(bytes) => RecoveryJsonDoc::from_bytes(bytes).unwrap(),
            _ => panic!("Expected bytes"),
        };
        assert_eq!(doc.id, doc_id);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.value.as_object().unwrap().get("name").unwrap(), "Alice");
    }

    #[test]
    fn test_json_set_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_set.wal");

        let run_id = RunId::new();
        let doc_id = "test-doc";
        let initial_value: JsonValue = serde_json::json!({ "count": 0 }).into();
        let new_value: JsonValue = serde_json::json!(42).into();

        // Write WAL entries: create, then set
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Transaction 1: Create
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&initial_value),
                version: 1,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Transaction 2: Set
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonSet {
                run_id,
                doc_id: doc_id.to_string(),
                path: "count".parse::<JsonPath>().unwrap(),
                value_bytes: json_to_msgpack(&new_value),
                version: 2,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.json_creates_applied, 1);
        assert_eq!(stats.json_sets_applied, 1);
        assert_eq!(stats.final_version, 2);

        // Verify document has updated value
        let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let stored = store.get(&key).unwrap().expect("Document should exist");
        let doc = match &stored.value {
            Value::Bytes(bytes) => RecoveryJsonDoc::from_bytes(bytes).unwrap(),
            _ => panic!("Expected bytes"),
        };
        assert_eq!(doc.version, 2);
        assert_eq!(doc.value.as_object().unwrap().get("count").unwrap(), 42);
    }

    #[test]
    fn test_json_delete_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_delete.wal");

        let run_id = RunId::new();
        let doc_id = "test-doc";
        let initial_value: JsonValue = serde_json::json!({
            "name": "Bob",
            "temp": "to_be_deleted"
        })
        .into();

        // Write WAL entries: create, then delete field
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Create
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&initial_value),
                version: 1,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Delete field
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonDelete {
                run_id,
                doc_id: doc_id.to_string(),
                path: "temp".parse::<JsonPath>().unwrap(),
                version: 2,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.json_creates_applied, 1);
        assert_eq!(stats.json_deletes_applied, 1);
        assert_eq!(stats.final_version, 2);

        // Verify temp field is deleted but name remains
        let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let stored = store.get(&key).unwrap().expect("Document should exist");
        let doc = match &stored.value {
            Value::Bytes(bytes) => RecoveryJsonDoc::from_bytes(bytes).unwrap(),
            _ => panic!("Expected bytes"),
        };
        assert_eq!(doc.version, 2);
        assert_eq!(doc.value.as_object().unwrap().get("name").unwrap(), "Bob");
        assert!(doc.value.as_object().unwrap().get("temp").is_none());
    }

    #[test]
    fn test_json_destroy_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_destroy.wal");

        let run_id = RunId::new();
        let doc_id = "test-doc";
        let initial_value: JsonValue = serde_json::json!({ "data": "test" }).into();

        // Write WAL entries: create, then destroy
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Create
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&initial_value),
                version: 1,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Destroy
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonDestroy { run_id, doc_id: doc_id.to_string() })
                .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 2);
        assert_eq!(stats.json_creates_applied, 1);
        assert_eq!(stats.json_destroys_applied, 1);

        // Document should not exist
        let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_json_incomplete_transaction_discarded() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_incomplete.wal");

        let run_id = RunId::new();
        let doc_id = "test-doc";
        let initial_value: JsonValue = serde_json::json!({ "status": "initial" }).into();

        // Simulate crash: begin transaction but no commit
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&initial_value),
                version: 1,
                timestamp: now(),
            })
            .unwrap();

            // NO CommitTxn - simulating crash
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // Transaction should be discarded (incomplete)
        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.json_creates_applied, 0);
        assert_eq!(stats.incomplete_txns, 1);

        // Document should NOT exist
        let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_json_idempotent_replay() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_idempotent.wal");

        let run_id = RunId::new();
        let doc_id = "test-doc";
        let value: JsonValue = serde_json::json!({ "count": 1 }).into();

        // Write WAL entries
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&value),
                version: 1,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // First replay
        let store = ShardedStore::new();
        {
            let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let stats = replay_wal(&wal, &store).unwrap();
            assert_eq!(stats.json_creates_applied, 1);
        }

        // Second replay (idempotent - should just overwrite)
        {
            let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let stats = replay_wal(&wal, &store).unwrap();
            assert_eq!(stats.json_creates_applied, 1);
        }

        // Document should still have correct value
        let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let stored = store.get(&key).unwrap().expect("Document should exist");
        let doc = match &stored.value {
            Value::Bytes(bytes) => RecoveryJsonDoc::from_bytes(bytes).unwrap(),
            _ => panic!("Expected bytes"),
        };
        assert_eq!(doc.value.as_object().unwrap().get("count").unwrap(), 1);
    }

    // ========================================================================
    // Adversarial Recovery Tests
    // ========================================================================

    #[test]
    fn test_replay_idempotent_kv_operations() {
        // Replaying the same WAL twice to the same storage should produce identical state
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("idempotent.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "k1"),
                value: Value::Int(42),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        let store = ShardedStore::new();

        // First replay
        {
            let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let stats = replay_wal(&wal, &store).unwrap();
            assert_eq!(stats.txns_applied, 1);
            assert_eq!(stats.writes_applied, 1);
        }

        let version_after_first = store.current_version();
        let val_after_first = store.get(&Key::new_kv(ns.clone(), "k1")).unwrap().unwrap();

        // Second replay (idempotent - overwrites same keys with same values)
        {
            let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let stats = replay_wal(&wal, &store).unwrap();
            assert_eq!(stats.txns_applied, 1);
        }

        let version_after_second = store.current_version();
        let val_after_second = store.get(&Key::new_kv(ns, "k1")).unwrap().unwrap();

        // State should be identical
        assert_eq!(version_after_first, version_after_second);
        assert_eq!(val_after_first.value, val_after_second.value);
        assert_eq!(val_after_first.version, val_after_second.version);
    }

    #[test]
    fn test_replay_orphaned_entries_counted_but_not_applied() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("orphaned.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write orphaned entries (writes without BeginTxn)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Orphaned write (no transaction started)
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "orphan"),
                value: Value::Int(999),
                version: 1,
            })
            .unwrap();
        }

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.orphaned_entries, 1);

        // Orphaned entry should NOT be in storage
        assert!(store.get(&Key::new_kv(ns, "orphan")).unwrap().is_none());
    }

    #[test]
    fn test_replay_max_txn_id_tracked_across_committed_and_incomplete() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("max_txn.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Committed txn with id 5
            wal.append(&WALEntry::BeginTxn {
                txn_id: 5,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "k1"),
                value: Value::Int(1),
                version: 1,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 5, run_id })
                .unwrap();

            // Incomplete txn with HIGHER id (100)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 100,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "k2"),
                value: Value::Int(2),
                version: 2,
            })
            .unwrap();
            // NO CommitTxn - incomplete
        }

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // max_txn_id should include the incomplete transaction's ID
        // This is critical so the next TransactionManager doesn't reuse ID 100
        assert_eq!(stats.max_txn_id, 100);
        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.incomplete_txns, 1);
    }

    #[test]
    fn test_replay_aborted_and_incomplete_not_conflated() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("aborted_vs_incomplete.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Committed txn
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "k1"),
                value: Value::Int(1),
                version: 1,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Aborted txn
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "k2"),
                value: Value::Int(2),
                version: 2,
            })
            .unwrap();
            wal.append(&WALEntry::AbortTxn { txn_id: 2, run_id })
                .unwrap();

            // Incomplete txn
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "k3"),
                value: Value::Int(3),
                version: 3,
            })
            .unwrap();
            // NO CommitTxn or AbortTxn
        }

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.aborted_txns, 1);
        assert_eq!(stats.incomplete_txns, 1);
        assert_eq!(stats.writes_applied, 1);

        // Only committed key should exist
        assert!(store.get(&Key::new_kv(ns.clone(), "k1")).unwrap().is_some());
        assert!(store.get(&Key::new_kv(ns.clone(), "k2")).unwrap().is_none());
        assert!(store.get(&Key::new_kv(ns.clone(), "k3")).unwrap().is_none());
    }

    #[test]
    fn test_validate_checkpoint_always_valid() {
        let entries = vec![WALEntry::Checkpoint {
            snapshot_id: uuid::Uuid::new_v4(),
            version: 100,
            active_runs: vec![RunId::new()],
        }];

        let result = validate_transactions(&entries);
        assert!(result.is_clean());
    }

    #[test]
    fn test_validate_aborted_transaction_not_incomplete() {
        let run_id = RunId::new();

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::AbortTxn { txn_id: 1, run_id },
        ];

        let result = validate_transactions(&entries);
        // Aborted is not incomplete
        assert!(result.incomplete_txns.is_empty());
        assert!(result.is_clean());
    }

    #[test]
    fn test_validation_result_log_warnings_does_not_panic() {
        // Ensure log_warnings doesn't panic with various combinations
        let mut result = ValidationResult::default();
        result.log_warnings(); // Empty - should be no-op

        result.incomplete_txns = vec![1, 2, 3];
        result.orphaned_entries = 5;
        result.warnings.push(ValidationWarning {
            entry_index: 0,
            message: "test warning".to_string(),
        });
        result.log_warnings(); // Should not panic
    }

    #[test]
    fn test_replay_stats_default() {
        let stats = ReplayStats::default();
        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.deletes_applied, 0);
        assert_eq!(stats.final_version, 0);
        assert_eq!(stats.max_txn_id, 0);
        assert_eq!(stats.incomplete_txns, 0);
        assert_eq!(stats.aborted_txns, 0);
        assert_eq!(stats.orphaned_entries, 0);
        assert_eq!(stats.txns_filtered, 0);
        assert_eq!(stats.corrupted_entries, 0);
        assert_eq!(stats.json_creates_applied, 0);
        assert_eq!(stats.json_sets_applied, 0);
        assert_eq!(stats.json_deletes_applied, 0);
        assert_eq!(stats.json_destroys_applied, 0);
    }

    #[test]
    fn test_replay_options_debug_with_callback() {
        use std::sync::Arc;

        let options = ReplayOptions {
            filter_run_id: Some(RunId::new()),
            stop_at_version: Some(100),
            progress_callback: Some(Arc::new(|_| {})),
        };

        // Debug should work and show <callback> instead of function pointer
        let debug = format!("{:?}", options);
        assert!(debug.contains("<callback>"));
    }

    #[test]
    fn test_replay_options_debug_without_callback() {
        let options = ReplayOptions {
            filter_run_id: None,
            stop_at_version: None,
            progress_callback: None,
        };

        let debug = format!("{:?}", options);
        assert!(debug.contains("None"));
    }

    #[test]
    fn test_recovery_json_doc_roundtrip() {
        let doc = RecoveryJsonDoc::new(
            "test-doc",
            serde_json::json!({"nested": {"key": [1, 2, 3]}}).into(),
            5,
            1706000000,
        );

        let bytes = doc.to_bytes().unwrap();
        let recovered = RecoveryJsonDoc::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.id, "test-doc");
        assert_eq!(recovered.version, 5);
        assert_eq!(recovered.created_at, 1706000000);
        // Verify nested structure survived serialization
        let inner = recovered.value.as_inner();
        assert_eq!(inner["nested"]["key"], serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn test_recovery_json_doc_from_corrupt_bytes() {
        let corrupt = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let result = RecoveryJsonDoc::from_bytes(&corrupt);
        assert!(result.is_err());
    }

    #[test]
    fn test_json_mixed_with_kv_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("json_mixed.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let doc_id = "test-doc";
        let json_value: JsonValue = serde_json::json!({ "type": "json" }).into();

        // Write mixed KV and JSON operations
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // KV write
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "kv_key"),
                value: Value::String("kv_value".to_string()),
                version: 1,
            })
            .unwrap();

            // JSON create
            wal.append(&WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: json_to_msgpack(&json_value),
                version: 2,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = ShardedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.writes_applied, 1);
        assert_eq!(stats.json_creates_applied, 1);
        assert_eq!(stats.final_version, 2);

        // Verify both exist
        let kv_key = Key::new_kv(ns.clone(), "kv_key");
        assert!(store.get(&kv_key).unwrap().is_some());

        let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        assert!(store.get(&json_key).unwrap().is_some());
    }
}
