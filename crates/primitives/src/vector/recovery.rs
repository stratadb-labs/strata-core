//! Vector Recovery Participant
//!
//! Registers VectorStore as a recovery participant so that vector state
//! (in-memory backends with embeddings) is restored when the Database reopens.
//!
//! ## Usage
//!
//! Call `register_vector_recovery()` once during application startup,
//! before opening any Database:
//!
//! ```ignore
//! use strata_primitives::vector::register_vector_recovery;
//!
//! // Call once at startup
//! register_vector_recovery();
//!
//! // Now Database::open() will automatically recover vector state
//! let db = Database::open("/path/to/data")?;
//! ```
//!
//! ## How It Works
//!
//! 1. `register_vector_recovery()` registers a recovery function with the engine
//! 2. When `Database::open()` runs, it calls all registered recovery participants
//! 3. The vector recovery function creates a VectorStore and calls `recover()`
//! 4. `recover()` replays WAL entries into `VectorBackendState` (stored in Database extensions)
//! 5. The Database is ready with all vector embeddings restored

use strata_core::error::{Error, Result};
use strata_engine::{register_recovery_participant, Database, RecoveryParticipant};
use tracing::info;

/// Recovery function for VectorStore
///
/// Called by Database during startup to restore vector state from WAL.
fn recover_vector_state(db: &Database) -> Result<()> {
    recover_from_db(db)
}

