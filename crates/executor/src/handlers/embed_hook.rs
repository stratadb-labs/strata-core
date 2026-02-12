//! Auto-embed hook called from write handlers.
//!
//! When auto-embedding is enabled and the `embed` feature is compiled in,
//! this module generates embeddings for text values and stores them in
//! shadow vector collections.
//!
//! Embeddings are buffered in an [`EmbedBuffer`] and flushed as a batch when
//! the buffer reaches `batch_size` items, or when [`flush_embed_buffer()`] is
//! called explicitly (e.g. on `db.flush()`).

use std::sync::Arc;

use crate::bridge::Primitives;

// Re-export shadow collection names from engine (single source of truth).
pub use strata_engine::database::{SHADOW_EVENT, SHADOW_JSON, SHADOW_KV, SHADOW_STATE};

/// Separator for composite shadow keys (ASCII Unit Separator).
/// Avoids ambiguity since "/" is allowed in both space and key names.
#[cfg(feature = "embed")]
const SHADOW_KEY_SEP: char = '\x1f';

/// In-memory state for auto-embedding shadow collection tracking.
///
/// Stored as a Database extension to share the created-collections cache
/// across all handles. Uses `Mutex<HashSet>` to avoid adding a `dashmap`
/// dependency to the executor crate.
#[cfg(feature = "embed")]
pub struct AutoEmbedState {
    /// Tracks which shadow collections have been created (keyed by "branch_id/collection_name").
    /// Prevents repeated `create_system_collection` calls on every write.
    shadow_collections_created: std::sync::Mutex<std::collections::HashSet<String>>,
}

#[cfg(feature = "embed")]
impl Default for AutoEmbedState {
    fn default() -> Self {
        Self {
            shadow_collections_created: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }
}

#[cfg(feature = "embed")]
impl AutoEmbedState {
    fn contains(&self, key: &str) -> bool {
        self.shadow_collections_created
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(key)
    }

    fn insert(&self, key: String) {
        self.shadow_collections_created
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key);
    }
}

// ---------------------------------------------------------------------------
// Write-behind embed buffer
// ---------------------------------------------------------------------------

/// A pending embedding that has been buffered but not yet computed.
#[cfg(feature = "embed")]
struct PendingEmbed {
    branch_id: strata_core::types::BranchId,
    space: String,
    shadow_collection: &'static str,
    key: String,
    text: String,
    source_ref: strata_core::EntityRef,
}

/// Write-behind buffer for embedding requests.
///
/// Stored as a `Database` extension (`db.extension::<EmbedBuffer>()`).
/// Pending items accumulate until `batch_size` is reached (auto-flush) or
/// [`flush_embed_buffer()`] is called (manual flush on `db.flush()`).
#[cfg(feature = "embed")]
pub struct EmbedBuffer {
    pending: std::sync::Mutex<Vec<PendingEmbed>>,
    batch_size: usize,
}

#[cfg(feature = "embed")]
impl Default for EmbedBuffer {
    fn default() -> Self {
        Self {
            pending: std::sync::Mutex::new(Vec::with_capacity(64)),
            batch_size: 64,
        }
    }
}

/// Buffer a text for embedding in a shadow vector collection.
///
/// Best-effort: failures are logged, never propagated to the caller.
/// When the buffer reaches `batch_size`, the calling thread auto-flushes.
#[cfg(feature = "embed")]
pub fn maybe_embed_text(
    p: &Arc<Primitives>,
    branch_id: strata_core::types::BranchId,
    space: &str,
    shadow_collection: &'static str,
    key: &str,
    text: &str,
    source_ref: strata_core::EntityRef,
) {
    if !p.db.auto_embed_enabled() {
        return;
    }

    let buf = match p.db.extension::<EmbedBuffer>() {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(target: "strata::embed", error = %e, "Failed to get embed buffer");
            return;
        }
    };

    let should_flush = {
        let mut pending = buf.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.push(PendingEmbed {
            branch_id,
            space: space.to_owned(),
            shadow_collection,
            key: key.to_owned(),
            text: text.to_owned(),
            source_ref,
        });
        pending.len() >= buf.batch_size
    };

    if should_flush {
        flush_embed_buffer(p);
    }
}

/// No-op when the embed feature is not compiled in.
#[cfg(not(feature = "embed"))]
pub fn maybe_embed_text(
    _p: &Arc<Primitives>,
    _branch_id: strata_core::types::BranchId,
    _space: &str,
    _shadow_collection: &str,
    _key: &str,
    _text: &str,
    _source_ref: strata_core::EntityRef,
) {
}

