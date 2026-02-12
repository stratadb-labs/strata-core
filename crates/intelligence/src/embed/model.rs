//! MiniLM-L6-v2 encoder architecture and forward pass.

use std::sync::Arc;

use crate::runtime::backend::{select_backend, ComputeBackend, DeviceTensor};
use crate::runtime::safetensors::SafeTensors;
use crate::runtime::tensor::Tensor;

use super::tokenizer::{TokenizedInput, WordPieceTokenizer};

const HIDDEN_SIZE: usize = 384;
const NUM_HEADS: usize = 12;
const HEAD_DIM: usize = HIDDEN_SIZE / NUM_HEADS; // 32
const NUM_LAYERS: usize = 6;
const VOCAB_SIZE: usize = 30522;
const LAYER_NORM_EPS: f32 = 1e-12;

/// A single transformer encoder layer with weights on the compute device.
struct TransformerLayer {
    q_weight: DeviceTensor,
    q_bias: DeviceTensor,
    k_weight: DeviceTensor,
    k_bias: DeviceTensor,
    v_weight: DeviceTensor,
    v_bias: DeviceTensor,
    attn_output_weight: DeviceTensor,
    attn_output_bias: DeviceTensor,
    attn_ln_weight: DeviceTensor,
    attn_ln_bias: DeviceTensor,
    intermediate_weight: DeviceTensor,
    intermediate_bias: DeviceTensor,
    output_weight: DeviceTensor,
    output_bias: DeviceTensor,
    output_ln_weight: DeviceTensor,
    output_ln_bias: DeviceTensor,
}

/// The MiniLM-L6-v2 embedding model.
pub struct EmbedModel {
    tokenizer: WordPieceTokenizer,
    // Embedding tables stay on CPU for gather (avoids uploading 45MB vocab table).
    word_embeddings: Tensor,
    position_embeddings: Tensor,
    token_type_embeddings: Tensor,
    // Layer norm weights on device.
    embed_ln_weight: DeviceTensor,
    embed_ln_bias: DeviceTensor,
    layers: Vec<TransformerLayer>,
    backend: Arc<dyn ComputeBackend>,
}

