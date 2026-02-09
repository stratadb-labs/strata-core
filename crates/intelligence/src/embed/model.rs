//! MiniLM-L6-v2 encoder architecture and forward pass.

use crate::runtime::safetensors::SafeTensors;
use crate::runtime::tensor::Tensor;

use super::tokenizer::{TokenizedInput, WordPieceTokenizer};

const HIDDEN_SIZE: usize = 384;
const NUM_HEADS: usize = 12;
const HEAD_DIM: usize = HIDDEN_SIZE / NUM_HEADS; // 32
const NUM_LAYERS: usize = 6;
const VOCAB_SIZE: usize = 30522;
const LAYER_NORM_EPS: f32 = 1e-12;

/// A single transformer encoder layer.
struct TransformerLayer {
    q_weight: Tensor,
    q_bias: Vec<f32>,
    k_weight: Tensor,
    k_bias: Vec<f32>,
    v_weight: Tensor,
    v_bias: Vec<f32>,
    attn_output_weight: Tensor,
    attn_output_bias: Vec<f32>,
    attn_ln_weight: Vec<f32>,
    attn_ln_bias: Vec<f32>,
    intermediate_weight: Tensor,
    intermediate_bias: Vec<f32>,
    output_weight: Tensor,
    output_bias: Vec<f32>,
    output_ln_weight: Vec<f32>,
    output_ln_bias: Vec<f32>,
}

/// The MiniLM-L6-v2 embedding model.
pub struct EmbedModel {
    tokenizer: WordPieceTokenizer,
    word_embeddings: Tensor,
    position_embeddings: Tensor,
    token_type_embeddings: Tensor,
    embed_ln_weight: Vec<f32>,
    embed_ln_bias: Vec<f32>,
    layers: Vec<TransformerLayer>,
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

        let embed_ln_weight = st
            .tensor_1d(&format!("{}embeddings.LayerNorm.weight", prefix))
            .ok_or("Missing embeddings LayerNorm weight")?;

        let embed_ln_bias = st
            .tensor_1d(&format!("{}embeddings.LayerNorm.bias", prefix))
            .ok_or("Missing embeddings LayerNorm bias")?;

