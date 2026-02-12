//! CUDA compute backend for GPU-accelerated tensor operations.
//!
//! Loads the CUDA driver at runtime (no link-time dependency) and dispatches
//! all tensor operations to GPU kernels written in PTX. Falls back gracefully
//! if CUDA is not available — `CudaBackend::try_new()` returns `Err` and the
//! backend selector moves on to the next option.

use std::os::raw::c_void;
use std::sync::Arc;

use super::backend::{ComputeBackend, DeviceTensor};
use super::tensor::Tensor;
use ffi::{CUdeviceptr, CUfunction, CUmodule, CUstream, CudaApi};

pub mod ffi;
pub mod kernels;

// ---------------------------------------------------------------------------
// CudaBuffer — RAII wrapper for a device allocation
// ---------------------------------------------------------------------------

/// A buffer allocated on the CUDA device.
///
/// Automatically freed when dropped via `cuMemFree`.
struct CudaBuffer {
    ptr: CUdeviceptr,
    #[allow(dead_code)]
    len: usize, // in bytes — retained for debugging / future introspection
    api: Arc<CudaApi>,
}

impl Drop for CudaBuffer {
    fn drop(&mut self) {
        if self.ptr != 0 {
            if let Err(e) = self.api.mem_free(self.ptr) {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: failed to free device memory");
            }
        }
    }
}

// SAFETY: CudaBuffer holds a u64 device pointer and an Arc to thread-safe API.
// The device pointer is just an integer handle; it does not reference host memory.
unsafe impl Send for CudaBuffer {}
unsafe impl Sync for CudaBuffer {}

// ---------------------------------------------------------------------------
// CudaBackend
// ---------------------------------------------------------------------------

/// CUDA compute backend.
///
/// Manages a CUDA context, stream, loaded PTX module, and pre-resolved kernel
/// function handles. All tensor operations are dispatched as asynchronous kernel
/// launches on the backend's stream, with synchronization at download boundaries.
pub struct CudaBackend {
    api: Arc<CudaApi>,
    stream: CUstream,
    module: CUmodule,

    // Pre-loaded kernel function handles
    fn_gemm: CUfunction,
    fn_gemm_transpose: CUfunction,
    fn_gelu: CUfunction,
    fn_add_tensor: CUfunction,
    fn_add_bias: CUfunction,
    fn_scale: CUfunction,
    fn_layer_norm: CUfunction,
    fn_softmax_rows: CUfunction,
    fn_slice_columns: CUfunction,
    fn_scatter_columns: CUfunction,
    fn_attention_mask: CUfunction,
    fn_mean_pool: CUfunction,
}

// SAFETY: All CUDA function handles are process-global, and the Driver API is
// documented as thread-safe. The stream is used behind &self which Rust's
// borrow checker already serialises for &mut operations.
unsafe impl Send for CudaBackend {}
unsafe impl Sync for CudaBackend {}

