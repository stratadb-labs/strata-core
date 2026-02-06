//! Auto-embedding module: MiniLM-L6-v2 text embeddings.
//!
//! Provides a lazy-loading model lifecycle via [`EmbedModelState`] and
//! text extraction from Strata [`Value`] types.

pub mod extract;
pub mod model;
pub mod tokenizer;

use std::path::Path;
use std::sync::Arc;

use model::EmbedModel;

/// Lazy-loading model state stored as a Database extension.
///
/// On first use, loads the MiniLM-L6-v2 model from the model directory.
/// If model files are missing, stores the error and never retries.
pub struct EmbedModelState {
    model: once_cell::sync::OnceCell<Result<Arc<EmbedModel>, String>>,
}

impl Default for EmbedModelState {
    fn default() -> Self {
        Self {
            model: once_cell::sync::OnceCell::new(),
        }
    }
}

impl EmbedModelState {
    /// Get or load the embedding model.
    ///
    /// Loads from `model_dir/model.safetensors` and `model_dir/vocab.txt`.
    /// Caches the result (success or failure) so filesystem is probed at most once.
    pub fn get_or_load(&self, model_dir: &Path) -> Result<Arc<EmbedModel>, String> {
        self.model
            .get_or_init(|| {
                let safetensors_path = model_dir.join("model.safetensors");
                let vocab_path = model_dir.join("vocab.txt");

                let safetensors_bytes = std::fs::read(&safetensors_path).map_err(|e| {
                    format!(
                        "Failed to read model file '{}': {}",
                        safetensors_path.display(),
                        e
                    )
                })?;

                let vocab_text = std::fs::read_to_string(&vocab_path).map_err(|e| {
                    format!(
                        "Failed to read vocab file '{}': {}",
                        vocab_path.display(),
                        e
                    )
                })?;

                let model = EmbedModel::load(&safetensors_bytes, &vocab_text)?;
                Ok(Arc::new(model))
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_is_empty() {
        let state = EmbedModelState::default();
        let result = state.get_or_load(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_model_file_error_message() {
        let dir = tempfile::tempdir().unwrap();
        let state = EmbedModelState::default();
        let result = state.get_or_load(dir.path());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains(dir.path().to_str().unwrap()),
            "error should contain the path: {}",
            err
        );
    }

    #[test]
    fn test_error_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let state = EmbedModelState::default();
        let err1 = state.get_or_load(dir.path()).err().unwrap();
        let err2 = state.get_or_load(dir.path()).err().unwrap();
        assert_eq!(err1, err2, "error should be cached and identical");
    }

    #[test]
    fn test_missing_vocab_file() {
        let dir = tempfile::tempdir().unwrap();
        // Create a model.safetensors file (minimal valid safetensors)
        let header = b"{}";
        let header_len = header.len() as u64;
        let mut buf = Vec::new();
        buf.extend_from_slice(&header_len.to_le_bytes());
        buf.extend_from_slice(header);
        std::fs::write(dir.path().join("model.safetensors"), &buf).unwrap();

        let state = EmbedModelState::default();
        let result = state.get_or_load(dir.path());
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains("vocab"),
            "error should mention 'vocab': {}",
            err
        );
    }
}