impl EmbedModel {
    /// Load model weights from SafeTensors bytes and vocabulary text.
    ///
    /// Supports both naming conventions:
    /// - HuggingFace BERT: `bert.embeddings.word_embeddings.weight`
    /// - Sentence Transformers: `embeddings.word_embeddings.weight`
    pub fn load(safetensors_bytes: &[u8], vocab_text: &str) -> Result<Self, String> {
        let st = SafeTensors::from_bytes(safetensors_bytes)?;
        let tokenizer = WordPieceTokenizer::from_vocab(vocab_text);
        let backend = select_backend();

        // Detect naming convention: try with "bert." prefix first, fall back to without.
        let prefix = if st
            .tensor("bert.embeddings.word_embeddings.weight")
            .is_some()
        {
            "bert."
        } else {
            ""
        };

        let word_embeddings = st
            .tensor(&format!("{}embeddings.word_embeddings.weight", prefix))
            .ok_or("Missing word_embeddings")?;

        if word_embeddings.rows != VOCAB_SIZE || word_embeddings.cols != HIDDEN_SIZE {
            return Err(format!(
                "word_embeddings shape mismatch: expected {}x{}, got {}x{}",
                VOCAB_SIZE, HIDDEN_SIZE, word_embeddings.rows, word_embeddings.cols
            ));
        }

        let position_embeddings = st
            .tensor(&format!("{}embeddings.position_embeddings.weight", prefix))
            .ok_or("Missing position_embeddings")?;

        let token_type_embeddings = st
            .tensor(&format!(
                "{}embeddings.token_type_embeddings.weight",
                prefix
            ))
            .ok_or("Missing token_type_embeddings")?;

        let embed_ln_weight = backend.upload_1d(
            &st.tensor_1d(&format!("{}embeddings.LayerNorm.weight", prefix))
                .ok_or("Missing embeddings LayerNorm weight")?,
        );

        let embed_ln_bias = backend.upload_1d(
            &st.tensor_1d(&format!("{}embeddings.LayerNorm.bias", prefix))
                .ok_or("Missing embeddings LayerNorm bias")?,
        );

        let mut layers = Vec::with_capacity(NUM_LAYERS);
        for i in 0..NUM_LAYERS {
            let lp = format!("{}encoder.layer.{}", prefix, i);
            let layer = TransformerLayer {
                q_weight: backend.upload(
                    &st.tensor(&format!("{}.attention.self.query.weight", lp))
                        .ok_or_else(|| {
                            format!("Missing {}.attention.self.query.weight", lp)
                        })?,
                ),
                q_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.attention.self.query.bias", lp))
                        .ok_or_else(|| format!("Missing {}.attention.self.query.bias", lp))?,
                ),
                k_weight: backend.upload(
                    &st.tensor(&format!("{}.attention.self.key.weight", lp))
                        .ok_or_else(|| format!("Missing {}.attention.self.key.weight", lp))?,
                ),
                k_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.attention.self.key.bias", lp))
                        .ok_or_else(|| format!("Missing {}.attention.self.key.bias", lp))?,
                ),
                v_weight: backend.upload(
                    &st.tensor(&format!("{}.attention.self.value.weight", lp))
                        .ok_or_else(|| {
                            format!("Missing {}.attention.self.value.weight", lp)
                        })?,
                ),
                v_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.attention.self.value.bias", lp))
                        .ok_or_else(|| format!("Missing {}.attention.self.value.bias", lp))?,
                ),
                attn_output_weight: backend.upload(
                    &st.tensor(&format!("{}.attention.output.dense.weight", lp))
                        .ok_or_else(|| {
                            format!("Missing {}.attention.output.dense.weight", lp)
                        })?,
                ),
                attn_output_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.attention.output.dense.bias", lp))
                        .ok_or_else(|| {
                            format!("Missing {}.attention.output.dense.bias", lp)
                        })?,
                ),
                attn_ln_weight: backend.upload_1d(
                    &st.tensor_1d(&format!(
                        "{}.attention.output.LayerNorm.weight",
                        lp
                    ))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.output.LayerNorm.weight", lp)
                    })?,
                ),
                attn_ln_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.attention.output.LayerNorm.bias", lp))
                        .ok_or_else(|| {
                            format!("Missing {}.attention.output.LayerNorm.bias", lp)
                        })?,
                ),
                intermediate_weight: backend.upload(
                    &st.tensor(&format!("{}.intermediate.dense.weight", lp))
                        .ok_or_else(|| format!("Missing {}.intermediate.dense.weight", lp))?,
                ),
                intermediate_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.intermediate.dense.bias", lp))
                        .ok_or_else(|| format!("Missing {}.intermediate.dense.bias", lp))?,
                ),
                output_weight: backend.upload(
                    &st.tensor(&format!("{}.output.dense.weight", lp))
                        .ok_or_else(|| format!("Missing {}.output.dense.weight", lp))?,
                ),
                output_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.output.dense.bias", lp))
                        .ok_or_else(|| format!("Missing {}.output.dense.bias", lp))?,
                ),
                output_ln_weight: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.output.LayerNorm.weight", lp))
                        .ok_or_else(|| format!("Missing {}.output.LayerNorm.weight", lp))?,
                ),
                output_ln_bias: backend.upload_1d(
                    &st.tensor_1d(&format!("{}.output.LayerNorm.bias", lp))
                        .ok_or_else(|| format!("Missing {}.output.LayerNorm.bias", lp))?,
                ),
            };
            layers.push(layer);
        }

        Ok(Self {
            tokenizer,
            word_embeddings,
            position_embeddings,
            token_type_embeddings,
            embed_ln_weight,
            embed_ln_bias,
            layers,
            backend,
        })
    }

    /// Embed a text string into a 384-dimensional vector.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let input = self.tokenizer.tokenize(text);
        let seq_len = input.input_ids.len();

        // 1. Gather embeddings (always on CPU — avoids uploading 45MB vocab table)
        let hidden_cpu = self.gather_embeddings(&input, seq_len);

        // 2. Upload to device
        let hidden = self.backend.upload(&hidden_cpu);

        // 3. Layer norm
        let mut hidden = self.backend.layer_norm(
            &hidden,
            &self.embed_ln_weight,
            &self.embed_ln_bias,
            LAYER_NORM_EPS,
        );

        // 4. Transformer layers
        for layer in &self.layers {
            hidden = self.transformer_layer(layer, &hidden, &input.attention_mask);
        }

        // 5. Mean pooling (returns host Vec)
        let pooled = self.backend.mean_pool(&hidden, &input.attention_mask);

        // 6. L2 normalize (CPU, trivial for 384 elements)
        l2_normalize(&pooled)
    }

    fn gather_embeddings(&self, input: &TokenizedInput, seq_len: usize) -> Tensor {
        let mut data = vec![0.0f32; seq_len * HIDDEN_SIZE];

        for (pos, (&token_id, &type_id)) in input
            .input_ids
            .iter()
            .zip(input.token_type_ids.iter())
            .enumerate()
        {
            let word_row = self.word_embeddings.row(token_id as usize);
            let pos_row = self.position_embeddings.row(pos);
            let type_row = self.token_type_embeddings.row(type_id as usize);

            let offset = pos * HIDDEN_SIZE;
            for i in 0..HIDDEN_SIZE {
                data[offset + i] = word_row[i] + pos_row[i] + type_row[i];
            }
        }

        Tensor::from_slice(&data, seq_len, HIDDEN_SIZE)
    }

    fn transformer_layer(
        &self,
        layer: &TransformerLayer,
        hidden: &DeviceTensor,
        attention_mask: &[u32],
    ) -> DeviceTensor {
        let seq_len = hidden.rows;
        let b = &self.backend;

        // Self-attention: Q, K, V projections
        // BERT stores weights transposed: shape is (out, in), so we use matmul_transpose
        let mut q = b.matmul_transpose(hidden, &layer.q_weight);
        b.add_bias(&mut q, &layer.q_bias);
        let mut k = b.matmul_transpose(hidden, &layer.k_weight);
        b.add_bias(&mut k, &layer.k_bias);
        let mut v = b.matmul_transpose(hidden, &layer.v_weight);
        b.add_bias(&mut v, &layer.v_bias);

        // Multi-head attention
        let scale = 1.0 / (HEAD_DIM as f32).sqrt();
        let mut attn_output = b.zeros(seq_len, HIDDEN_SIZE);

        for head in 0..NUM_HEADS {
            let offset = head * HEAD_DIM;

            // Extract Q, K, V slices for this head
            let q_head = b.slice_columns(&q, offset, offset + HEAD_DIM);
            let k_head = b.slice_columns(&k, offset, offset + HEAD_DIM);
            let v_head = b.slice_columns(&v, offset, offset + HEAD_DIM);

            // Attention scores: Q * K^T / sqrt(d_k)
            let mut scores = b.matmul_transpose(&q_head, &k_head);
            b.scale(&mut scores, scale);

            // Apply attention mask (set padding positions to -10000)
            b.apply_attention_mask(&mut scores, attention_mask);

            b.softmax_rows(&mut scores);

            // Weighted sum: scores * V
            let context = b.matmul(&scores, &v_head);

            // Copy back to full hidden dim
            b.scatter_columns(&mut attn_output, &context, offset);
        }

        // Output projection
        let mut projected = b.matmul_transpose(&attn_output, &layer.attn_output_weight);
        b.add_bias(&mut projected, &layer.attn_output_bias);

        // Residual + LayerNorm
        let post_attn = b.add_tensor(&projected, hidden);
        let normed_attn = b.layer_norm(
            &post_attn,
            &layer.attn_ln_weight,
            &layer.attn_ln_bias,
            LAYER_NORM_EPS,
        );

        // FFN: intermediate
        let mut intermediate = b.matmul_transpose(&normed_attn, &layer.intermediate_weight);
        b.add_bias(&mut intermediate, &layer.intermediate_bias);
        let intermediate = b.gelu(&intermediate);

        // FFN: output
        let mut output = b.matmul_transpose(&intermediate, &layer.output_weight);
        b.add_bias(&mut output, &layer.output_bias);

        // Residual + LayerNorm
        let post_ffn = b.add_tensor(&output, &normed_attn);
        b.layer_norm(
            &post_ffn,
            &layer.output_ln_weight,
            &layer.output_ln_bias,
            LAYER_NORM_EPS,
        )
    }
}

fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter().map(|x| x / norm).collect()
    } else {
        v.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize() {
        let v = vec![3.0, 4.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 0.6).abs() < 1e-6);
        assert!((n[1] - 0.8).abs() < 1e-6);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_l2_normalize_zero() {
        let v = vec![0.0, 0.0];
        let n = l2_normalize(&v);
        assert_eq!(n, vec![0.0, 0.0]);
    }

    #[test]
    fn test_l2_normalize_unit_norm() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm = {}, expected 1.0", norm);
    }

    #[test]
    fn test_l2_normalize_single_element() {
        let v = vec![5.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_negative_values() {
        let v = vec![-3.0, 4.0];
        let n = l2_normalize(&v);
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_l2_normalize_preserves_direction() {
        let v = vec![2.0, 4.0, 6.0];
        let n = l2_normalize(&v);
        // Ratios should be preserved: n[1]/n[0] ≈ 2.0, n[2]/n[0] ≈ 3.0
        assert!((n[1] / n[0] - 2.0).abs() < 1e-5);
        assert!((n[2] / n[0] - 3.0).abs() < 1e-5);
    }

    /// Build synthetic SafeTensors bytes with named tensors.
    /// Each entry is (name, dtype, shape, data_bytes).
    fn build_safetensors(entries: &[(&str, &str, &[usize], &[u8])]) -> Vec<u8> {
        let mut header_map = serde_json::Map::new();
        let mut offset = 0usize;
        let mut all_data = Vec::new();

        for &(name, dtype, shape, data) in entries {
            let end = offset + data.len();
            let info = serde_json::json!({
                "dtype": dtype,
                "shape": shape,
                "data_offsets": [offset, end],
            });
            header_map.insert(name.to_string(), info);
            all_data.extend_from_slice(data);
            offset = end;
        }

        let header_json = serde_json::to_string(&serde_json::Value::Object(header_map)).unwrap();
        let header_bytes = header_json.as_bytes();
        let header_len = header_bytes.len() as u64;

        let mut buf = Vec::new();
        buf.extend_from_slice(&header_len.to_le_bytes());
        buf.extend_from_slice(header_bytes);
        buf.extend_from_slice(&all_data);
        buf
    }

    fn f32_bytes(vals: &[f32]) -> Vec<u8> {
        vals.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn test_load_missing_word_embeddings() {
        // SafeTensors with a random tensor but no bert.embeddings.word_embeddings.weight
        let data = f32_bytes(&[1.0, 2.0]);
        let bytes = build_safetensors(&[("some_other_tensor", "F32", &[1, 2], &data)]);
        let result = EmbedModel::load(&bytes, "[PAD]\n[UNK]");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains("Missing"),
            "expected 'Missing' in error: {}",
            err
        );
    }

    #[test]
    fn test_load_wrong_dimensions() {
        // word_embeddings with wrong shape (2x2 instead of 30522x384)
        let data = f32_bytes(&[1.0, 2.0, 3.0, 4.0]);
        let bytes = build_safetensors(&[(
            "bert.embeddings.word_embeddings.weight",
            "F32",
            &[2, 2],
            &data,
        )]);
        let result = EmbedModel::load(&bytes, "[PAD]\n[UNK]");
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains("shape mismatch"),
            "expected 'shape mismatch' in error: {}",
            err
        );
    }

    #[test]
    fn test_load_missing_layer_weights() {
        // Provide embeddings with correct shape but no encoder layers.
        let embed_size = VOCAB_SIZE * HIDDEN_SIZE;
        let word_data = f32_bytes(&vec![0.0f32; embed_size]);
        let pos_size = 512 * HIDDEN_SIZE; // BERT max position embeddings
        let pos_data = f32_bytes(&vec![0.0f32; pos_size]);
        let type_size = 2 * HIDDEN_SIZE; // 2 token types
        let type_data = f32_bytes(&vec![0.0f32; type_size]);
        let ln_data = f32_bytes(&vec![1.0f32; HIDDEN_SIZE]);
        let ln_bias = f32_bytes(&vec![0.0f32; HIDDEN_SIZE]);

        let bytes = build_safetensors(&[
            (
                "bert.embeddings.word_embeddings.weight",
                "F32",
                &[VOCAB_SIZE, HIDDEN_SIZE],
                &word_data,
            ),
            (
                "bert.embeddings.position_embeddings.weight",
                "F32",
                &[512, HIDDEN_SIZE],
                &pos_data,
            ),
            (
                "bert.embeddings.token_type_embeddings.weight",
                "F32",
                &[2, HIDDEN_SIZE],
                &type_data,
            ),
            (
                "bert.embeddings.LayerNorm.weight",
                "F32",
                &[HIDDEN_SIZE],
                &ln_data,
            ),
            (
                "bert.embeddings.LayerNorm.bias",
                "F32",
                &[HIDDEN_SIZE],
                &ln_bias,
            ),
        ]);

        let mut vocab_lines: Vec<String> = (0..VOCAB_SIZE).map(|i| format!("tok{}", i)).collect();
        vocab_lines[0] = "[PAD]".into();
        vocab_lines[100] = "[UNK]".into();
        vocab_lines[101] = "[CLS]".into();
        vocab_lines[102] = "[SEP]".into();
        let vocab = vocab_lines.join("\n");

        let result = EmbedModel::load(&bytes, &vocab);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            err.contains("Missing"),
            "expected 'Missing' in error for missing layer weights: {}",
            err
        );
    }

    #[test]
    #[ignore] // Requires real model files
    fn test_embed_produces_384_dim_unit_vector() {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let model_dir = workspace.join("models/minilm-l6-v2");
        let safetensors_bytes = std::fs::read(model_dir.join("model.safetensors"))
            .expect("model.safetensors not found");
        let vocab_text =
            std::fs::read_to_string(model_dir.join("vocab.txt")).expect("vocab.txt not found");

        let model = EmbedModel::load(&safetensors_bytes, &vocab_text).expect("load model");
        let embedding = model.embed("hello");

        assert_eq!(embedding.len(), HIDDEN_SIZE);
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "L2 norm = {}, expected 1.0",
            norm
        );
    }
}
