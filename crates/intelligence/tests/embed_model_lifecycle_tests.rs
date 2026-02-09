//! Integration tests for the full model lifecycle.
//!
//! All tests require real model files and are `#[ignore]` by default.
//! Run with: cargo test -p strata-intelligence --features embed -- --include-ignored

#![cfg(feature = "embed")]

use std::path::Path;
use strata_intelligence::embed::model::EmbedModel;
use strata_intelligence::embed::EmbedModelState;

fn model_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/minilm-l6-v2")
}

fn load_model() -> EmbedModel {
    let dir = model_dir();
    let safetensors_bytes =
        std::fs::read(dir.join("model.safetensors")).expect("model.safetensors not found");
    let vocab_text = std::fs::read_to_string(dir.join("vocab.txt")).expect("vocab.txt not found");
    EmbedModel::load(&safetensors_bytes, &vocab_text).expect("failed to load model")
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

#[test]
#[ignore]
fn test_model_load_and_embed() {
    let model = load_model();
    let embedding = model.embed("hello world");
    assert_eq!(embedding.len(), 384);
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-4,
        "L2 norm = {}, expected 1.0",
        norm
    );
}

#[test]
#[ignore]
fn test_similar_texts_have_similar_embeddings() {
    let model = load_model();
    let a = model.embed("the cat sat on the mat");
    let b = model.embed("a cat rested on the mat");
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim > 0.8,
        "similar texts should have cosine similarity > 0.8, got {}",
        sim
    );
}

#[test]
#[ignore]
fn test_dissimilar_texts_have_low_similarity() {
    let model = load_model();
    let a = model.embed("quantum physics");
    let b = model.embed("chocolate cake recipe");
    let sim = cosine_similarity(&a, &b);
    assert!(
        sim < 0.5,
        "dissimilar texts should have cosine similarity < 0.5, got {}",
        sim
    );
}

#[test]
#[ignore]
fn test_embed_model_state_caches_across_calls() {
    let state = EmbedModelState::default();
    let dir = model_dir();

    let arc1 = state.get_or_load(&dir).expect("first load");
    let arc2 = state.get_or_load(&dir).expect("second load");

    // Same Arc (pointer equality) — model was only loaded once.
    assert!(
        std::sync::Arc::ptr_eq(&arc1, &arc2),
        "get_or_load should return the same Arc on second call"
    );
}