/// Internal recovery implementation that works with &Database
fn recover_from_db(db: &Database) -> Result<()> {
    use super::{
        CollectionId, DistanceMetric, IndexBackendFactory, VectorBackendState, VectorConfig,
        VectorId,
    };
    use strata_durability::wal::WALEntry;
    use std::collections::{HashMap, HashSet};

    // Skip if InMemory mode (no WAL)
    if !db.durability_mode().requires_wal() {
        return Ok(());
    }

    // Get access to the shared backend state
    let state = db.extension::<VectorBackendState>();
    let factory = IndexBackendFactory::default();

    // Read all WAL entries
    let wal = db.wal();
    let wal_guard = wal.lock().unwrap();
    let entries = wal_guard
        .read_all()
        .map_err(|e| Error::StorageError(format!("WAL read failed: {}", e)))?;
    drop(wal_guard);

    // Track transactions
    struct TxnState {
        entries: Vec<WALEntry>,
        committed: bool,
    }
    let mut transactions: HashMap<u64, TxnState> = HashMap::new();
    let mut active_txn: HashMap<strata_core::types::RunId, u64> = HashMap::new();
    let mut entries_in_txn: HashSet<usize> = HashSet::new();

    // First pass: group transactional entries
    for (idx, entry) in entries.iter().enumerate() {
        match entry {
            WALEntry::BeginTxn { txn_id, run_id, .. } => {
                transactions.insert(
                    *txn_id,
                    TxnState {
                        entries: Vec::new(),
                        committed: false,
                    },
                );
                active_txn.insert(*run_id, *txn_id);
                entries_in_txn.insert(idx);
            }
            WALEntry::CommitTxn { txn_id, .. } => {
                if let Some(txn) = transactions.get_mut(txn_id) {
                    txn.committed = true;
                }
                entries_in_txn.insert(idx);
            }
            WALEntry::AbortTxn { txn_id, run_id } => {
                transactions.remove(txn_id);
                if active_txn.get(run_id) == Some(txn_id) {
                    active_txn.remove(run_id);
                }
                entries_in_txn.insert(idx);
            }
            WALEntry::VectorCollectionCreate { run_id, .. }
            | WALEntry::VectorCollectionDelete { run_id, .. }
            | WALEntry::VectorUpsert { run_id, .. }
            | WALEntry::VectorDelete { run_id, .. } => {
                if let Some(&txn_id) = active_txn.get(run_id) {
                    if let Some(txn) = transactions.get_mut(&txn_id) {
                        txn.entries.push(entry.clone());
                        entries_in_txn.insert(idx);
                    }
                }
            }
            _ => {}
        }
    }

    let mut stats = super::RecoveryStats::default();

    // Helper to replay a single entry
    let replay_entry = |entry: &WALEntry, stats: &mut super::RecoveryStats| -> Result<()> {
        match entry {
            WALEntry::VectorCollectionCreate {
                run_id,
                collection,
                dimension,
                metric,
                ..
            } => {
                let config = VectorConfig {
                    dimension: *dimension,
                    metric: DistanceMetric::from_byte(*metric).ok_or_else(|| {
                        Error::StorageError(format!("Invalid metric: {}", metric))
                    })?,
                    storage_dtype: super::StorageDtype::F32,
                };
                let collection_id = CollectionId::new(*run_id, collection);
                let backend = factory.create(&config);
                state.backends.write().unwrap().insert(collection_id, backend);
                stats.collections_created += 1;
            }
            WALEntry::VectorCollectionDelete {
                run_id,
                collection,
                ..
            } => {
                let collection_id = CollectionId::new(*run_id, collection);
                state.backends.write().unwrap().remove(&collection_id);
                stats.collections_deleted += 1;
            }
            WALEntry::VectorUpsert {
                run_id,
                collection,
                vector_id,
                embedding,
                ..
            } => {
                let collection_id = CollectionId::new(*run_id, collection);
                let vid = VectorId::new(*vector_id);
                let mut backends = state.backends.write().unwrap();
                if let Some(backend) = backends.get_mut(&collection_id) {
                    // Use insert_with_id to maintain VectorId monotonicity
                    let _ = backend.insert_with_id(vid, embedding);
                    stats.vectors_upserted += 1;
                }
            }
            WALEntry::VectorDelete {
                run_id,
                collection,
                vector_id,
                ..
            } => {
                let collection_id = CollectionId::new(*run_id, collection);
                let vid = VectorId::new(*vector_id);
                let mut backends = state.backends.write().unwrap();
                if let Some(backend) = backends.get_mut(&collection_id) {
                    let _ = backend.delete(vid);
                    stats.vectors_deleted += 1;
                }
            }
            _ => {}
        }
        Ok(())
    };

    // Second pass: replay committed transactional entries
    let mut committed_txns: Vec<_> = transactions
        .into_iter()
        .filter(|(_, txn)| txn.committed)
        .collect();
    committed_txns.sort_by_key(|(txn_id, _)| *txn_id);

    for (_txn_id, txn) in committed_txns {
        for entry in txn.entries {
            replay_entry(&entry, &mut stats)?;
        }
    }

    // Third pass: replay standalone entries (not in any transaction)
    for (idx, entry) in entries.iter().enumerate() {
        if entries_in_txn.contains(&idx) {
            continue;
        }

        match entry {
            WALEntry::VectorCollectionCreate { .. }
            | WALEntry::VectorCollectionDelete { .. }
            | WALEntry::VectorUpsert { .. }
            | WALEntry::VectorDelete { .. } => {
                replay_entry(entry, &mut stats)?;
            }
            _ => {}
        }
    }

    info!(
        collections_created = stats.collections_created,
        collections_deleted = stats.collections_deleted,
        vectors_upserted = stats.vectors_upserted,
        vectors_deleted = stats.vectors_deleted,
        "Vector recovery complete"
    );

    Ok(())
}

/// Register VectorStore as a recovery participant
///
/// Call this once during application startup, before opening any Database.
/// This ensures that vector state (in-memory backends with embeddings) is
/// automatically restored when a Database is reopened.
///
/// # Example
///
/// ```ignore
/// use strata_primitives::vector::register_vector_recovery;
///
/// fn main() {
///     // Register recovery participant before any Database operations
///     register_vector_recovery();
///
///     // Now Database::open() will recover vector state
///     let db = Database::open("/path/to/data").unwrap();
/// }
/// ```
pub fn register_vector_recovery() {
    register_recovery_participant(RecoveryParticipant::new("vector", recover_vector_state));
}
