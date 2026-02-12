//! Compute backend trait for GPU-accelerated tensor operations.
//!
//! Provides a `ComputeBackend` trait that abstracts tensor operations across
//! CPU, CUDA, and Metal. Backend selection happens once at model load time.

use std::any::Any;
use std::sync::Arc;

use super::tensor::Tensor;

/// A tensor that lives on a compute device (CPU, CUDA, or Metal).
///
/// The inner representation is backend-specific and type-erased.
pub struct DeviceTensor {
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
    /// Backend-specific storage.
    pub(crate) inner: Box<dyn Any + Send + Sync>,
}

/// Trait for compute backends that execute tensor operations.
///
/// Implementations dispatch operations to CPU, CUDA, or Metal.
pub trait ComputeBackend: Send + Sync {
    /// Upload a CPU tensor to the device.
    fn upload(&self, t: &Tensor) -> DeviceTensor;

    /// Upload a 1D slice to the device as a single-row tensor.
    fn upload_1d(&self, v: &[f32]) -> DeviceTensor;

    /// Download a device tensor back to CPU.
    fn download(&self, dt: &DeviceTensor) -> Tensor;

    /// Matrix multiply: (M,K) x (K,N) -> (M,N).
    fn matmul(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor;

    /// Matrix multiply with transpose: (M,K) x (N,K)^T -> (M,N).
    fn matmul_transpose(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor;

    /// Broadcast row-add: add a 1-row bias to each row of t.
    fn add_bias(&self, t: &mut DeviceTensor, bias: &DeviceTensor);

    /// Element-wise add.
    fn add_tensor(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor;

    /// Fast GELU activation.
    fn gelu(&self, t: &DeviceTensor) -> DeviceTensor;

    /// Layer normalization per row.
    fn layer_norm(
        &self,
        t: &DeviceTensor,
        w: &DeviceTensor,
        b: &DeviceTensor,
        eps: f32,
    ) -> DeviceTensor;

    /// Per-row softmax in place.
    fn softmax_rows(&self, t: &mut DeviceTensor);

    /// Scalar multiply in place.
    fn scale(&self, t: &mut DeviceTensor, factor: f32);

    /// Extract a contiguous column slice from each row.
    fn slice_columns(&self, t: &DeviceTensor, start: usize, end: usize) -> DeviceTensor;

    /// Write src columns into dst starting at col_offset.
    fn scatter_columns(&self, dst: &mut DeviceTensor, src: &DeviceTensor, col_offset: usize);

    /// Create a zero tensor on the device.
    fn zeros(&self, rows: usize, cols: usize) -> DeviceTensor;

    /// Apply attention mask: set padding positions to -10000.
    fn apply_attention_mask(&self, scores: &mut DeviceTensor, mask: &[u32]);

    /// Mean pooling with attention mask, returns host vector.
    fn mean_pool(&self, hidden: &DeviceTensor, mask: &[u32]) -> Vec<f32>;

    /// Backend name for logging.
    fn name(&self) -> &'static str;
}

/// Select the best available compute backend.
///
/// Tries CUDA first, then Metal, falls back to CPU.
pub fn select_backend() -> Arc<dyn ComputeBackend> {
    #[cfg(feature = "embed-cuda")]
    {
        match super::cuda::CudaBackend::try_new() {
            Ok(backend) => {
                tracing::info!(target: "strata::embed", "Using CUDA compute backend");
                return Arc::new(backend);
            }
            Err(e) => {
                tracing::info!(target: "strata::embed", error = %e, "CUDA not available, trying next backend");
            }
        }
    }

    #[cfg(all(feature = "embed-metal", target_os = "macos"))]
    {
        match super::metal::MetalBackend::try_new() {
            Ok(backend) => {
                tracing::info!(target: "strata::embed", "Using Metal compute backend");
                return Arc::new(backend);
            }
            Err(e) => {
                tracing::info!(target: "strata::embed", error = %e, "Metal not available, falling back to CPU");
            }
        }
    }

    tracing::info!(target: "strata::embed", "Using CPU compute backend");
    Arc::new(super::cpu_backend::CpuBackend)
}
