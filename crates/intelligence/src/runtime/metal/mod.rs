//! Metal compute backend for macOS GPU acceleration.
//!
//! Provides `MetalBackend`, which implements `ComputeBackend` by dispatching
//! MSL compute kernels through the Metal framework via raw Objective-C FFI.
//! No external Objective-C or Metal crate dependencies are required.

use super::backend::{ComputeBackend, DeviceTensor};
use super::tensor::Tensor;
use ffi::*;

pub mod ffi;
pub mod kernels;

// ---------------------------------------------------------------------------
// MetalBuffer — a reference-counted MTLBuffer wrapper
// ---------------------------------------------------------------------------

/// A buffer allocated on the Metal device with `StorageModeShared`.
struct MetalBuffer {
    /// Raw `MTLBuffer` Objective-C object pointer.
    buffer: Id,
    /// Size in bytes — retained for debugging / future introspection.
    #[allow(dead_code)]
    len: usize,
}

// Metal shared-mode buffers can be read/written from any thread.
unsafe impl Send for MetalBuffer {}
unsafe impl Sync for MetalBuffer {}

impl Drop for MetalBuffer {
    fn drop(&mut self) {
        if self.buffer != NIL {
            unsafe {
                msg_send_void(self.buffer, sel_registerName(b"release\0".as_ptr() as _));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MetalBackend
// ---------------------------------------------------------------------------

/// Metal compute backend for macOS GPU acceleration.
///
/// Holds the Metal device, command queue, selector cache, and pre-compiled
/// pipeline state objects (PSOs) for every kernel in the MSL source.
///
/// Each operation creates a fresh command buffer, encodes work, commits, and
/// calls `waitUntilCompleted` — so all GPU work is serialised and results are
/// available immediately on return.
pub struct MetalBackend {
    device: Id,
    command_queue: Id,
    sels: Selectors,
    // Pipeline state objects — one per kernel function.
    pso_gemm: Id,
    pso_gemm_transpose: Id,
    pso_gelu: Id,
    pso_add_tensor: Id,
    pso_add_bias: Id,
    pso_scale: Id,
    pso_layer_norm: Id,
    pso_softmax_rows: Id,
    pso_slice_columns: Id,
    pso_scatter_columns: Id,
    pso_attention_mask: Id,
    pso_mean_pool: Id,
}

// We synchronize every dispatch with waitUntilCompleted, so the backend can
// safely be shared across threads.
unsafe impl Send for MetalBackend {}
unsafe impl Sync for MetalBackend {}

impl Drop for MetalBackend {
    fn drop(&mut self) {
        unsafe {
            let rel = sel_registerName(b"release\0".as_ptr() as _);
            // Release all PSOs
            for pso in [
                self.pso_gemm,
                self.pso_gemm_transpose,
                self.pso_gelu,
                self.pso_add_tensor,
                self.pso_add_bias,
                self.pso_scale,
                self.pso_layer_norm,
                self.pso_softmax_rows,
                self.pso_slice_columns,
                self.pso_scatter_columns,
                self.pso_attention_mask,
                self.pso_mean_pool,
            ] {
                if pso != NIL {
                    msg_send_void(pso, rel);
                }
            }
            if self.command_queue != NIL {
                msg_send_void(self.command_queue, rel);
            }
            if self.device != NIL {
                msg_send_void(self.device, rel);
            }
        }
    }
}

impl MetalBackend {
    /// Try to create a Metal compute backend.
    ///
    /// Returns `Err` if no Metal device is available or if the MSL kernels
    /// fail to compile.
    pub fn try_new() -> Result<Self, String> {
        unsafe {
            // 1. Get the default Metal device.
            let device = MTLCreateSystemDefaultDevice();
            if device == NIL {
                return Err("No Metal device available".into());
            }
            // Retain the device so our Drop can release it.
            msg_send_void(device, sel_registerName(b"retain\0".as_ptr() as _));

            // 2. Pre-register all selectors.
            let sels = Selectors::new();

            // 3. Create command queue.
            let command_queue = msg_send_id(device, sels.new_command_queue);
            if command_queue == NIL {
                msg_send_void(device, sels.release);
                return Err("Failed to create Metal command queue".into());
            }

            // 4. Compile the MSL library from source.
            let source = ns_string(kernels::MSL_SOURCE);
            let mut error: Id = NIL;
            let library = msg_send_id_id_id_id(
                device,
                sels.new_library_with_source,
                source,
                NIL, // default compile options
                &mut error as *mut Id as Id,
            );
            if library == NIL {
                let desc = obj_description(error);
                msg_send_void(command_queue, sels.release);
                msg_send_void(device, sels.release);
                return Err(format!("Metal MSL compile error: {}", desc));
            }

            // 5. Create pipeline state objects for each kernel.
            let kernel_names = [
                "gemm",
                "gemm_transpose",
                "gelu",
                "add_tensor",
                "add_bias",
                "scale_kernel",
                "layer_norm",
                "softmax_rows",
                "slice_columns",
                "scatter_columns",
                "attention_mask",
                "mean_pool",
            ];

            let mut psos = [NIL; 12];
            for (i, name) in kernel_names.iter().enumerate() {
                let ns_name = ns_string(name);
                let func = msg_send_id_id(library, sels.new_function_with_name, ns_name);
                if func == NIL {
                    // Cleanup already-created PSOs.
                    for p in &psos[..i] {
                        if *p != NIL {
                            msg_send_void(*p, sels.release);
                        }
                    }
                    msg_send_void(library, sels.release);
                    msg_send_void(command_queue, sels.release);
                    msg_send_void(device, sels.release);
                    return Err(format!("Metal kernel function '{}' not found", name));
                }

                let mut pso_error: Id = NIL;
                let pso = msg_send_id_id_err(
                    device,
                    sels.new_compute_pipeline,
                    func,
                    &mut pso_error,
                );
                msg_send_void(func, sels.release);

                if pso == NIL {
                    let desc = obj_description(pso_error);
                    for p in &psos[..i] {
                        if *p != NIL {
                            msg_send_void(*p, sels.release);
                        }
                    }
                    msg_send_void(library, sels.release);
                    msg_send_void(command_queue, sels.release);
                    msg_send_void(device, sels.release);
                    return Err(format!(
                        "Metal PSO creation failed for '{}': {}",
                        name, desc
                    ));
                }
                psos[i] = pso;
            }

            msg_send_void(library, sels.release);

            Ok(Self {
                device,
                command_queue,
                sels,
                pso_gemm: psos[0],
                pso_gemm_transpose: psos[1],
                pso_gelu: psos[2],
                pso_add_tensor: psos[3],
                pso_add_bias: psos[4],
                pso_scale: psos[5],
                pso_layer_norm: psos[6],
                pso_softmax_rows: psos[7],
                pso_slice_columns: psos[8],
                pso_scatter_columns: psos[9],
                pso_attention_mask: psos[10],
                pso_mean_pool: psos[11],
            })
        }
    }

    // -------------------------------------------------------------------
    // Buffer helpers
    // -------------------------------------------------------------------

    /// Create a Metal buffer from a byte slice.
    unsafe fn create_buffer(&self, data: &[u8]) -> Id {
        msg_send_new_buffer(
            self.device,
            self.sels.new_buffer_with_bytes,
            data.as_ptr(),
            data.len(),
            MTL_RESOURCE_STORAGE_MODE_SHARED,
        )
    }

    /// Create a Metal buffer from a `&[f32]` slice.
    unsafe fn create_buffer_f32(&self, data: &[f32]) -> Id {
        let bytes = std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            data.len() * std::mem::size_of::<f32>(),
        );
        self.create_buffer(bytes)
    }

    /// Create a Metal buffer from a `&[u32]` slice.
    unsafe fn create_buffer_u32(&self, data: &[u32]) -> Id {
        let bytes = std::slice::from_raw_parts(
            data.as_ptr() as *const u8,
            data.len() * std::mem::size_of::<u32>(),
        );
        self.create_buffer(bytes)
    }

    /// Create an uninitialised Metal buffer of `len` bytes.
    unsafe fn create_buffer_empty(&self, len: usize) -> Id {
        msg_send_new_buffer_length(
            self.device,
            self.sels.new_buffer_with_length,
            len,
            MTL_RESOURCE_STORAGE_MODE_SHARED,
        )
    }

    /// Read back the contents of a Metal buffer as a `Vec<f32>`.
    unsafe fn read_buffer_f32(&self, buffer: Id, count: usize) -> Vec<f32> {
        let ptr = msg_send_ptr(buffer, self.sels.contents) as *const f32;
        let slice = std::slice::from_raw_parts(ptr, count);
        slice.to_vec()
    }

    // -------------------------------------------------------------------
    // Extract the raw MTLBuffer Id from a DeviceTensor
    // -------------------------------------------------------------------

    fn metal_buffer(dt: &DeviceTensor) -> &MetalBuffer {
        dt.inner
            .downcast_ref::<MetalBuffer>()
            .expect("MetalBackend: expected MetalBuffer in DeviceTensor")
    }

    #[allow(dead_code)]
    fn metal_buffer_mut(dt: &mut DeviceTensor) -> &mut MetalBuffer {
        dt.inner
            .downcast_mut::<MetalBuffer>()
            .expect("MetalBackend: expected MetalBuffer in DeviceTensor")
    }

    fn buf_id(dt: &DeviceTensor) -> Id {
        Self::metal_buffer(dt).buffer
    }

    fn wrap(buffer: Id, len: usize, rows: usize, cols: usize) -> DeviceTensor {
        DeviceTensor {
            rows,
            cols,
            inner: Box::new(MetalBuffer { buffer, len }),
        }
    }

    // -------------------------------------------------------------------
    // Dispatch helpers
    // -------------------------------------------------------------------

    /// Create a command buffer, compute encoder, and set the pipeline.
    /// Returns `(command_buffer, encoder)`.
    unsafe fn begin_command(&self, pso: Id) -> (Id, Id) {
        let cmd = msg_send_id(self.command_queue, self.sels.command_buffer);
        let enc = msg_send_id(cmd, self.sels.compute_command_encoder);
        msg_send_void_id(enc, self.sels.set_compute_pipeline, pso);
        (cmd, enc)
    }

    /// End encoding, commit, and wait for completion.
    unsafe fn end_command(&self, cmd: Id, enc: Id) {
        msg_send_void(enc, self.sels.end_encoding);
        msg_send_void(cmd, self.sels.commit);
        msg_send_void(cmd, self.sels.wait_until_completed);
    }

    /// Bind a MTLBuffer at `index`.
    unsafe fn set_buffer(&self, enc: Id, buf: Id, index: usize) {
        msg_send_set_buffer(enc, self.sels.set_buffer, buf, 0, index);
    }

    /// Bind small constant data (e.g. a single u32 or f32) at `index`.
    unsafe fn set_bytes(&self, enc: Id, data: &[u8], index: usize) {
        msg_send_set_bytes(
            enc,
            self.sels.set_bytes,
            data.as_ptr(),
            data.len(),
            index,
        );
    }

    /// Bind a `u32` parameter at `index`.
    unsafe fn set_u32(&self, enc: Id, val: u32, index: usize) {
        self.set_bytes(enc, &val.to_ne_bytes(), index);
    }

    /// Bind a `f32` parameter at `index`.
    unsafe fn set_f32(&self, enc: Id, val: f32, index: usize) {
        self.set_bytes(enc, &val.to_ne_bytes(), index);
    }

    /// Dispatch a 1D grid: `ceil(n / threads)` threadgroups, each `threads` wide.
    unsafe fn dispatch_1d(&self, enc: Id, cmd: Id, n: usize) {
        let threads = 256usize;
        let groups = (n + threads - 1) / threads;
        msg_send_dispatch(
            enc,
            self.sels.dispatch_threadgroups,
            groups, 1, 1,   // threadgroups
            threads, 1, 1,  // threads per threadgroup
        );
        self.end_command(cmd, enc);
    }

    /// Dispatch a 2D grid: ceil(width/tx) x ceil(height/ty) threadgroups.
    unsafe fn dispatch_2d(
        &self,
        enc: Id,
        cmd: Id,
        width: usize,
        height: usize,
        tx: usize,
        ty: usize,
    ) {
        let gx = (width + tx - 1) / tx;
        let gy = (height + ty - 1) / ty;
        msg_send_dispatch(
            enc,
            self.sels.dispatch_threadgroups,
            gx, gy, 1,  // threadgroups
            tx, ty, 1,  // threads per threadgroup
        );
        self.end_command(cmd, enc);
    }

    /// Dispatch with one threadgroup per row, `threads_per_group` threads each.
    /// Used by reduction kernels (layer_norm, softmax_rows).
    unsafe fn dispatch_rows(
        &self,
        enc: Id,
        cmd: Id,
        num_rows: usize,
        threads_per_group: usize,
    ) {
        msg_send_dispatch(
            enc,
            self.sels.dispatch_threadgroups,
            num_rows, 1, 1,             // threadgroups
            threads_per_group, 1, 1,    // threads per threadgroup
        );
        self.end_command(cmd, enc);
    }
}

// ---------------------------------------------------------------------------
// ComputeBackend implementation
// ---------------------------------------------------------------------------

impl ComputeBackend for MetalBackend {
    fn upload(&self, t: &Tensor) -> DeviceTensor {
        let byte_len = t.data.len() * std::mem::size_of::<f32>();
        let buf = unsafe { self.create_buffer_f32(&t.data) };
        Self::wrap(buf, byte_len, t.rows, t.cols)
    }

    fn upload_1d(&self, v: &[f32]) -> DeviceTensor {
        let byte_len = v.len() * std::mem::size_of::<f32>();
        let buf = unsafe { self.create_buffer_f32(v) };
        Self::wrap(buf, byte_len, 1, v.len())
    }

    fn download(&self, dt: &DeviceTensor) -> Tensor {
        let count = dt.rows * dt.cols;
        let data = unsafe { self.read_buffer_f32(Self::buf_id(dt), count) };
        Tensor {
            data,
            rows: dt.rows,
            cols: dt.cols,
        }
    }

    fn matmul(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let m = a.rows;
        let k = a.cols;
        let n = b.cols;
        assert_eq!(k, b.rows, "matmul dimension mismatch");

        let out_count = m * n;
        let out_bytes = out_count * std::mem::size_of::<f32>();

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_gemm);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, m as u32, 3);
            self.set_u32(enc, k as u32, 4);
            self.set_u32(enc, n as u32, 5);

            self.dispatch_2d(enc, cmd, n, m, 16, 16);

            Self::wrap(out_buf, out_bytes, m, n)
        }
    }