/// Flush all pending embeddings: compute vectors in batch and insert.
///
/// Safe to call concurrently — drain is atomic (`mem::take` under Mutex),
/// so a second caller simply processes an empty/partial buffer.
#[cfg(feature = "embed")]
pub fn flush_embed_buffer(p: &Arc<Primitives>) {
    use strata_intelligence::embed::EmbedModelState;

    let buf = match p.db.extension::<EmbedBuffer>() {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(target: "strata::embed", error = %e, "Failed to get embed buffer for flush");
            return;
        }
    };

    // Atomically drain the buffer.
    let batch = {
        let mut pending = buf.pending.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *pending)
    };

    if batch.is_empty() {
        return;
    }

    // Load model once for the whole batch.
    let model_dir = p.db.model_dir();
    let embed_state = match p.db.extension::<EmbedModelState>() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(target: "strata::embed", error = %e, "Failed to get embed model state");
            return;
        }
    };

    let model = match embed_state.get_or_load(&model_dir) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(target: "strata::embed", error = %e, "Failed to load embedding model");
            return;
        }
    };

    // Compute all embeddings in one Rust call (back-to-back forward passes).
    let texts: Vec<&str> = batch.iter().map(|pe| pe.text.as_str()).collect();
    let embeddings = model.embed_batch(&texts);
    let count = batch.len();

    // Insert each embedding into its shadow collection.
    for (pe, embedding) in batch.into_iter().zip(embeddings.iter()) {
        ensure_shadow_collection(p, pe.branch_id, pe.shadow_collection);

        let composite_key = format!("{}{}{}", pe.space, SHADOW_KEY_SEP, pe.key);
        let metadata = serde_json::json!({
            "source_space": pe.space,
            "source_key": pe.key,
        });

        if let Err(e) = p.vector.system_insert_with_source(
            pe.branch_id,
            pe.shadow_collection,
            &composite_key,
            &embedding,
            Some(metadata),
            pe.source_ref,
        ) {
            tracing::warn!(
                target: "strata::embed",
                collection = pe.shadow_collection,
                key = composite_key,
                error = %e,
                "Failed to insert embedding"
            );
        }
    }

    tracing::debug!(
        target: "strata::embed",
        count,
        "Flushed embed buffer"
    );
}

/// No-op when the embed feature is not compiled in.
#[cfg(not(feature = "embed"))]
pub fn flush_embed_buffer(_p: &Arc<Primitives>) {}

/// Remove a shadow embedding entry on delete.
///
/// Also drains any matching pending embed from the buffer to prevent a
/// ghost embedding being inserted after the delete (write-behind race).
///
/// Best-effort: failures are logged, never propagated to the caller.
#[cfg(feature = "embed")]
pub fn maybe_remove_embedding(
    p: &Arc<Primitives>,
    branch_id: strata_core::types::BranchId,
    space: &str,
    shadow_collection: &str,
    key: &str,
) {
    if !p.db.auto_embed_enabled() {
        return;
    }

    // Drain any buffered-but-not-yet-flushed embed for this key to prevent a
    // ghost embedding from being inserted after the delete.
    if let Ok(buf) = p.db.extension::<EmbedBuffer>() {
        let mut pending = buf.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.retain(|pe| {
            !(pe.branch_id == branch_id
                && pe.shadow_collection == shadow_collection
                && pe.space == space
                && pe.key == key)
        });
    }

    let composite_key = format!("{}{}{}", space, SHADOW_KEY_SEP, key);

    if let Err(e) = p
        .vector
        .system_delete(branch_id, shadow_collection, &composite_key)
    {
        // Collection or vector may not exist yet (no embeds were ever created), that's fine.
        if !e.is_not_found() {
            tracing::warn!(
                target: "strata::embed",
                collection = shadow_collection,
                key = composite_key,
                error = %e,
                "Failed to remove shadow embedding"
            );
        }
    }
}

/// No-op when the embed feature is not compiled in.
#[cfg(not(feature = "embed"))]
pub fn maybe_remove_embedding(
    _p: &Arc<Primitives>,
    _branch_id: strata_core::types::BranchId,
    _space: &str,
    _shadow_collection: &str,
    _key: &str,
) {
}

/// Extract embeddable text from a Value.
#[cfg(feature = "embed")]
pub fn extract_text(value: &strata_core::Value) -> Option<String> {
    strata_intelligence::embed::extract::extract_text(value)
}

