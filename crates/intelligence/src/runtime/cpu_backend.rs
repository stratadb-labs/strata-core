//! CPU compute backend wrapping existing Tensor operations.
//!
//! This is the default fallback backend. It wraps `Tensor` methods directly,
//! producing bit-identical results to the original non-backend code path.

use super::backend::{ComputeBackend, DeviceTensor};
use super::tensor::Tensor;

/// CPU compute backend — delegates to Tensor methods.
pub struct CpuBackend;

impl CpuBackend {
    fn as_tensor(dt: &DeviceTensor) -> &Tensor {
        dt.inner
            .downcast_ref::<Tensor>()
            .expect("CpuBackend: expected Tensor in DeviceTensor")
    }

    fn as_tensor_mut(dt: &mut DeviceTensor) -> &mut Tensor {
        dt.inner
            .downcast_mut::<Tensor>()
            .expect("CpuBackend: expected Tensor in DeviceTensor")
    }

    fn wrap(t: Tensor) -> DeviceTensor {
        DeviceTensor {
            rows: t.rows,
            cols: t.cols,
            inner: Box::new(t),
        }
    }

    fn as_1d(dt: &DeviceTensor) -> &[f32] {
        let t = Self::as_tensor(dt);
        assert_eq!(t.rows, 1, "expected 1-row tensor for 1D data");
        &t.data
    }
}

impl ComputeBackend for CpuBackend {
    fn upload(&self, t: &Tensor) -> DeviceTensor {
        Self::wrap(t.clone())
    }

    fn upload_1d(&self, v: &[f32]) -> DeviceTensor {
        Self::wrap(Tensor::from_slice(v, 1, v.len()))
    }

    fn download(&self, dt: &DeviceTensor) -> Tensor {
        Self::as_tensor(dt).clone()
    }

    fn matmul(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        Self::wrap(Self::as_tensor(a).matmul(Self::as_tensor(b)))
    }

    fn matmul_transpose(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        Self::wrap(Self::as_tensor(a).matmul_transpose(Self::as_tensor(b)))
    }

    fn add_bias(&self, t: &mut DeviceTensor, bias: &DeviceTensor) {
        let bias_data = Self::as_1d(bias);
        Self::as_tensor_mut(t).add_bias(bias_data);
    }

    fn add_tensor(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        Self::wrap(Self::as_tensor(a).add_tensor(Self::as_tensor(b)))
    }

    fn gelu(&self, t: &DeviceTensor) -> DeviceTensor {
        Self::wrap(Self::as_tensor(t).gelu())
    }

    fn layer_norm(
        &self,
        t: &DeviceTensor,
        w: &DeviceTensor,
        b: &DeviceTensor,
        eps: f32,
    ) -> DeviceTensor {
        let weight = Self::as_1d(w);
        let bias = Self::as_1d(b);
        Self::wrap(Self::as_tensor(t).layer_norm(weight, bias, eps))
    }

    fn softmax_rows(&self, t: &mut DeviceTensor) {
        Self::as_tensor_mut(t).softmax_rows();
    }

    fn scale(&self, t: &mut DeviceTensor, factor: f32) {
        Self::as_tensor_mut(t).scale(factor);
    }

    fn slice_columns(&self, t: &DeviceTensor, start: usize, end: usize) -> DeviceTensor {
        let src = Self::as_tensor(t);
        let width = end - start;
        let mut data = vec![0.0f32; src.rows * width];
        for r in 0..src.rows {
            let src_off = r * src.cols + start;
            let dst_off = r * width;
            data[dst_off..dst_off + width].copy_from_slice(&src.data[src_off..src_off + width]);
        }
        Self::wrap(Tensor::from_slice(&data, src.rows, width))
    }

