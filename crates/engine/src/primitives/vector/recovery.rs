//! Vector Recovery Participant
//!
//! Registers VectorStore as a recovery participant so that vector state
//! (in-memory backends with embeddings) is restored when the Database reopens.
//!
//! ## How It Works
//!
//! 1. `register_vector_recovery()` registers a recovery function with the engine
//! 2. When `Database::open()` runs, it calls all registered recovery participants
//! 3. The vector recovery function scans KV store for vector config and data entries
//! 4. For each collection config found, it creates a backend and loads embeddings
//! 5. The Database is ready with all vector embeddings restored
//!
//! ## KV-Based Recovery (Phase 3)
//!
//! Vector data is already persisted through KV transactions. Recovery rebuilds
//! in-memory indices by scanning the KV store after KV recovery completes.
//! This eliminates the need for separate WALEntry::Vector* variants.

use strata_core::StrataResult;
use crate::recovery::{register_recovery_participant, RecoveryParticipant};
use crate::database::Database;
use tracing::info;

/// Recovery function for VectorStore
///
/// Called by Database during startup to restore vector state from KV store.
fn recover_vector_state(db: &Database) -> StrataResult<()> {
    recover_from_db(db)
}

/// Internal recovery implementation that works with &Database
fn recover_from_db(db: &Database) -> StrataResult<()> {
    use super::{
        CollectionId, IndexBackendFactory, VectorBackendState, VectorId, VectorConfig,
    };
    use strata_core::traits::SnapshotView;
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;

    // Skip recovery for ephemeral databases
    if db.is_ephemeral() {
        return Ok(());
    }

    // Get access to the shared backend state
    let state = db.extension::<VectorBackendState>();
    let factory = IndexBackendFactory::default();

    let snapshot = db.storage().create_snapshot();
    let mut stats = super::RecoveryStats::default();

    // Iterate all branch_ids in storage
    for branch_id in db.storage().branch_ids() {
        let ns = Namespace::for_branch(branch_id);

        // Scan for vector config entries in this run
        let config_prefix = Key::new_vector_config_prefix(ns.clone());
        let config_entries = match snapshot.scan_prefix(&config_prefix) {
            Ok(entries) => entries,
            Err(e) => {
                tracing::warn!(
                    branch_id = ?branch_id,
                    error = %e,
                    "Failed to scan vector configs during recovery"
                );
                continue;
            }
        };

        for (key, versioned) in &config_entries {
            // Parse the collection config from the KV value
            let config_bytes = match &versioned.value {
                Value::Bytes(b) => b,
                _ => continue,
            };

            // Decode the CollectionRecord
            let record = match super::CollectionRecord::from_bytes(config_bytes) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        key = ?key,
                        error = %e,
                        "Failed to decode collection record during recovery, skipping"
                    );
                    continue;
                }
            };

            // Extract collection name from the key's user_key
            let collection_name = match key.user_key_string() {
                Some(name) => name,
                None => continue,
            };

            let config: VectorConfig = match record.config.try_into() {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        collection = %collection_name,
                        error = %e,
                        "Failed to convert collection config during recovery, skipping"
                    );
                    continue;
                }
            };
            let collection_id = CollectionId::new(branch_id, &collection_name);

            // Create backend for this collection
            let backend = factory.create(&config);
            state.backends.write().insert(collection_id.clone(), backend);
            stats.collections_created += 1;

            // Scan for all vector entries in this collection
            let vector_prefix = Key::new_vector(ns.clone(), &collection_name, "");
            let vector_entries = match snapshot.scan_prefix(&vector_prefix) {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!(
                        collection = %collection_name,
                        error = %e,
                        "Failed to scan vectors during recovery"
                    );
                    continue;
                }
            };

            for (_vec_key, vec_versioned) in &vector_entries {
                let vec_bytes = match &vec_versioned.value {
                    Value::Bytes(b) => b,
                    _ => continue,
                };

                // Decode the VectorRecord
                let vec_record = match super::VectorRecord::from_bytes(vec_bytes) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to decode vector record during recovery, skipping"
                        );
                        continue;
                    }
                };

                // Insert into the backend
                let vid = VectorId::new(vec_record.vector_id);
                let mut backends = state.backends.write();
                if let Some(backend) = backends.get_mut(&collection_id) {
                    let _ = backend.insert_with_id(vid, &vec_record.embedding);
                    stats.vectors_upserted += 1;
                }
            }
        }
    }

    if stats.collections_created > 0 || stats.vectors_upserted > 0 {
        info!(
            collections_created = stats.collections_created,
            vectors_upserted = stats.vectors_upserted,
            "Vector recovery complete (KV-based)"
        );
    }

    Ok(())
}

/// Register VectorStore as a recovery participant
///
/// Call this once during application startup, before opening any Database.
/// This ensures that vector state (in-memory backends with embeddings) is
/// automatically restored when a Database is reopened.
pub fn register_vector_recovery() {
    register_recovery_participant(RecoveryParticipant::new("vector", recover_vector_state));
}
