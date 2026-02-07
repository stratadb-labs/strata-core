//! Auto-embed hook called from write handlers.
//!
//! When auto-embedding is enabled and the `embed` feature is compiled in,
//! this module generates embeddings for text values and stores them in
//! shadow vector collections.

use std::sync::Arc;

use crate::bridge::Primitives;

/// Shadow collection names for each primitive type.
pub const SHADOW_KV: &str = "_system_embed_kv";
pub const SHADOW_JSON: &str = "_system_embed_json";
pub const SHADOW_EVENT: &str = "_system_embed_event";
pub const SHADOW_STATE: &str = "_system_embed_state";

/// Separator for composite shadow keys (ASCII Unit Separator).
/// Avoids ambiguity since "/" is allowed in both space and key names.
#[cfg(feature = "embed")]
const SHADOW_KEY_SEP: char = '\x1f';

/// Attempt to embed text and store in a shadow vector collection.
///
/// Best-effort: failures are logged, never propagated to the caller.
/// The `source_ref` traces the shadow embedding back to the originating record.
#[cfg(feature = "embed")]
pub fn maybe_embed_text(
    p: &Arc<Primitives>,
    branch_id: strata_core::types::BranchId,
    space: &str,
    shadow_collection: &str,
    key: &str,
    text: &str,
    source_ref: strata_core::EntityRef,
) {
    use strata_intelligence::embed::EmbedModelState;

    if !p.db.auto_embed_enabled() {
        return;
    }

    let model_dir = p.db.model_dir();
    let embed_state = p.db.extension::<EmbedModelState>();

    let model = match embed_state.get_or_load(&model_dir) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(target: "strata::embed", error = %e, "Failed to load embedding model");
            return;
        }
    };

    let embedding = model.embed(text);

    // Ensure shadow collection exists (384-dim cosine)
    ensure_shadow_collection(p, branch_id, shadow_collection);

    // Build composite key: "{space}\x1f{key}"
    let composite_key = format!("{}{}{}", space, SHADOW_KEY_SEP, key);

    // Build source metadata
    let metadata = serde_json::json!({
        "source_space": space,
        "source_key": key,
    });

    if let Err(e) = p.vector.system_insert_with_source(
        branch_id,
        shadow_collection,
        &composite_key,
        &embedding,
        Some(metadata),
        source_ref,
    ) {
        tracing::warn!(
            target: "strata::embed",
            collection = shadow_collection,
            key = composite_key,
            error = %e,
            "Failed to insert embedding"
        );
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

/// Remove a shadow embedding entry on delete.
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
    use strata_engine::database::AutoEmbedState;

    let cache_key = format!("{:?}{}{}", branch_id.as_bytes(), SHADOW_KEY_SEP, name);
    let state = p.db.extension::<AutoEmbedState>();

    // Fast path: already created in this process lifetime
    if state.shadow_collections_created.contains_key(&cache_key) {
        return;
    }

    let config = VectorConfig::for_minilm();

    match p.vector.create_system_collection(branch_id, name, config) {
        Ok(_) => {
            tracing::info!(target: "strata::embed", collection = name, "Created shadow embedding collection");
            state.shadow_collections_created.insert(cache_key, ());
        }
        Err(strata_engine::vector::VectorError::CollectionAlreadyExists { .. }) => {
            // Already exists from a previous process run — mark as created
            state.shadow_collections_created.insert(cache_key, ());
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
