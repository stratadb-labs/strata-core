//! Minimal 2D tensor with matmul, GELU, LayerNorm, softmax.

/// A 2D row-major tensor of f32 values.
#[derive(Debug, Clone)]
pub struct Tensor {
    /// Flat row-major data buffer.
    pub data: Vec<f32>,
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
}

impl Tensor {
    /// Create a tensor of zeros.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            data: vec![0.0; rows * cols],
            rows,
            cols,
        }
    }

    /// Create a tensor from a flat slice.
    pub fn from_slice(data: &[f32], rows: usize, cols: usize) -> Self {
        assert_eq!(data.len(), rows * cols, "data length mismatch");
        Self {
            data: data.to_vec(),
            rows,
            cols,
        }
    }

    /// Get a single row as a slice.
    pub fn row(&self, r: usize) -> &[f32] {
        let start = r * self.cols;
        &self.data[start..start + self.cols]
    }

    /// Slice a range of rows into a new tensor.
    pub fn slice_rows(&self, start: usize, end: usize) -> Self {
        let s = start * self.cols;
        let e = end * self.cols;
        Self {
            data: self.data[s..e].to_vec(),
            rows: end - start,
            cols: self.cols,
        }
    }

    /// Matrix multiply: (M,K) × (K,N) → (M,N)
    pub fn matmul(&self, other: &Tensor) -> Tensor {
        assert_eq!(self.cols, other.rows, "matmul dimension mismatch");
        let m = self.rows;
        let k = self.cols;
        let n = other.cols;
        let mut out = vec![0.0f32; m * n];

        for i in 0..m {
            let a_row = i * k;
            let o_row = i * n;
            for p in 0..k {
                let a_val = self.data[a_row + p];
                let b_row = p * n;
                for j in 0..n {
                    out[o_row + j] += a_val * other.data[b_row + j];
                }
            }
        }

        Tensor {
            data: out,
            rows: m,
            cols: n,
        }
    }

    /// Matrix multiply with transpose: (M,K) × (N,K)ᵀ → (M,N)
    pub fn matmul_transpose(&self, other: &Tensor) -> Tensor {
        assert_eq!(self.cols, other.cols, "matmul_transpose dimension mismatch");
        let m = self.rows;
        let k = self.cols;
        let n = other.rows;
        let mut out = vec![0.0f32; m * n];

        for i in 0..m {
            let a_row = i * k;
            let o_row = i * n;
            for j in 0..n {
                let b_row = j * k;
                let mut sum = 0.0f32;
                for p in 0..k {
                    sum += self.data[a_row + p] * other.data[b_row + p];
                }
                out[o_row + j] = sum;
            }
        }

        Tensor {
            data: out,
            rows: m,
            cols: n,
        }
    }

    /// Broadcast row-add: add a 1D bias to each row.
    pub fn add_bias(&mut self, bias: &[f32]) {
        assert_eq!(bias.len(), self.cols, "bias length mismatch");
        for r in 0..self.rows {
            let start = r * self.cols;
            for c in 0..self.cols {
                self.data[start + c] += bias[c];
            }
        }
    }

    /// Element-wise add (residual connections).
    pub fn add_tensor(&self, other: &Tensor) -> Tensor {
        assert_eq!(
            self.data.len(),
            other.data.len(),
            "add_tensor size mismatch"
        );
        let data: Vec<f32> = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a + b)
            .collect();
        Tensor {
            data,
            rows: self.rows,
            cols: self.cols,
        }
    }

    /// Fast GELU approximation: x * 0.5 * (1 + tanh(sqrt(2/π) * (x + 0.044715 * x³)))
    pub fn gelu(&self) -> Tensor {
        const SQRT_2_OVER_PI: f32 = 0.797_884_56; // sqrt(2/π)
        let data: Vec<f32> = self
            .data
            .iter()
            .map(|&x| {
                let inner = SQRT_2_OVER_PI * (x + 0.044715 * x * x * x);
                0.5 * x * (1.0 + inner.tanh())
            })
            .collect();
        Tensor {
            data,
            rows: self.rows,
            cols: self.cols,
        }
    }

    /// Layer normalization per row.
    pub fn layer_norm(&self, weight: &[f32], bias: &[f32], eps: f32) -> Tensor {
        assert_eq!(weight.len(), self.cols);
        assert_eq!(bias.len(), self.cols);
        let mut out = vec![0.0f32; self.data.len()];

        for r in 0..self.rows {
            let start = r * self.cols;
            let row = &self.data[start..start + self.cols];

            // Mean
            let mean: f32 = row.iter().sum::<f32>() / self.cols as f32;

            // Variance
            let var: f32 =
                row.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / self.cols as f32;
            let inv_std = 1.0 / (var + eps).sqrt();

            for c in 0..self.cols {
                out[start + c] = (row[c] - mean) * inv_std * weight[c] + bias[c];
            }
        }

        Tensor {
            data: out,
            rows: self.rows,
            cols: self.cols,
        }
    }

    /// Per-row softmax with max-subtraction for numerical stability.
    pub fn softmax_rows(&mut self) {
        for r in 0..self.rows {
            let start = r * self.cols;
            let row = &mut self.data[start..start + self.cols];

            let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let mut sum = 0.0f32;
            for v in row.iter_mut() {
                *v = (*v - max).exp();
                sum += *v;
            }
            if sum > 0.0 {
                for v in row.iter_mut() {
                    *v /= sum;
                }
            }
        }
    }

    /// Scalar multiply.
    pub fn scale(&mut self, factor: f32) {
        for v in self.data.iter_mut() {
            *v *= factor;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zeros() {
        let t = Tensor::zeros(2, 3);
        assert_eq!(t.data, vec![0.0; 6]);
    }

    #[test]
    fn test_matmul_2x3_times_3x2() {
        // [[1,2,3],[4,5,6]] × [[7,8],[9,10],[11,12]]
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let b = Tensor::from_slice(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0], 3, 2);
        let c = a.matmul(&b);
        assert_eq!(c.rows, 2);
        assert_eq!(c.cols, 2);
        // [1*7+2*9+3*11, 1*8+2*10+3*12] = [58, 64]
        // [4*7+5*9+6*11, 4*8+5*10+6*12] = [139, 154]
        assert_eq!(c.data, vec![58.0, 64.0, 139.0, 154.0]);
    }

    #[test]
    fn test_matmul_transpose() {
        // A=(2,3), B=(2,3) → A × Bᵀ = (2,2)
        let a = Tensor::from_slice(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 2, 3);
        let b = Tensor::from_slice(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0], 2, 3);
        let c = a.matmul_transpose(&b);
        assert_eq!(c.rows, 2);
        assert_eq!(c.cols, 2);
        assert_eq!(c.data, vec![1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_add_bias() {
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        t.add_bias(&[10.0, 20.0]);
        assert_eq!(t.data, vec![11.0, 22.0, 13.0, 24.0]);
    }

    #[test]
    fn test_add_tensor() {
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let b = Tensor::from_slice(&[10.0, 20.0, 30.0, 40.0], 2, 2);
        let c = a.add_tensor(&b);
        assert_eq!(c.data, vec![11.0, 22.0, 33.0, 44.0]);
    }

    #[test]
    fn test_gelu_zero() {
        let t = Tensor::from_slice(&[0.0], 1, 1);
        let g = t.gelu();
        assert!((g.data[0]).abs() < 1e-6);
    }

    #[test]
    fn test_layer_norm() {
        // Simple case: [1, 3] → mean=2, var=1, normalized [-1, 1]
        let t = Tensor::from_slice(&[1.0, 3.0], 1, 2);
        let w = vec![1.0, 1.0];
        let b = vec![0.0, 0.0];
        let n = t.layer_norm(&w, &b, 1e-5);
        assert!((n.data[0] - (-1.0)).abs() < 1e-4);
        assert!((n.data[1] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_softmax_rows() {
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0], 1, 3);
        t.softmax_rows();
        let sum: f32 = t.data.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
        // Values should be monotonically increasing
        assert!(t.data[0] < t.data[1]);
        assert!(t.data[1] < t.data[2]);
    }

    #[test]
    fn test_scale() {
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        t.scale(2.0);
        assert_eq!(t.data, vec![2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn test_slice_rows() {
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, 2);
        let s = t.slice_rows(1, 3);
        assert_eq!(s.rows, 2);
        assert_eq!(s.data, vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn test_matmul_identity() {
        // Multiplying by 3x3 identity should return the original matrix.
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let identity = Tensor::from_slice(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], 3, 3);
        let result = a.matmul(&identity);
        assert_eq!(result.rows, 2);
        assert_eq!(result.cols, 3);
        for (a_val, r_val) in a.data.iter().zip(result.data.iter()) {
            assert!((a_val - r_val).abs() < 1e-6);
        }
    }

    #[test]
    fn test_matmul_transpose_non_identity() {
        // A = [[1, 2], [3, 4]], B = [[5, 6], [7, 8]]
        // A × Bᵀ = [[1*5+2*6, 1*7+2*8], [3*5+4*6, 3*7+4*8]]
        //         = [[17, 23], [39, 53]]
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let b = Tensor::from_slice(&[5.0, 6.0, 7.0, 8.0], 2, 2);
        let c = a.matmul_transpose(&b);
        assert_eq!(c.rows, 2);
        assert_eq!(c.cols, 2);
        assert!((c.data[0] - 17.0).abs() < 1e-6);
        assert!((c.data[1] - 23.0).abs() < 1e-6);
        assert!((c.data[2] - 39.0).abs() < 1e-6);
        assert!((c.data[3] - 53.0).abs() < 1e-6);
    }

    #[test]
    fn test_softmax_rows_multi_row() {
        // Each row should independently sum to 1.0.
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0, 10.0, 20.0, 30.0], 2, 3);
        t.softmax_rows();
        for r in 0..2 {
            let row_sum: f32 = t.row(r).iter().sum();
            assert!(
                (row_sum - 1.0).abs() < 1e-5,
                "row {} sum = {}, expected 1.0",
                r,
                row_sum
            );
        }
    }

    #[test]
    fn test_softmax_numerical_stability() {
        // Large values should not overflow — still sums to 1.0.
        let mut t = Tensor::from_slice(&[1000.0, 1001.0, 1000.5], 1, 3);
        t.softmax_rows();
        let sum: f32 = t.data.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-5,
            "softmax sum = {}, expected 1.0",
            sum
        );
        assert!(t.data.iter().all(|&v| v.is_finite()));
    }

    #[test]
    fn test_softmax_all_equal() {
        // Equal inputs → uniform distribution.
        let mut t = Tensor::from_slice(&[5.0, 5.0, 5.0, 5.0], 1, 4);
        t.softmax_rows();
        for &v in &t.data {
            assert!((v - 0.25).abs() < 1e-5, "expected uniform 0.25, got {}", v);
        }
    }

    #[test]
    fn test_layer_norm_multi_row() {
        // Two rows with different means/variances, each independently normalized.
        // Row 0: [1, 3] → mean=2, var=1, normed=[-1, 1]
        // Row 1: [2, 6] → mean=4, var=4, normed=[-1, 1]
        let t = Tensor::from_slice(&[1.0, 3.0, 2.0, 6.0], 2, 2);
        let w = vec![1.0, 1.0];
        let b = vec![0.0, 0.0];
        let n = t.layer_norm(&w, &b, 1e-5);
        // Both rows should normalize to approximately [-1, 1]
        assert!((n.data[0] - (-1.0)).abs() < 1e-4);
        assert!((n.data[1] - 1.0).abs() < 1e-4);
        assert!((n.data[2] - (-1.0)).abs() < 1e-4);
        assert!((n.data[3] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_layer_norm_with_weight_bias() {
        // [1, 3] → mean=2, var=1, normed=[-1, 1]
        // With weight=[2,2], bias=[10,10]: output = [-1*2+10, 1*2+10] = [8, 12]
        let t = Tensor::from_slice(&[1.0, 3.0], 1, 2);
        let w = vec![2.0, 2.0];
        let b = vec![10.0, 10.0];
        let n = t.layer_norm(&w, &b, 1e-5);
        assert!((n.data[0] - 8.0).abs() < 1e-4);
        assert!((n.data[1] - 12.0).abs() < 1e-4);
    }

    #[test]
    fn test_gelu_positive() {
        // For large positive x, GELU(x) ≈ x.
        let t = Tensor::from_slice(&[5.0], 1, 1);
        let g = t.gelu();
        assert!(
            (g.data[0] - 5.0).abs() < 0.01,
            "GELU(5.0) = {}, expected ≈5.0",
            g.data[0]
        );
    }

    #[test]
    fn test_gelu_negative() {
        // For large negative x, GELU(x) ≈ 0.
        let t = Tensor::from_slice(&[-5.0], 1, 1);
        let g = t.gelu();
        assert!(
            g.data[0].abs() < 0.01,
            "GELU(-5.0) = {}, expected ≈0.0",
            g.data[0]
        );
    }

    #[test]
    #[should_panic(expected = "data length mismatch")]
    fn test_from_slice_dimension_mismatch_panics() {
        // 5 floats cannot form a 2x3 tensor.
        Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0], 2, 3);
    }

    #[test]
    #[should_panic(expected = "matmul dimension mismatch")]
    fn test_matmul_dimension_mismatch_panics() {
        // A(2,3) × B(2,3): inner dims 3 ≠ 2.
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let b = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        a.matmul(&b);
    }

    #[test]
    fn test_row_access() {
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, 2);
        assert_eq!(t.row(0), &[1.0, 2.0]);
        assert_eq!(t.row(1), &[3.0, 4.0]);
        assert_eq!(t.row(2), &[5.0, 6.0]);
    }
}