        let mut layers = Vec::with_capacity(NUM_LAYERS);
        for i in 0..NUM_LAYERS {
            let layer_prefix = format!("{}encoder.layer.{}", prefix, i);
            let layer = TransformerLayer {
                q_weight: st
                    .tensor(&format!("{}.attention.self.query.weight", layer_prefix))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.self.query.weight", layer_prefix)
                    })?,
                q_bias: st
                    .tensor_1d(&format!("{}.attention.self.query.bias", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.attention.self.query.bias", layer_prefix))?,
                k_weight: st
                    .tensor(&format!("{}.attention.self.key.weight", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.attention.self.key.weight", layer_prefix))?,
                k_bias: st
                    .tensor_1d(&format!("{}.attention.self.key.bias", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.attention.self.key.bias", layer_prefix))?,
                v_weight: st
                    .tensor(&format!("{}.attention.self.value.weight", layer_prefix))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.self.value.weight", layer_prefix)
                    })?,
                v_bias: st
                    .tensor_1d(&format!("{}.attention.self.value.bias", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.attention.self.value.bias", layer_prefix))?,
                attn_output_weight: st
                    .tensor(&format!("{}.attention.output.dense.weight", layer_prefix))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.output.dense.weight", layer_prefix)
                    })?,
                attn_output_bias: st
                    .tensor_1d(&format!("{}.attention.output.dense.bias", layer_prefix))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.output.dense.bias", layer_prefix)
                    })?,
                attn_ln_weight: st
                    .tensor_1d(&format!(
                        "{}.attention.output.LayerNorm.weight",
                        layer_prefix
                    ))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.output.LayerNorm.weight", layer_prefix)
                    })?,
                attn_ln_bias: st
                    .tensor_1d(&format!("{}.attention.output.LayerNorm.bias", layer_prefix))
                    .ok_or_else(|| {
                        format!("Missing {}.attention.output.LayerNorm.bias", layer_prefix)
                    })?,
                intermediate_weight: st
                    .tensor(&format!("{}.intermediate.dense.weight", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.intermediate.dense.weight", layer_prefix))?,
                intermediate_bias: st
                    .tensor_1d(&format!("{}.intermediate.dense.bias", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.intermediate.dense.bias", layer_prefix))?,
                output_weight: st
                    .tensor(&format!("{}.output.dense.weight", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.output.dense.weight", layer_prefix))?,
                output_bias: st
                    .tensor_1d(&format!("{}.output.dense.bias", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.output.dense.bias", layer_prefix))?,
                output_ln_weight: st
                    .tensor_1d(&format!("{}.output.LayerNorm.weight", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.output.LayerNorm.weight", layer_prefix))?,
                output_ln_bias: st
                    .tensor_1d(&format!("{}.output.LayerNorm.bias", layer_prefix))
                    .ok_or_else(|| format!("Missing {}.output.LayerNorm.bias", layer_prefix))?,
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
        })
    }

    /// Embed a text string into a 384-dimensional vector.
    pub fn embed(&self, text: &str) -> Vec<f32> {
        let input = self.tokenizer.tokenize(text);
        let seq_len = input.input_ids.len();

        // 1. Gather embeddings
        let hidden = self.gather_embeddings(&input, seq_len);

        // 2. Layer norm
        let mut hidden =
            hidden.layer_norm(&self.embed_ln_weight, &self.embed_ln_bias, LAYER_NORM_EPS);

        // 3. Transformer layers
        for layer in &self.layers {
            hidden = self.transformer_layer(layer, &hidden, &input.attention_mask);
        }

        // 4. Mean pooling (exclude padding)
        let pooled = self.mean_pool(&hidden, &input.attention_mask);

        // 5. L2 normalize
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
        hidden: &Tensor,
        attention_mask: &[u32],
    ) -> Tensor {
        let seq_len = hidden.rows;

        // Self-attention
        // Q, K, V projections: hidden × Wᵀ + b
        // BERT stores weights transposed: shape is (out, in), so we use matmul_transpose
        let mut q = hidden.matmul_transpose(&layer.q_weight);
        q.add_bias(&layer.q_bias);
        let mut k = hidden.matmul_transpose(&layer.k_weight);
        k.add_bias(&layer.k_bias);
        let mut v = hidden.matmul_transpose(&layer.v_weight);
        v.add_bias(&layer.v_bias);

        // Multi-head attention
        let scale = 1.0 / (HEAD_DIM as f32).sqrt();
        let mut attn_output_data = vec![0.0f32; seq_len * HIDDEN_SIZE];

        for head in 0..NUM_HEADS {
            let head_offset = head * HEAD_DIM;

            // Extract Q, K, V slices for this head
            let mut q_head = Tensor::zeros(seq_len, HEAD_DIM);
            let mut k_head = Tensor::zeros(seq_len, HEAD_DIM);
            let mut v_head = Tensor::zeros(seq_len, HEAD_DIM);

            for s in 0..seq_len {
                for d in 0..HEAD_DIM {
                    q_head.data[s * HEAD_DIM + d] = q.data[s * HIDDEN_SIZE + head_offset + d];
                    k_head.data[s * HEAD_DIM + d] = k.data[s * HIDDEN_SIZE + head_offset + d];
                    v_head.data[s * HEAD_DIM + d] = v.data[s * HIDDEN_SIZE + head_offset + d];
                }
            }

            // Attention scores: Q × Kᵀ / √d_k
            let mut scores = q_head.matmul_transpose(&k_head);
            scores.scale(scale);

            // Apply attention mask (set padding positions to -10000)
            for i in 0..seq_len {
                for j in 0..seq_len {
                    if attention_mask[j] == 0 {
                        scores.data[i * seq_len + j] = -10000.0;
                    }
                }
            }

            scores.softmax_rows();

            // Weighted sum: scores × V
            let context = scores.matmul(&v_head);

            // Copy back to full hidden dim
            for s in 0..seq_len {
                for d in 0..HEAD_DIM {
                    attn_output_data[s * HIDDEN_SIZE + head_offset + d] =
                        context.data[s * HEAD_DIM + d];
                }
            }
        }

        let attn_output = Tensor::from_slice(&attn_output_data, seq_len, HIDDEN_SIZE);

        // Output projection
        let mut projected = attn_output.matmul_transpose(&layer.attn_output_weight);
        projected.add_bias(&layer.attn_output_bias);

        // Residual + LayerNorm
        let post_attn = projected.add_tensor(hidden);
        let normed_attn =
            post_attn.layer_norm(&layer.attn_ln_weight, &layer.attn_ln_bias, LAYER_NORM_EPS);

        // FFN: intermediate
        let mut intermediate = normed_attn.matmul_transpose(&layer.intermediate_weight);
        intermediate.add_bias(&layer.intermediate_bias);
        let intermediate = intermediate.gelu();

        // FFN: output
        let mut output = intermediate.matmul_transpose(&layer.output_weight);
        output.add_bias(&layer.output_bias);

        // Residual + LayerNorm
        let post_ffn = output.add_tensor(&normed_attn);
        post_ffn.layer_norm(
            &layer.output_ln_weight,
            &layer.output_ln_bias,
            LAYER_NORM_EPS,
        )
    }

    fn mean_pool(&self, hidden: &Tensor, attention_mask: &[u32]) -> Vec<f32> {
        let seq_len = hidden.rows;
        let mut sum = vec![0.0f32; HIDDEN_SIZE];
        let mut count = 0.0f32;

        for s in 0..seq_len {
            if attention_mask[s] == 1 {
                let row = hidden.row(s);
                for i in 0..HIDDEN_SIZE {
                    sum[i] += row[i];
                }
                count += 1.0;
            }
        }

        if count > 0.0 {
            for v in sum.iter_mut() {
                *v /= count;
            }
        }

        sum
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