impl CudaBackend {
    /// Attempt to create a new CUDA backend.
    ///
    /// This will:
    /// 1. Load the CUDA driver library and initialise it.
    /// 2. Create a context on device 0.
    /// 3. Create a compute stream.
    /// 4. Load the PTX module containing all kernels.
    /// 5. Resolve every kernel function handle.
    ///
    /// Returns `Err` if any step fails (no CUDA driver, no GPU, PTX load error, etc.).
    pub fn try_new() -> Result<Self, String> {
        let api = Arc::new(CudaApi::load()?);

        let stream = api.stream_create()?;

        // Load the PTX module. cuModuleLoadData expects a null-terminated string.
        let ptx = kernels::PTX_MODULE;
        let module = api.module_load_data(ptx.as_ptr() as *const c_void)?;

        // Resolve all kernel functions.
        macro_rules! get_fn {
            ($name:expr) => {{
                let cname = concat!($name, "\0");
                let cstr = unsafe { std::ffi::CStr::from_bytes_with_nul_unchecked(cname.as_bytes()) };
                api.module_get_function(module, cstr)?
            }};
        }

        let fn_gemm = get_fn!("gemm");
        let fn_gemm_transpose = get_fn!("gemm_transpose");
        let fn_gelu = get_fn!("gelu");
        let fn_add_tensor = get_fn!("add_tensor");
        let fn_add_bias = get_fn!("add_bias");
        let fn_scale = get_fn!("scale");
        let fn_layer_norm = get_fn!("layer_norm");
        let fn_softmax_rows = get_fn!("softmax_rows");
        let fn_slice_columns = get_fn!("slice_columns");
        let fn_scatter_columns = get_fn!("scatter_columns");
        let fn_attention_mask = get_fn!("attention_mask");
        let fn_mean_pool = get_fn!("mean_pool");

        Ok(Self {
            api,
            stream,
            module,
            fn_gemm,
            fn_gemm_transpose,
            fn_gelu,
            fn_add_tensor,
            fn_add_bias,
            fn_scale,
            fn_layer_norm,
            fn_softmax_rows,
            fn_slice_columns,
            fn_scatter_columns,
            fn_attention_mask,
            fn_mean_pool,
        })
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Allocate device memory and copy host f32 data to it.
    fn upload_f32(&self, data: &[f32]) -> Result<CudaBuffer, String> {
        let bytesize = data.len() * std::mem::size_of::<f32>();
        let ptr = self.api.mem_alloc(bytesize)?;
        self.api
            .memcpy_h_to_d(ptr, data.as_ptr() as *const c_void, bytesize)?;
        Ok(CudaBuffer {
            ptr,
            len: bytesize,
            api: Arc::clone(&self.api),
        })
    }

    /// Allocate zeroed device memory for `n` f32 elements.
    fn alloc_zeros_f32(&self, n: usize) -> Result<CudaBuffer, String> {
        let bytesize = n * std::mem::size_of::<f32>();
        let ptr = self.api.mem_alloc(bytesize)?;
        // cuMemsetD32 sets each 32-bit word; 0u32 corresponds to 0.0f32.
        self.api.memset_d32(ptr, 0, n)?;
        Ok(CudaBuffer {
            ptr,
            len: bytesize,
            api: Arc::clone(&self.api),
        })
    }

    /// Upload u32 data to the device.
    fn upload_u32(&self, data: &[u32]) -> Result<CudaBuffer, String> {
        let bytesize = data.len() * std::mem::size_of::<u32>();
        let ptr = self.api.mem_alloc(bytesize)?;
        self.api
            .memcpy_h_to_d(ptr, data.as_ptr() as *const c_void, bytesize)?;
        Ok(CudaBuffer {
            ptr,
            len: bytesize,
            api: Arc::clone(&self.api),
        })
    }

    /// Download `n` f32 elements from device to host.
    fn download_f32(&self, ptr: CUdeviceptr, n: usize) -> Result<Vec<f32>, String> {
        let mut host = vec![0.0f32; n];
        let bytesize = n * std::mem::size_of::<f32>();
        self.api
            .memcpy_d_to_h(host.as_mut_ptr() as *mut c_void, ptr, bytesize)?;
        Ok(host)
    }

    /// Extract the `CudaBuffer` from a `DeviceTensor`.
    fn as_buf(dt: &DeviceTensor) -> &CudaBuffer {
        dt.inner
            .downcast_ref::<CudaBuffer>()
            .expect("CudaBackend: expected CudaBuffer in DeviceTensor")
    }

    /// Wrap a `CudaBuffer` into a `DeviceTensor`.
    fn wrap(buf: CudaBuffer, rows: usize, cols: usize) -> DeviceTensor {
        DeviceTensor {
            rows,
            cols,
            inner: Box::new(buf),
        }
    }

    /// Synchronize the compute stream.
    fn sync(&self) {
        if let Err(e) = self.api.stream_synchronize(self.stream) {
            tracing::warn!(target: "strata::embed", error = %e, "CUDA: stream synchronize failed");
        }
    }

    /// Launch a kernel with the given grid/block configuration and parameters.
    ///
    /// # Safety
    ///
    /// `params` must be a correctly constructed parameter array matching the
    /// kernel signature.
    unsafe fn launch(
        &self,
        func: CUfunction,
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        shared_mem: u32,
        params: &mut [*mut c_void],
    ) {
        if let Err(e) = self.api.launch_kernel(
            func,
            grid,
            block,
            shared_mem,
            self.stream,
            params.as_mut_ptr(),
        ) {
            tracing::warn!(target: "strata::embed", error = %e, "CUDA: kernel launch failed");
        }
    }

    /// Integer ceiling division.
    fn div_ceil(a: u32, b: u32) -> u32 {
        (a + b - 1) / b
    }
}

// ---------------------------------------------------------------------------
// ComputeBackend implementation
// ---------------------------------------------------------------------------

impl ComputeBackend for CudaBackend {
    fn upload(&self, t: &Tensor) -> DeviceTensor {
        match self.upload_f32(&t.data) {
            Ok(buf) => Self::wrap(buf, t.rows, t.cols),
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: upload failed, falling back to zeros");
                Self::wrap(
                    self.alloc_zeros_f32(t.rows * t.cols)
                        .expect("CUDA: alloc_zeros_f32 failed after upload failure"),
                    t.rows,
                    t.cols,
                )
            }
        }
    }