/// No-op when the embed feature is not compiled in.
#[cfg(not(feature = "embed"))]
pub fn extract_text(_value: &strata_core::Value) -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "embed"))]
mod tests {
    use super::*;
    use strata_core::types::BranchId;

    /// Helper: create a Primitives with auto_embed enabled.
    fn setup() -> Arc<Primitives> {
        let db = strata_engine::Database::cache().expect("open cache db");
        db.set_auto_embed(true);
        let p = Arc::new(Primitives::new(db));
        // Ensure the default branch exists.
        let _ = p.branch.create_branch(&BranchId::default().to_string());
        p
    }

    /// Helper: read current buffer length.
    fn buffer_len(p: &Arc<Primitives>) -> usize {
        let buf = p.db.extension::<EmbedBuffer>().unwrap();
        let pending = buf.pending.lock().unwrap();
        pending.len()
    }

    /// Helper: push N items to the embed buffer directly.
    fn push_n(p: &Arc<Primitives>, n: usize) {
        let branch_id = BranchId::default();
        for i in 0..n {
            maybe_embed_text(
                p,
                branch_id,
                "default",
                SHADOW_KV,
                &format!("key-{}", i),
                &format!("text for key {}", i),
                strata_core::EntityRef::kv(branch_id, &format!("key-{}", i)),
            );
        }
    }

    #[test]
    fn test_buffer_accumulates_items() {
        let p = setup();
        assert_eq!(buffer_len(&p), 0);

        push_n(&p, 5);
        assert_eq!(buffer_len(&p), 5);
    }

    #[test]
    fn test_auto_flush_at_batch_size() {
        let p = setup();

        // Default batch_size is 64 — we test against that.

        // Push 63 items — should NOT trigger auto-flush.
        push_n(&p, 63);
        // Buffer should hold 63 items (no flush because model isn't available,
        // but maybe_embed_text returns early from flush_embed_buffer when model
        // fails — the key thing is the buffer was drained by flush attempt).
        //
        // Actually: flush_embed_buffer drains the buffer THEN tries model.
        // If model fails, items are lost but buffer is empty.
        // So with 63 items (< 64), no flush triggered → buffer has 63.
        assert_eq!(buffer_len(&p), 63);

        // Push the 64th item — triggers auto-flush → buffer drained.
        push_n(&p, 1);
        // The flush drains the buffer (even though model load fails, the
        // drain via mem::take already happened).
        assert_eq!(buffer_len(&p), 0);
    }

    #[test]
    fn test_manual_flush_drains_buffer() {
        let p = setup();

        push_n(&p, 10);
        assert_eq!(buffer_len(&p), 10);

        // Manual flush drains the buffer (model load will fail in test, but
        // the drain is the first operation).
        flush_embed_buffer(&p);
        assert_eq!(buffer_len(&p), 0);
    }

    #[test]
    fn test_flush_empty_buffer_is_noop() {
        let p = setup();
        assert_eq!(buffer_len(&p), 0);

        // Should not panic or error.
        flush_embed_buffer(&p);
        assert_eq!(buffer_len(&p), 0);
    }

    #[test]
    fn test_delete_removes_pending_embed_from_buffer() {
        let p = setup();
        let branch_id = BranchId::default();

        // Buffer 3 items with different keys.
        maybe_embed_text(
            &p,
            branch_id,
            "default",
            SHADOW_KV,
            "keep-1",
            "text one",
            strata_core::EntityRef::kv(branch_id, "keep-1"),
        );
        maybe_embed_text(
            &p,
            branch_id,
            "default",
            SHADOW_KV,
            "to-delete",
            "text two",
            strata_core::EntityRef::kv(branch_id, "to-delete"),
        );
        maybe_embed_text(
            &p,
            branch_id,
            "default",
            SHADOW_KV,
            "keep-2",
            "text three",
            strata_core::EntityRef::kv(branch_id, "keep-2"),
        );
        assert_eq!(buffer_len(&p), 3);

        // Delete the middle key — should remove it from the buffer.
        maybe_remove_embedding(&p, branch_id, "default", SHADOW_KV, "to-delete");
        assert_eq!(buffer_len(&p), 2);

        // Verify the correct items remain.
        let buf = p.db.extension::<EmbedBuffer>().unwrap();
        let pending = buf.pending.lock().unwrap();
        assert_eq!(pending[0].key, "keep-1");
        assert_eq!(pending[1].key, "keep-2");
    }