    fn scatter_columns(&self, dst: &mut DeviceTensor, src: &DeviceTensor, col_offset: usize) {
        let src_t = Self::as_tensor(src);
        let dst_t = Self::as_tensor_mut(dst);
        for r in 0..src_t.rows {
            let src_off = r * src_t.cols;
            let dst_off = r * dst_t.cols + col_offset;
            dst_t.data[dst_off..dst_off + src_t.cols]
                .copy_from_slice(&src_t.data[src_off..src_off + src_t.cols]);
        }
    }

    fn zeros(&self, rows: usize, cols: usize) -> DeviceTensor {
        Self::wrap(Tensor::zeros(rows, cols))
    }

    fn apply_attention_mask(&self, scores: &mut DeviceTensor, mask: &[u32]) {
        let t = Self::as_tensor_mut(scores);
        let seq_len = t.cols;
        for i in 0..t.rows {
            for j in 0..seq_len {
                if mask[j] == 0 {
                    t.data[i * seq_len + j] = -10000.0;
                }
            }
        }
    }

    fn mean_pool(&self, hidden: &DeviceTensor, mask: &[u32]) -> Vec<f32> {
        let t = Self::as_tensor(hidden);
        let mut sum = vec![0.0f32; t.cols];
        let mut count = 0.0f32;
        for s in 0..t.rows {
            if mask[s] == 1 {
                let row = t.row(s);
                for i in 0..t.cols {
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

    fn name(&self) -> &'static str {
        "CPU"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::backend::ComputeBackend;

    fn backend() -> CpuBackend {
        CpuBackend
    }

    // -------------------------------------------------------------------
    // Issue 1: Tests for newly extracted CpuBackend operations
    // -------------------------------------------------------------------

    #[test]
    fn test_slice_columns() {
        let b = backend();
        // 2x4 matrix, slice columns 1..3 → 2x2
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 2, 4);
        let dt = b.upload(&t);
        let sliced = b.slice_columns(&dt, 1, 3);
        let result = b.download(&sliced);
        assert_eq!(result.rows, 2);
        assert_eq!(result.cols, 2);
        assert_eq!(result.data, vec![2.0, 3.0, 6.0, 7.0]);
    }

    #[test]
    fn test_slice_columns_full_width() {
        let b = backend();
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let dt = b.upload(&t);
        let sliced = b.slice_columns(&dt, 0, 3);
        let result = b.download(&sliced);
        assert_eq!(result.data, t.data);
    }

    #[test]
    fn test_slice_columns_single_column() {
        let b = backend();
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let dt = b.upload(&t);
        let sliced = b.slice_columns(&dt, 2, 3);
        let result = b.download(&sliced);
        assert_eq!(result.rows, 2);
        assert_eq!(result.cols, 1);
        assert_eq!(result.data, vec![3.0, 6.0]);
    }

    #[test]
    fn test_scatter_columns() {
        let b = backend();
        // Destination: 2x4 zeros
        let mut dst = b.zeros(2, 4);
        // Source: 2x2 values
        let src_t = Tensor::from_slice(&[10.0, 20.0, 30.0, 40.0], 2, 2);
        let src = b.upload(&src_t);
        // Scatter at column offset 1
        b.scatter_columns(&mut dst, &src, 1);
        let result = b.download(&dst);
        assert_eq!(result.data, vec![0.0, 10.0, 20.0, 0.0, 0.0, 30.0, 40.0, 0.0]);
    }

    #[test]
    fn test_scatter_columns_at_start() {
        let b = backend();
        let mut dst = b.zeros(2, 3);
        let src_t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let src = b.upload(&src_t);
        b.scatter_columns(&mut dst, &src, 0);
        let result = b.download(&dst);
        assert_eq!(result.data, vec![1.0, 2.0, 0.0, 3.0, 4.0, 0.0]);
    }

    #[test]
    fn test_slice_scatter_roundtrip() {
        let b = backend();
        // Create 2x6, slice cols 2..5, scatter back into a fresh 2x6 at offset 2
        let t = Tensor::from_slice(
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0,
              7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
            2, 6,
        );
        let dt = b.upload(&t);
        let sliced = b.slice_columns(&dt, 2, 5);
        let mut dst = b.zeros(2, 6);
        b.scatter_columns(&mut dst, &sliced, 2);
        let result = b.download(&dst);
        assert_eq!(
            result.data,
            vec![0.0, 0.0, 3.0, 4.0, 5.0, 0.0,
                 0.0, 0.0, 9.0, 10.0, 11.0, 0.0]
        );
    }

    #[test]
    fn test_apply_attention_mask_basic() {
        let b = backend();
        // 2x3 scores, mask=[1,0,1] → column 1 should become -10000
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let mut dt = b.upload(&t);
        b.apply_attention_mask(&mut dt, &[1, 0, 1]);
        let result = b.download(&dt);
        assert_eq!(result.data[0], 1.0);
        assert_eq!(result.data[1], -10000.0);
        assert_eq!(result.data[2], 3.0);
        assert_eq!(result.data[3], 4.0);
        assert_eq!(result.data[4], -10000.0);
        assert_eq!(result.data[5], 6.0);
    }

    #[test]
    fn test_apply_attention_mask_all_ones() {
        let b = backend();
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0], 1, 3);
        let mut dt = b.upload(&t);
        b.apply_attention_mask(&mut dt, &[1, 1, 1]);
        let result = b.download(&dt);
        assert_eq!(result.data, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_apply_attention_mask_all_zeros() {
        let b = backend();
        let t = Tensor::from_slice(&[1.0, 2.0], 1, 2);
        let mut dt = b.upload(&t);
        b.apply_attention_mask(&mut dt, &[0, 0]);
        let result = b.download(&dt);
        assert_eq!(result.data, vec![-10000.0, -10000.0]);
    }

    #[test]
    fn test_mean_pool_basic() {
        let b = backend();
        // 3 rows x 2 cols, mask=[1,1,0] → average of rows 0 and 1
        let t = Tensor::from_slice(&[2.0, 4.0, 6.0, 8.0, 100.0, 200.0], 3, 2);
        let dt = b.upload(&t);
        let result = b.mean_pool(&dt, &[1, 1, 0]);
        assert_eq!(result.len(), 2);
        assert!((result[0] - 4.0).abs() < 1e-6); // (2+6)/2
        assert!((result[1] - 6.0).abs() < 1e-6); // (4+8)/2
    }

    #[test]
    fn test_mean_pool_single_token() {
        let b = backend();
        let t = Tensor::from_slice(&[3.0, 7.0, 100.0, 200.0], 2, 2);
        let dt = b.upload(&t);
        let result = b.mean_pool(&dt, &[1, 0]);
        assert!((result[0] - 3.0).abs() < 1e-6);
        assert!((result[1] - 7.0).abs() < 1e-6);
    }

    #[test]
    fn test_mean_pool_all_masked() {
        let b = backend();
        let t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let dt = b.upload(&t);
        let result = b.mean_pool(&dt, &[0, 0]);
        // No tokens contribute → result is zeros (sum=0, count=0 guard)
        assert_eq!(result, vec![0.0, 0.0]);
    }

    // -------------------------------------------------------------------
    // Issue 2: Backend round-trip tests — verify upload → op → download
    //          matches the direct Tensor method for every operation.
    // -------------------------------------------------------------------

    #[test]
    fn test_roundtrip_matmul() {
        let b = backend();
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        let c = Tensor::from_slice(&[7.0, 8.0, 9.0, 10.0, 11.0, 12.0], 3, 2);
        let expected = a.matmul(&c);

        let da = b.upload(&a);
        let dc = b.upload(&c);
        let result = b.download(&b.matmul(&da, &dc));
        assert_eq!(result.data, expected.data);
    }

    #[test]
    fn test_roundtrip_matmul_transpose() {
        let b = backend();
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let c = Tensor::from_slice(&[5.0, 6.0, 7.0, 8.0], 2, 2);
        let expected = a.matmul_transpose(&c);

        let da = b.upload(&a);
        let dc = b.upload(&c);
        let result = b.download(&b.matmul_transpose(&da, &dc));
        assert_eq!(result.data, expected.data);
    }

    #[test]
    fn test_roundtrip_add_bias() {
        let b = backend();
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let bias = [10.0, 20.0];
        let mut dt = b.upload(&t);
        let dbias = b.upload_1d(&bias);
        b.add_bias(&mut dt, &dbias);
        let result = b.download(&dt);

        t.add_bias(&bias);
        assert_eq!(result.data, t.data);
    }

    #[test]
    fn test_roundtrip_add_tensor() {
        let b = backend();
        let a = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let c = Tensor::from_slice(&[10.0, 20.0, 30.0, 40.0], 2, 2);
        let expected = a.add_tensor(&c);

        let da = b.upload(&a);
        let dc = b.upload(&c);
        let result = b.download(&b.add_tensor(&da, &dc));
        assert_eq!(result.data, expected.data);
    }

    #[test]
    fn test_roundtrip_gelu() {
        let b = backend();
        let t = Tensor::from_slice(&[-2.0, -1.0, 0.0, 1.0, 2.0, 5.0], 2, 3);
        let expected = t.gelu();

        let dt = b.upload(&t);
        let result = b.download(&b.gelu(&dt));
        for (e, r) in expected.data.iter().zip(result.data.iter()) {
            assert!((e - r).abs() < 1e-6, "gelu mismatch: expected {}, got {}", e, r);
        }
    }

    #[test]
    fn test_roundtrip_layer_norm() {
        let b = backend();
        let t = Tensor::from_slice(&[1.0, 3.0, 2.0, 6.0], 2, 2);
        let w = vec![1.0, 1.0];
        let bias = vec![0.0, 0.0];
        let expected = t.layer_norm(&w, &bias, 1e-5);

        let dt = b.upload(&t);
        let dw = b.upload_1d(&w);
        let db = b.upload_1d(&bias);
        let result = b.download(&b.layer_norm(&dt, &dw, &db, 1e-5));
        for (e, r) in expected.data.iter().zip(result.data.iter()) {
            assert!((e - r).abs() < 1e-4, "layer_norm mismatch: expected {}, got {}", e, r);
        }
    }

    #[test]
    fn test_roundtrip_softmax_rows() {
        let b = backend();
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0, 10.0, 20.0, 30.0], 2, 3);
        let mut dt = b.upload(&t);
        b.softmax_rows(&mut dt);
        let result = b.download(&dt);

        t.softmax_rows();
        for (e, r) in t.data.iter().zip(result.data.iter()) {
            assert!((e - r).abs() < 1e-6, "softmax mismatch: expected {}, got {}", e, r);
        }
    }

    #[test]
    fn test_roundtrip_scale() {
        let b = backend();
        let mut t = Tensor::from_slice(&[1.0, 2.0, 3.0, 4.0], 2, 2);
        let mut dt = b.upload(&t);
        b.scale(&mut dt, 0.5);
        let result = b.download(&dt);

        t.scale(0.5);
        assert_eq!(result.data, t.data);
    }

    #[test]
    fn test_roundtrip_upload_download_identity() {
        let b = backend();
        let t = Tensor::from_slice(&[1.5, -2.5, 3.14, 0.0, f32::MAX, f32::MIN], 2, 3);
        let dt = b.upload(&t);
        let result = b.download(&dt);
        assert_eq!(result.rows, t.rows);
        assert_eq!(result.cols, t.cols);
        assert_eq!(result.data, t.data);
    }

    #[test]
    fn test_roundtrip_zeros() {
        let b = backend();
        let dt = b.zeros(3, 4);
        let result = b.download(&dt);
        assert_eq!(result.rows, 3);
        assert_eq!(result.cols, 4);
        assert!(result.data.iter().all(|&v| v == 0.0));
    }
}