    fn upload_1d(&self, v: &[f32]) -> DeviceTensor {
        match self.upload_f32(v) {
            Ok(buf) => Self::wrap(buf, 1, v.len()),
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: upload_1d failed");
                Self::wrap(
                    self.alloc_zeros_f32(v.len())
                        .expect("CUDA: alloc_zeros_f32 failed after upload_1d failure"),
                    1,
                    v.len(),
                )
            }
        }
    }

    fn download(&self, dt: &DeviceTensor) -> Tensor {
        self.sync();
        let buf = Self::as_buf(dt);
        let n = dt.rows * dt.cols;
        match self.download_f32(buf.ptr, n) {
            Ok(data) => Tensor {
                data,
                rows: dt.rows,
                cols: dt.cols,
            },
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: download failed, returning zeros");
                Tensor::zeros(dt.rows, dt.cols)
            }
        }
    }

    fn matmul(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let m = a.rows as u32;
        let k = a.cols as u32;
        let n = b.cols as u32;

        let out = match self.alloc_zeros_f32(a.rows * b.cols) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: matmul alloc failed, returning zeros");
                return self.zeros(a.rows, b.cols);
            }
        };

        let a_buf = Self::as_buf(a);
        let b_buf = Self::as_buf(b);

        let mut p_a = a_buf.ptr;
        let mut p_b = b_buf.ptr;
        let mut p_c = out.ptr;
        let mut p_m = m;
        let mut p_k = k;
        let mut p_n = n;

        let mut params: [*mut c_void; 6] = [
            &mut p_a as *mut _ as *mut c_void,
            &mut p_b as *mut _ as *mut c_void,
            &mut p_c as *mut _ as *mut c_void,
            &mut p_m as *mut _ as *mut c_void,
            &mut p_k as *mut _ as *mut c_void,
            &mut p_n as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(n, 16), Self::div_ceil(m, 16), 1);
        let block = (16, 16, 1);
        unsafe {
            self.launch(self.fn_gemm, grid, block, 0, &mut params);
        }

        Self::wrap(out, a.rows, b.cols)
    }

    fn matmul_transpose(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let m = a.rows as u32;
        let k = a.cols as u32;
        let n = b.rows as u32; // B is (N, K) and we treat it as transposed

        let out = match self.alloc_zeros_f32(a.rows * b.rows) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: matmul_transpose alloc failed, returning zeros");
                return self.zeros(a.rows, b.rows);
            }
        };

        let a_buf = Self::as_buf(a);
        let b_buf = Self::as_buf(b);

        let mut p_a = a_buf.ptr;
        let mut p_b = b_buf.ptr;
        let mut p_c = out.ptr;
        let mut p_m = m;
        let mut p_k = k;
        let mut p_n = n;

        let mut params: [*mut c_void; 6] = [
            &mut p_a as *mut _ as *mut c_void,
            &mut p_b as *mut _ as *mut c_void,
            &mut p_c as *mut _ as *mut c_void,
            &mut p_m as *mut _ as *mut c_void,
            &mut p_k as *mut _ as *mut c_void,
            &mut p_n as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(n, 16), Self::div_ceil(m, 16), 1);
        let block = (16, 16, 1);
        unsafe {
            self.launch(self.fn_gemm_transpose, grid, block, 0, &mut params);
        }

        Self::wrap(out, a.rows, b.rows)
    }

    fn add_bias(&self, t: &mut DeviceTensor, bias: &DeviceTensor) {
        let rows = t.rows as u32;
        let cols = t.cols as u32;

        let t_buf = Self::as_buf(t);
        let bias_buf = Self::as_buf(bias);

        let mut p_t = t_buf.ptr;
        let mut p_bias = bias_buf.ptr;
        let mut p_rows = rows;
        let mut p_cols = cols;

        let mut params: [*mut c_void; 4] = [
            &mut p_t as *mut _ as *mut c_void,
            &mut p_bias as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_cols as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(cols, 256), rows, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_add_bias, grid, block, 0, &mut params);
        }
    }

    fn add_tensor(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let n = (a.rows * a.cols) as u32;

        let out = match self.alloc_zeros_f32(a.rows * a.cols) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: add_tensor alloc failed, returning zeros");
                return self.zeros(a.rows, a.cols);
            }
        };

        let a_buf = Self::as_buf(a);
        let b_buf = Self::as_buf(b);

        let mut p_a = a_buf.ptr;
        let mut p_b = b_buf.ptr;
        let mut p_c = out.ptr;
        let mut p_n = n;

        let mut params: [*mut c_void; 4] = [
            &mut p_a as *mut _ as *mut c_void,
            &mut p_b as *mut _ as *mut c_void,
            &mut p_c as *mut _ as *mut c_void,
            &mut p_n as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(n, 256), 1, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_add_tensor, grid, block, 0, &mut params);
        }

        Self::wrap(out, a.rows, a.cols)
    }

    fn gelu(&self, t: &DeviceTensor) -> DeviceTensor {
        let n = (t.rows * t.cols) as u32;

        let out = match self.alloc_zeros_f32(t.rows * t.cols) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: gelu alloc failed, returning zeros");
                return self.zeros(t.rows, t.cols);
            }
        };

        let t_buf = Self::as_buf(t);

        let mut p_in = t_buf.ptr;
        let mut p_out = out.ptr;
        let mut p_n = n;

        let mut params: [*mut c_void; 3] = [
            &mut p_in as *mut _ as *mut c_void,
            &mut p_out as *mut _ as *mut c_void,
            &mut p_n as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(n, 256), 1, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_gelu, grid, block, 0, &mut params);
        }

        Self::wrap(out, t.rows, t.cols)
    }

    fn layer_norm(
        &self,
        t: &DeviceTensor,
        w: &DeviceTensor,
        b: &DeviceTensor,
        eps: f32,
    ) -> DeviceTensor {
        let rows = t.rows as u32;
        let cols = t.cols as u32;

        let out = match self.alloc_zeros_f32(t.rows * t.cols) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: layer_norm alloc failed, returning zeros");
                return self.zeros(t.rows, t.cols);
            }
        };

        let t_buf = Self::as_buf(t);
        let w_buf = Self::as_buf(w);
        let b_buf = Self::as_buf(b);

        let mut p_in = t_buf.ptr;
        let mut p_out = out.ptr;
        let mut p_w = w_buf.ptr;
        let mut p_b = b_buf.ptr;
        let mut p_rows = rows;
        let mut p_cols = cols;
        let mut p_eps = eps;

        let mut params: [*mut c_void; 7] = [
            &mut p_in as *mut _ as *mut c_void,
            &mut p_out as *mut _ as *mut c_void,
            &mut p_w as *mut _ as *mut c_void,
            &mut p_b as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_cols as *mut _ as *mut c_void,
            &mut p_eps as *mut _ as *mut c_void,
        ];

        let grid = (1, rows, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_layer_norm, grid, block, 0, &mut params);
        }

        Self::wrap(out, t.rows, t.cols)
    }

    fn softmax_rows(&self, t: &mut DeviceTensor) {
        let rows = t.rows as u32;
        let cols = t.cols as u32;

        let t_buf = Self::as_buf(t);

        let mut p_data = t_buf.ptr;
        let mut p_rows = rows;
        let mut p_cols = cols;

        let mut params: [*mut c_void; 3] = [
            &mut p_data as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_cols as *mut _ as *mut c_void,
        ];

        let grid = (1, rows, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_softmax_rows, grid, block, 0, &mut params);
        }
    }

    fn scale(&self, t: &mut DeviceTensor, factor: f32) {
        let n = (t.rows * t.cols) as u32;

        let t_buf = Self::as_buf(t);

        let mut p_t = t_buf.ptr;
        let mut p_factor = factor;
        let mut p_n = n;

        let mut params: [*mut c_void; 3] = [
            &mut p_t as *mut _ as *mut c_void,
            &mut p_factor as *mut _ as *mut c_void,
            &mut p_n as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(n, 256), 1, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_scale, grid, block, 0, &mut params);
        }
    }

    fn slice_columns(&self, t: &DeviceTensor, start: usize, end: usize) -> DeviceTensor {
        let rows = t.rows as u32;
        let src_cols = t.cols as u32;
        let dst_cols = (end - start) as u32;
        let col_start = start as u32;

        let out = match self.alloc_zeros_f32(t.rows * (end - start)) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: slice_columns alloc failed, returning zeros");
                return self.zeros(t.rows, end - start);
            }
        };

        let t_buf = Self::as_buf(t);

        let mut p_src = t_buf.ptr;
        let mut p_dst = out.ptr;
        let mut p_rows = rows;
        let mut p_src_cols = src_cols;
        let mut p_dst_cols = dst_cols;
        let mut p_col_start = col_start;

        let mut params: [*mut c_void; 6] = [
            &mut p_src as *mut _ as *mut c_void,
            &mut p_dst as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_src_cols as *mut _ as *mut c_void,
            &mut p_dst_cols as *mut _ as *mut c_void,
            &mut p_col_start as *mut _ as *mut c_void,
        ];

        let grid = (
            Self::div_ceil(dst_cols, 16),
            Self::div_ceil(rows, 16),
            1,
        );
        let block = (16, 16, 1);
        unsafe {
            self.launch(self.fn_slice_columns, grid, block, 0, &mut params);
        }

        Self::wrap(out, t.rows, end - start)
    }

    fn scatter_columns(&self, dst: &mut DeviceTensor, src: &DeviceTensor, col_offset: usize) {
        let rows = src.rows as u32;
        let dst_cols = dst.cols as u32;
        let src_cols = src.cols as u32;
        let col_off = col_offset as u32;

        let dst_buf = Self::as_buf(dst);
        let src_buf = Self::as_buf(src);

        let mut p_dst = dst_buf.ptr;
        let mut p_src = src_buf.ptr;
        let mut p_rows = rows;
        let mut p_dst_cols = dst_cols;
        let mut p_src_cols = src_cols;
        let mut p_col_offset = col_off;

        let mut params: [*mut c_void; 6] = [
            &mut p_dst as *mut _ as *mut c_void,
            &mut p_src as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_dst_cols as *mut _ as *mut c_void,
            &mut p_src_cols as *mut _ as *mut c_void,
            &mut p_col_offset as *mut _ as *mut c_void,
        ];

        let grid = (
            Self::div_ceil(src_cols, 16),
            Self::div_ceil(rows, 16),
            1,
        );
        let block = (16, 16, 1);
        unsafe {
            self.launch(self.fn_scatter_columns, grid, block, 0, &mut params);
        }
    }

    fn zeros(&self, rows: usize, cols: usize) -> DeviceTensor {
        match self.alloc_zeros_f32(rows * cols) {
            Ok(buf) => Self::wrap(buf, rows, cols),
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: zeros alloc failed");
                // This is a critical path; panic is acceptable here.
                panic!("CUDA: failed to allocate zero tensor: {}", e);
            }
        }
    }

    fn apply_attention_mask(&self, scores: &mut DeviceTensor, mask: &[u32]) {
        let rows = scores.rows as u32;
        let cols = scores.cols as u32;

        // Upload mask to device
        let mask_buf = match self.upload_u32(mask) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: attention_mask upload failed");
                return;
            }
        };

        let scores_buf = Self::as_buf(scores);

        let mut p_scores = scores_buf.ptr;
        let mut p_mask = mask_buf.ptr;
        let mut p_rows = rows;
        let mut p_cols = cols;

        let mut params: [*mut c_void; 4] = [
            &mut p_scores as *mut _ as *mut c_void,
            &mut p_mask as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_cols as *mut _ as *mut c_void,
        ];

        let grid = (Self::div_ceil(cols, 256), rows, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_attention_mask, grid, block, 0, &mut params);
        }
        // mask_buf is dropped here, freeing device memory
    }

    fn mean_pool(&self, hidden: &DeviceTensor, mask: &[u32]) -> Vec<f32> {
        let rows = hidden.rows as u32;
        let cols = hidden.cols as u32;

        // Upload mask
        let mask_buf = match self.upload_u32(mask) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: mean_pool mask upload failed");
                return vec![0.0f32; hidden.cols];
            }
        };

        // Allocate output on device (1 row)
        let out_buf = match self.alloc_zeros_f32(hidden.cols) {
            Ok(buf) => buf,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: mean_pool output alloc failed");
                return vec![0.0f32; hidden.cols];
            }
        };

        let hidden_buf = Self::as_buf(hidden);

        let mut p_hidden = hidden_buf.ptr;
        let mut p_mask = mask_buf.ptr;
        let mut p_output = out_buf.ptr;
        let mut p_rows = rows;
        let mut p_cols = cols;

        let mut params: [*mut c_void; 5] = [
            &mut p_hidden as *mut _ as *mut c_void,
            &mut p_mask as *mut _ as *mut c_void,
            &mut p_output as *mut _ as *mut c_void,
            &mut p_rows as *mut _ as *mut c_void,
            &mut p_cols as *mut _ as *mut c_void,
        ];

        let grid = (1, 1, 1);
        let block = (256, 1, 1);
        unsafe {
            self.launch(self.fn_mean_pool, grid, block, 0, &mut params);
        }

        // Synchronize and download
        self.sync();
        match self.download_f32(out_buf.ptr, hidden.cols) {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!(target: "strata::embed", error = %e, "CUDA: mean_pool download failed");
                vec![0.0f32; hidden.cols]
            }
        }
    }

    fn name(&self) -> &'static str {
        "CUDA"
    }
}

impl Drop for CudaBackend {
    fn drop(&mut self) {
        // Synchronize before cleanup to ensure all work is complete.
        let _ = self.api.stream_synchronize(self.stream);

        if let Err(e) = self.api.module_unload(self.module) {
            tracing::warn!(target: "strata::embed", error = %e, "CUDA: failed to unload module");
        }
        if let Err(e) = self.api.stream_destroy(self.stream) {
            tracing::warn!(target: "strata::embed", error = %e, "CUDA: failed to destroy stream");
        }
        // Context destruction is handled by CudaApi::drop (which destroys self.api.ctx).
        // We do NOT destroy the context here because the Arc<CudaApi> may still be
        // held by CudaBuffer instances that need to call cuMemFree.
    }
}