    fn matmul_transpose(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let m = a.rows;
        let k = a.cols;
        let n = b.rows; // B is (N, K), transposed
        assert_eq!(k, b.cols, "matmul_transpose dimension mismatch");

        let out_count = m * n;
        let out_bytes = out_count * std::mem::size_of::<f32>();

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_gemm_transpose);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, m as u32, 3);
            self.set_u32(enc, k as u32, 4);
            self.set_u32(enc, n as u32, 5);

            self.dispatch_2d(enc, cmd, n, m, 16, 16);

            Self::wrap(out_buf, out_bytes, m, n)
        }
    }

    fn add_bias(&self, t: &mut DeviceTensor, bias: &DeviceTensor) {
        let rows = t.rows;
        let cols = t.cols;
        assert_eq!(bias.cols, cols, "add_bias: bias width mismatch");

        unsafe {
            let (cmd, enc) = self.begin_command(self.pso_add_bias);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, Self::buf_id(bias), 1);
            self.set_u32(enc, rows as u32, 2);
            self.set_u32(enc, cols as u32, 3);

            self.dispatch_2d(enc, cmd, cols, rows, 16, 16);
        }
    }

    fn add_tensor(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let count = a.rows * a.cols;
        let out_bytes = count * std::mem::size_of::<f32>();

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_add_tensor);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, count as u32, 3);

            self.dispatch_1d(enc, cmd, count);

            Self::wrap(out_buf, out_bytes, a.rows, a.cols)
        }
    }

    fn gelu(&self, t: &DeviceTensor) -> DeviceTensor {
        let count = t.rows * t.cols;
        let out_bytes = count * std::mem::size_of::<f32>();

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_gelu);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, out_buf, 1);
            self.set_u32(enc, count as u32, 2);

            self.dispatch_1d(enc, cmd, count);

            Self::wrap(out_buf, out_bytes, t.rows, t.cols)
        }
    }

    fn layer_norm(
        &self,
        t: &DeviceTensor,
        w: &DeviceTensor,
        b: &DeviceTensor,
        eps: f32,
    ) -> DeviceTensor {
        let rows = t.rows;
        let cols = t.cols;
        let out_bytes = rows * cols * std::mem::size_of::<f32>();

        // Choose threads per threadgroup: min(256, next_power_of_2(cols))
        let threads_per_group = (cols.next_power_of_two()).min(256);

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_layer_norm);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, Self::buf_id(w), 1);
            self.set_buffer(enc, Self::buf_id(b), 2);
            self.set_buffer(enc, out_buf, 3);
            self.set_u32(enc, rows as u32, 4);
            self.set_u32(enc, cols as u32, 5);
            self.set_f32(enc, eps, 6);

            self.dispatch_rows(enc, cmd, rows, threads_per_group);

            Self::wrap(out_buf, out_bytes, rows, cols)
        }
    }

    fn softmax_rows(&self, t: &mut DeviceTensor) {
        let rows = t.rows;
        let cols = t.cols;

        let threads_per_group = (cols.next_power_of_two()).min(256);

        unsafe {
            let (cmd, enc) = self.begin_command(self.pso_softmax_rows);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_u32(enc, rows as u32, 1);
            self.set_u32(enc, cols as u32, 2);

            self.dispatch_rows(enc, cmd, rows, threads_per_group);
        }
    }

    fn scale(&self, t: &mut DeviceTensor, factor: f32) {
        let count = t.rows * t.cols;

        unsafe {
            let (cmd, enc) = self.begin_command(self.pso_scale);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_f32(enc, factor, 1);
            self.set_u32(enc, count as u32, 2);

            self.dispatch_1d(enc, cmd, count);
        }
    }

    fn slice_columns(&self, t: &DeviceTensor, start: usize, end: usize) -> DeviceTensor {
        let rows = t.rows;
        let width = end - start;
        let out_bytes = rows * width * std::mem::size_of::<f32>();

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_slice_columns);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, out_buf, 1);
            self.set_u32(enc, t.cols as u32, 2);
            self.set_u32(enc, start as u32, 3);
            self.set_u32(enc, width as u32, 4);
            self.set_u32(enc, rows as u32, 5);

            self.dispatch_2d(enc, cmd, width, rows, 16, 16);

            Self::wrap(out_buf, out_bytes, rows, width)
        }
    }

    fn scatter_columns(&self, dst: &mut DeviceTensor, src: &DeviceTensor, col_offset: usize) {
        let rows = src.rows;

        unsafe {
            let (cmd, enc) = self.begin_command(self.pso_scatter_columns);

            self.set_buffer(enc, Self::buf_id(dst), 0);
            self.set_buffer(enc, Self::buf_id(src), 1);
            self.set_u32(enc, dst.cols as u32, 2);
            self.set_u32(enc, src.cols as u32, 3);
            self.set_u32(enc, col_offset as u32, 4);
            self.set_u32(enc, rows as u32, 5);

            self.dispatch_2d(enc, cmd, src.cols, rows, 16, 16);
        }
    }

    fn zeros(&self, rows: usize, cols: usize) -> DeviceTensor {
        let count = rows * cols;
        let byte_len = count * std::mem::size_of::<f32>();
        let data = vec![0.0f32; count];
        let buf = unsafe { self.create_buffer_f32(&data) };
        Self::wrap(buf, byte_len, rows, cols)
    }

    fn apply_attention_mask(&self, scores: &mut DeviceTensor, mask: &[u32]) {
        let rows = scores.rows;
        let cols = scores.cols;

        unsafe {
            let mask_buf = self.create_buffer_u32(mask);
            let (cmd, enc) = self.begin_command(self.pso_attention_mask);

            self.set_buffer(enc, Self::buf_id(scores), 0);
            self.set_buffer(enc, mask_buf, 1);
            self.set_u32(enc, rows as u32, 2);
            self.set_u32(enc, cols as u32, 3);

            self.dispatch_2d(enc, cmd, cols, rows, 16, 16);

            // Release the temporary mask buffer.
            msg_send_void(mask_buf, self.sels.release);
        }
    }

    fn mean_pool(&self, hidden: &DeviceTensor, mask: &[u32]) -> Vec<f32> {
        let rows = hidden.rows;
        let cols = hidden.cols;
        let out_bytes = cols * std::mem::size_of::<f32>();

        unsafe {
            let mask_buf = self.create_buffer_u32(mask);
            let out_buf = self.create_buffer_empty(out_bytes);
            let (cmd, enc) = self.begin_command(self.pso_mean_pool);

            self.set_buffer(enc, Self::buf_id(hidden), 0);
            self.set_buffer(enc, mask_buf, 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, rows as u32, 3);
            self.set_u32(enc, cols as u32, 4);

            self.dispatch_1d(enc, cmd, cols);

            let result = self.read_buffer_f32(out_buf, cols);

            // Release temporary buffers.
            msg_send_void(mask_buf, self.sels.release);
            msg_send_void(out_buf, self.sels.release);

            result
        }
    }

    fn name(&self) -> &'static str {
        "Metal"
    }
}