    #[test]
    fn test_delete_nonexistent_key_leaves_buffer_intact() {
        let p = setup();
        let branch_id = BranchId::default();

        push_n(&p, 3);
        assert_eq!(buffer_len(&p), 3);

        // Delete a key that's not in the buffer — buffer unchanged.
        maybe_remove_embedding(&p, branch_id, "default", SHADOW_KV, "no-such-key");
        assert_eq!(buffer_len(&p), 3);
    }

    #[test]
    fn test_delete_only_removes_matching_collection() {
        let p = setup();
        let branch_id = BranchId::default();

        // Buffer an item in SHADOW_KV.
        maybe_embed_text(
            &p,
            branch_id,
            "default",
            SHADOW_KV,
            "shared-key",
            "kv text",
            strata_core::EntityRef::kv(branch_id, "shared-key"),
        );
        // Buffer an item in SHADOW_JSON with the same key name.
        maybe_embed_text(
            &p,
            branch_id,
            "default",
            SHADOW_JSON,
            "shared-key",
            "json text",
            strata_core::EntityRef::json(branch_id, "shared-key"),
        );
        assert_eq!(buffer_len(&p), 2);

        // Delete from SHADOW_KV only — SHADOW_JSON entry should remain.
        maybe_remove_embedding(&p, branch_id, "default", SHADOW_KV, "shared-key");
        assert_eq!(buffer_len(&p), 1);

        let buf = p.db.extension::<EmbedBuffer>().unwrap();
        let pending = buf.pending.lock().unwrap();
        assert_eq!(pending[0].shadow_collection, SHADOW_JSON);
    }

    #[test]
    fn test_disabled_auto_embed_skips_buffering() {
        let p = setup();
        p.db.set_auto_embed(false);

        push_n(&p, 10);
        // Nothing buffered because auto_embed is disabled.
        assert_eq!(buffer_len(&p), 0);
    }

    #[test]
    fn test_executor_drop_flushes_buffer() {
        use crate::Executor;

        let db = strata_engine::Database::cache().expect("open cache db");
        db.set_auto_embed(true);
        let executor = Executor::new(db);

        // Ensure the default branch exists.
        executor
            .execute(crate::Command::Ping)
            .expect("ping works");

        let branch_id = BranchId::default();
        let p = executor.primitives().clone();

        // Buffer some items.
        for i in 0..5 {
            maybe_embed_text(
                &p,
                branch_id,
                "default",
                SHADOW_KV,
                &format!("drop-key-{}", i),
                &format!("text {}", i),
                strata_core::EntityRef::kv(branch_id, &format!("drop-key-{}", i)),
            );
        }
        assert_eq!(buffer_len(&p), 5);

        // Drop the executor — should flush the buffer.
        drop(executor);

        // Buffer should be empty after drop (items drained, even if model
        // load fails the drain still happens).
        assert_eq!(buffer_len(&p), 0);
    }
}

/// Ensure a shadow collection exists, swallowing AlreadyExists errors.
///
/// Uses a per-Database cache to avoid repeated creation attempts on every write.
///
/// # Race Condition Safety
///
/// The check-then-act on `state.shadow_collections_created` is intentionally
/// non-atomic. If two threads race past the `contains_key` fast-path
/// simultaneously, both will call `create_system_collection`. The first call
/// succeeds; the second returns `CollectionAlreadyExists`, which is caught and
/// treated identically to success (the cache entry is inserted). This makes
/// the pattern safe despite the race: the worst case is a redundant creation
/// attempt that is harmlessly swallowed.
#[cfg(feature = "embed")]
fn ensure_shadow_collection(
    p: &Arc<Primitives>,
    branch_id: strata_core::types::BranchId,
    name: &str,
) {
    use strata_core::primitives::vector::VectorConfig;

    let cache_key = format!("{:?}{}{}", branch_id.as_bytes(), SHADOW_KEY_SEP, name);
    let state = match p.db.extension::<AutoEmbedState>() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(target: "strata::embed", error = %e, "Failed to get auto-embed state");
            return;
        }
    };

    // Fast path: already created in this process lifetime
    if state.contains(&cache_key) {
        return;
    }

    let config = VectorConfig::for_minilm();

    match p.vector.create_system_collection(branch_id, name, config) {
        Ok(_) => {
            tracing::info!(target: "strata::embed", collection = name, "Created shadow embedding collection");
            state.insert(cache_key);
        }
        Err(strata_engine::vector::VectorError::CollectionAlreadyExists { .. }) => {
            // Already exists from a previous process run — mark as created
            state.insert(cache_key);
        }
        Err(e) => {
            tracing::warn!(
                target: "strata::embed",
                collection = name,
                error = %e,
                "Failed to create shadow collection"
            );
        }
    }
}
