//! Metal compute backend for macOS GPU acceleration.
//!
//! Provides `MetalBackend`, which implements `ComputeBackend` by dispatching
//! MSL compute kernels through the Metal framework via raw Objective-C FFI.
//! No external Objective-C or Metal crate dependencies are required.

use std::sync::Mutex;

use super::backend::{ComputeBackend, DeviceTensor};
use super::tensor::Tensor;
use ffi::*;

pub mod ffi;
pub mod kernels;

/// `MTLBarrierScopeBuffers` — ensures all buffer writes from prior dispatches
/// are visible to subsequent dispatches within the same encoder.
const MTL_BARRIER_SCOPE_BUFFERS: NSUInteger = 1;

// ---------------------------------------------------------------------------
// IEEE 754 f32 ↔ f16 conversion
// ---------------------------------------------------------------------------

fn f32_to_f16(f: f32) -> u16 {
    let bits = f.to_bits();
    let sign = (bits >> 16) & 0x8000;
    let f32_exp = (bits >> 23) & 0xFF;
    let mantissa = bits & 0x7FFFFF;

    // NaN: preserve as f16 NaN (exponent=31, mantissa!=0)
    if f32_exp == 0xFF && mantissa != 0 {
        return (sign | 0x7E00) as u16; // quiet NaN
    }

    let exp = f32_exp as i32 - 127 + 15;
    if exp <= 0 {
        return sign as u16;
    } // underflow → ±0
    if exp >= 31 {
        return (sign | 0x7C00) as u16;
    } // overflow → ±inf

    // Round-to-nearest-even: check bit 12 (first dropped bit)
    let truncated = (mantissa >> 13) as u32;
    let round_bit = (mantissa >> 12) & 1;
    let sticky = mantissa & 0xFFF;
    let rounded = if round_bit != 0 && (sticky != 0 || (truncated & 1) != 0) {
        truncated + 1
    } else {
        truncated
    };
    // Handle mantissa overflow from rounding (e.g. 0x3FF + 1 = 0x400)
    if rounded >= 0x400 {
        let new_exp = exp + 1;
        if new_exp >= 31 {
            return (sign | 0x7C00) as u16; // overflow to ±inf
        }
        return (sign | ((new_exp as u32) << 10)) as u16; // mantissa becomes 0
    }
    (sign | ((exp as u32) << 10) | rounded) as u16
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1F) as u32;
    let mantissa = (h & 0x3FF) as u32;
    if exp == 0 {
        return f32::from_bits(sign << 31);
    } // ±0
    if exp == 31 {
        return f32::from_bits((sign << 31) | 0x7F800000 | (mantissa << 13));
    }
    f32::from_bits((sign << 31) | ((exp + 127 - 15) << 23) | (mantissa << 13))
}

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
/// Uses deferred command buffer batching: multiple GPU dispatches are encoded
/// into a single command buffer separated by memory barriers. The buffer is
/// only committed (`flush()`) when the CPU needs to read results (download,
/// mean_pool, batched_mean_pool). This reduces ~37 `waitUntilCompleted`
/// stalls to ~1-2 per embedding.
pub struct MetalBackend {
    device: Id,
    command_queue: Id,
    sels: Selectors,
    // Pipeline state objects — f32 kernels (kept for reference/fallback).
    #[allow(dead_code)]
    pso_gemm: Id,
    #[allow(dead_code)]
    pso_gemm_transpose: Id,
    #[allow(dead_code)]
    pso_gelu: Id,
    #[allow(dead_code)]
    pso_add_tensor: Id,
    #[allow(dead_code)]
    pso_add_bias: Id,
    #[allow(dead_code)]
    pso_scale: Id,
    #[allow(dead_code)]
    pso_layer_norm: Id,
    #[allow(dead_code)]
    pso_softmax_rows: Id,
    #[allow(dead_code)]
    pso_slice_columns: Id,
    #[allow(dead_code)]
    pso_scatter_columns: Id,
    #[allow(dead_code)]
    pso_attention_mask: Id,
    #[allow(dead_code)]
    pso_mean_pool: Id,
    #[allow(dead_code)]
    pso_batched_gemm_transpose: Id,
    #[allow(dead_code)]
    pso_batched_gemm: Id,
    #[allow(dead_code)]
    pso_batched_attention_mask: Id,
    #[allow(dead_code)]
    pso_batched_mean_pool: Id,
    #[allow(dead_code)]
    pso_transpose_heads: Id,
    #[allow(dead_code)]
    pso_untranspose_heads: Id,
    #[allow(dead_code)]
    pso_multi_head_batched_attention_mask: Id,
    // F16 pipeline state objects — used for all dispatches.
    pso_gemm_f16: Id,
    pso_gemm_transpose_f16: Id,
    pso_gelu_f16: Id,
    pso_add_tensor_f16: Id,
    pso_add_bias_f16: Id,
    pso_scale_f16: Id,
    pso_layer_norm_f16: Id,
    pso_softmax_rows_f16: Id,
    pso_slice_columns_f16: Id,
    pso_scatter_columns_f16: Id,
    pso_attention_mask_f16: Id,
    pso_mean_pool_f16: Id,
    pso_batched_gemm_transpose_f16: Id,
    pso_batched_gemm_f16: Id,
    pso_batched_attention_mask_f16: Id,
    pso_batched_mean_pool_f16: Id,
    pso_transpose_heads_f16: Id,
    pso_untranspose_heads_f16: Id,
    pso_multi_head_batched_attention_mask_f16: Id,
    /// Active command buffer and compute encoder for deferred dispatch.
    /// `None` means no open command buffer; dispatches create one lazily.
    active_cmd: Mutex<Option<(Id, Id)>>,
}

// We flush (waitUntilCompleted) before any CPU read, so the backend can
// safely be shared across threads.
unsafe impl Send for MetalBackend {}
unsafe impl Sync for MetalBackend {}

impl Drop for MetalBackend {
    fn drop(&mut self) {
        unsafe {
            // Flush any pending GPU work.
            self.flush();

            let rel = sel_registerName(b"release\0".as_ptr() as _);
            // Release all PSOs (f32 and f16)
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
                self.pso_batched_gemm_transpose,
                self.pso_batched_gemm,
                self.pso_batched_attention_mask,
                self.pso_batched_mean_pool,
                self.pso_transpose_heads,
                self.pso_untranspose_heads,
                self.pso_multi_head_batched_attention_mask,
                self.pso_gemm_f16,
                self.pso_gemm_transpose_f16,
                self.pso_gelu_f16,
                self.pso_add_tensor_f16,
                self.pso_add_bias_f16,
                self.pso_scale_f16,
                self.pso_layer_norm_f16,
                self.pso_softmax_rows_f16,
                self.pso_slice_columns_f16,
                self.pso_scatter_columns_f16,
                self.pso_attention_mask_f16,
                self.pso_mean_pool_f16,
                self.pso_batched_gemm_transpose_f16,
                self.pso_batched_gemm_f16,
                self.pso_batched_attention_mask_f16,
                self.pso_batched_mean_pool_f16,
                self.pso_transpose_heads_f16,
                self.pso_untranspose_heads_f16,
                self.pso_multi_head_batched_attention_mask_f16,
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
                "batched_gemm_transpose",
                "batched_gemm",
                "batched_attention_mask",
                "batched_mean_pool",
                "transpose_heads",
                "untranspose_heads",
                "multi_head_batched_attention_mask",
            ];

            let mut psos = [NIL; 19];
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

            // 6. Compile the F16 MSL library from source.
            let source_f16 = ns_string(kernels::MSL_SOURCE_F16);
            let mut error_f16: Id = NIL;
            let library_f16 = msg_send_id_id_id_id(
                device,
                sels.new_library_with_source,
                source_f16,
                NIL,
                &mut error_f16 as *mut Id as Id,
            );
            if library_f16 == NIL {
                let desc = obj_description(error_f16);
                // Cleanup f32 PSOs
                for p in &psos {
                    if *p != NIL {
                        msg_send_void(*p, sels.release);
                    }
                }
                msg_send_void(command_queue, sels.release);
                msg_send_void(device, sels.release);
                return Err(format!("Metal F16 MSL compile error: {}", desc));
            }

            // 7. Create F16 pipeline state objects.
            let f16_kernel_names = [
                "gemm_f16",
                "gemm_transpose_f16",
                "gelu_f16",
                "add_tensor_f16",
                "add_bias_f16",
                "scale_kernel_f16",
                "layer_norm_f16",
                "softmax_rows_f16",
                "slice_columns_f16",
                "scatter_columns_f16",
                "attention_mask_f16",
                "mean_pool_f16",
                "batched_gemm_transpose_f16",
                "batched_gemm_f16",
                "batched_attention_mask_f16",
                "batched_mean_pool_f16",
                "transpose_heads_f16",
                "untranspose_heads_f16",
                "multi_head_batched_attention_mask_f16",
            ];

            let mut psos_f16 = [NIL; 19];
            for (i, name) in f16_kernel_names.iter().enumerate() {
                let ns_name = ns_string(name);
                let func = msg_send_id_id(library_f16, sels.new_function_with_name, ns_name);
                if func == NIL {
                    for p in &psos_f16[..i] {
                        if *p != NIL {
                            msg_send_void(*p, sels.release);
                        }
                    }
                    for p in &psos {
                        if *p != NIL {
                            msg_send_void(*p, sels.release);
                        }
                    }
                    msg_send_void(library_f16, sels.release);
                    msg_send_void(command_queue, sels.release);
                    msg_send_void(device, sels.release);
                    return Err(format!("Metal F16 kernel function '{}' not found", name));
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
                    for p in &psos_f16[..i] {
                        if *p != NIL {
                            msg_send_void(*p, sels.release);
                        }
                    }
                    for p in &psos {
                        if *p != NIL {
                            msg_send_void(*p, sels.release);
                        }
                    }
                    msg_send_void(library_f16, sels.release);
                    msg_send_void(command_queue, sels.release);
                    msg_send_void(device, sels.release);
                    return Err(format!(
                        "Metal F16 PSO creation failed for '{}': {}",
                        name, desc
                    ));
                }
                psos_f16[i] = pso;
            }

            msg_send_void(library_f16, sels.release);

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
                pso_batched_gemm_transpose: psos[12],
                pso_batched_gemm: psos[13],
                pso_batched_attention_mask: psos[14],
                pso_batched_mean_pool: psos[15],
                pso_transpose_heads: psos[16],
                pso_untranspose_heads: psos[17],
                pso_multi_head_batched_attention_mask: psos[18],
                pso_gemm_f16: psos_f16[0],
                pso_gemm_transpose_f16: psos_f16[1],
                pso_gelu_f16: psos_f16[2],
                pso_add_tensor_f16: psos_f16[3],
                pso_add_bias_f16: psos_f16[4],
                pso_scale_f16: psos_f16[5],
                pso_layer_norm_f16: psos_f16[6],
                pso_softmax_rows_f16: psos_f16[7],
                pso_slice_columns_f16: psos_f16[8],
                pso_scatter_columns_f16: psos_f16[9],
                pso_attention_mask_f16: psos_f16[10],
                pso_mean_pool_f16: psos_f16[11],
                pso_batched_gemm_transpose_f16: psos_f16[12],
                pso_batched_gemm_f16: psos_f16[13],
                pso_batched_attention_mask_f16: psos_f16[14],
                pso_batched_mean_pool_f16: psos_f16[15],
                pso_transpose_heads_f16: psos_f16[16],
                pso_untranspose_heads_f16: psos_f16[17],
                pso_multi_head_batched_attention_mask_f16: psos_f16[18],
                active_cmd: Mutex::new(None),
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

    /// Create a Metal buffer containing f16 data converted from `&[f32]`.
    unsafe fn create_buffer_f16_from_f32(&self, data: &[f32]) -> Id {
        let f16_data: Vec<u16> = data.iter().map(|&f| f32_to_f16(f)).collect();
        let bytes = std::slice::from_raw_parts(
            f16_data.as_ptr() as *const u8,
            f16_data.len() * 2,
        );
        self.create_buffer(bytes)
    }

    /// Read back f16 buffer contents as `Vec<f32>`.
    unsafe fn read_buffer_f16_as_f32(&self, buffer: Id, count: usize) -> Vec<f32> {
        let ptr = msg_send_ptr(buffer, self.sels.contents) as *const u16;
        let slice = std::slice::from_raw_parts(ptr, count);
        slice.iter().map(|&h| f16_to_f32(h)).collect()
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
    // Deferred command buffer batching
    // -------------------------------------------------------------------

    /// Get or create the active command buffer and compute encoder, then set
    /// the pipeline state. Inserts a memory barrier before the dispatch so
    /// prior buffer writes are visible (cheap GPU-side fence, no CPU stall).
    ///
    /// Returns the encoder to bind buffers/bytes and dispatch on.
    unsafe fn ensure_encoder(&self, pso: Id) -> Id {
        let mut guard = self.active_cmd.lock().unwrap();
        let (_, enc) = guard.get_or_insert_with(|| {
            let cmd = msg_send_id(self.command_queue, self.sels.command_buffer);
            assert!(!cmd.is_null(), "command_buffer returned nil");
            let enc = msg_send_id(cmd, self.sels.compute_command_encoder);
            assert!(!enc.is_null(), "compute_command_encoder returned nil");
            (cmd, enc)
        });
        let enc = *enc;
        // Memory barrier: ensure all buffer writes from prior dispatches are
        // visible before this dispatch reads them.
        msg_send_void_nsuint(enc, self.sels.memory_barrier_with_scope, MTL_BARRIER_SCOPE_BUFFERS);
        // Set the pipeline for this dispatch.
        msg_send_void_id(enc, self.sels.set_compute_pipeline, pso);
        enc
    }

    /// Commit the active command buffer, wait for completion, and clear.
    /// This is the only place `waitUntilCompleted` is called.
    /// No-op if no command buffer is open.
    unsafe fn flush(&self) {
        let mut guard = self.active_cmd.lock().unwrap();
        if let Some((cmd, enc)) = guard.take() {
            msg_send_void(enc, self.sels.end_encoding);
            msg_send_void(cmd, self.sels.commit);
            msg_send_void(cmd, self.sels.wait_until_completed);
        }
    }

    // -------------------------------------------------------------------
    // Bind helpers
    // -------------------------------------------------------------------

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

    // -------------------------------------------------------------------
    // Dispatch helpers (deferred — no waitUntilCompleted)
    // -------------------------------------------------------------------

    /// Dispatch a 1D grid: `ceil(n / threads)` threadgroups, each `threads` wide.
    unsafe fn dispatch_1d(&self, enc: Id, n: usize) {
        let threads = 256usize;
        let groups = (n + threads - 1) / threads;
        msg_send_dispatch(
            enc,
            self.sels.dispatch_threadgroups,
            groups, 1, 1,   // threadgroups
            threads, 1, 1,  // threads per threadgroup
        );
    }

    /// Dispatch a 2D grid: ceil(width/tx) x ceil(height/ty) threadgroups.
    unsafe fn dispatch_2d(
        &self,
        enc: Id,
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
    }

    /// Dispatch with one threadgroup per row, `threads_per_group` threads each.
    /// Used by reduction kernels (layer_norm, softmax_rows).
    unsafe fn dispatch_rows(
        &self,
        enc: Id,
        num_rows: usize,
        threads_per_group: usize,
    ) {
        msg_send_dispatch(
            enc,
            self.sels.dispatch_threadgroups,
            num_rows, 1, 1,             // threadgroups
            threads_per_group, 1, 1,    // threads per threadgroup
        );
    }
}

// ---------------------------------------------------------------------------
// ComputeBackend implementation
// ---------------------------------------------------------------------------

impl ComputeBackend for MetalBackend {
    fn upload(&self, t: &Tensor) -> DeviceTensor {
        let byte_len = t.data.len() * 2; // f16 = 2 bytes
        let buf = unsafe { self.create_buffer_f16_from_f32(&t.data) };
        Self::wrap(buf, byte_len, t.rows, t.cols)
    }

    fn upload_1d(&self, v: &[f32]) -> DeviceTensor {
        let byte_len = v.len() * 2; // f16 = 2 bytes
        let buf = unsafe { self.create_buffer_f16_from_f32(v) };
        Self::wrap(buf, byte_len, 1, v.len())
    }

    fn download(&self, dt: &DeviceTensor) -> Tensor {
        unsafe { self.flush() };
        let count = dt.rows * dt.cols;
        let data = unsafe { self.read_buffer_f16_as_f32(Self::buf_id(dt), count) };
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
        let out_bytes = out_count * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_gemm_f16);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, m as u32, 3);
            self.set_u32(enc, k as u32, 4);
            self.set_u32(enc, n as u32, 5);

            // simdgroup_matrix: 32x32 tiles, 128 threads (4 simdgroups)
            let gx = (n + 31) / 32;
            let gy = (m + 31) / 32;
            msg_send_dispatch(
                enc,
                self.sels.dispatch_threadgroups,
                gx, gy, 1,
                128, 1, 1,
            );

            Self::wrap(out_buf, out_bytes, m, n)
        }
    }

    fn matmul_transpose(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let m = a.rows;
        let k = a.cols;
        let n = b.rows; // B is (N, K), transposed
        assert_eq!(k, b.cols, "matmul_transpose dimension mismatch");

        let out_count = m * n;
        let out_bytes = out_count * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_gemm_transpose_f16);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, m as u32, 3);
            self.set_u32(enc, k as u32, 4);
            self.set_u32(enc, n as u32, 5);

            // simdgroup_matrix: 32x32 tiles, 128 threads (4 simdgroups)
            let gx = (n + 31) / 32;
            let gy = (m + 31) / 32;
            msg_send_dispatch(
                enc,
                self.sels.dispatch_threadgroups,
                gx, gy, 1,
                128, 1, 1,
            );

            Self::wrap(out_buf, out_bytes, m, n)
        }
    }

    fn add_bias(&self, t: &mut DeviceTensor, bias: &DeviceTensor) {
        let rows = t.rows;
        let cols = t.cols;
        assert_eq!(bias.cols, cols, "add_bias: bias width mismatch");

        unsafe {
            let enc = self.ensure_encoder(self.pso_add_bias_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, Self::buf_id(bias), 1);
            self.set_u32(enc, rows as u32, 2);
            self.set_u32(enc, cols as u32, 3);

            self.dispatch_2d(enc, cols, rows, 16, 16);
        }
    }

    fn add_tensor(&self, a: &DeviceTensor, b: &DeviceTensor) -> DeviceTensor {
        let count = a.rows * a.cols;
        let out_bytes = count * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_add_tensor_f16);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, count as u32, 3);

            self.dispatch_1d(enc, count);

            Self::wrap(out_buf, out_bytes, a.rows, a.cols)
        }
    }

    fn gelu(&self, t: &DeviceTensor) -> DeviceTensor {
        let count = t.rows * t.cols;
        let out_bytes = count * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_gelu_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, out_buf, 1);
            self.set_u32(enc, count as u32, 2);

            self.dispatch_1d(enc, count);

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
        let out_bytes = rows * cols * 2; // f16

        // Choose threads per threadgroup: min(256, next_power_of_2(cols))
        let threads_per_group = (cols.next_power_of_two()).min(256);

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_layer_norm_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, Self::buf_id(w), 1);
            self.set_buffer(enc, Self::buf_id(b), 2);
            self.set_buffer(enc, out_buf, 3);
            self.set_u32(enc, rows as u32, 4);
            self.set_u32(enc, cols as u32, 5);
            self.set_f32(enc, eps, 6);

            self.dispatch_rows(enc, rows, threads_per_group);

            Self::wrap(out_buf, out_bytes, rows, cols)
        }
    }

    fn softmax_rows(&self, t: &mut DeviceTensor) {
        let rows = t.rows;
        let cols = t.cols;

        let threads_per_group = (cols.next_power_of_two()).min(256);

        unsafe {
            let enc = self.ensure_encoder(self.pso_softmax_rows_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_u32(enc, rows as u32, 1);
            self.set_u32(enc, cols as u32, 2);

            self.dispatch_rows(enc, rows, threads_per_group);
        }
    }

    fn scale(&self, t: &mut DeviceTensor, factor: f32) {
        let count = t.rows * t.cols;

        unsafe {
            let enc = self.ensure_encoder(self.pso_scale_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_f32(enc, factor, 1);
            self.set_u32(enc, count as u32, 2);

            self.dispatch_1d(enc, count);
        }
    }

    fn slice_columns(&self, t: &DeviceTensor, start: usize, end: usize) -> DeviceTensor {
        let rows = t.rows;
        let width = end - start;
        let out_bytes = rows * width * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_slice_columns_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, out_buf, 1);
            self.set_u32(enc, t.cols as u32, 2);
            self.set_u32(enc, start as u32, 3);
            self.set_u32(enc, width as u32, 4);
            self.set_u32(enc, rows as u32, 5);

            self.dispatch_2d(enc, width, rows, 16, 16);

            Self::wrap(out_buf, out_bytes, rows, width)
        }
    }

    fn scatter_columns(&self, dst: &mut DeviceTensor, src: &DeviceTensor, col_offset: usize) {
        let rows = src.rows;

        unsafe {
            let enc = self.ensure_encoder(self.pso_scatter_columns_f16);

            self.set_buffer(enc, Self::buf_id(dst), 0);
            self.set_buffer(enc, Self::buf_id(src), 1);
            self.set_u32(enc, dst.cols as u32, 2);
            self.set_u32(enc, src.cols as u32, 3);
            self.set_u32(enc, col_offset as u32, 4);
            self.set_u32(enc, rows as u32, 5);

            self.dispatch_2d(enc, src.cols, rows, 16, 16);
        }
    }

    fn zeros(&self, rows: usize, cols: usize) -> DeviceTensor {
        let count = rows * cols;
        let byte_len = count * 2; // f16
        let buf = unsafe { self.create_buffer_empty(byte_len) };
        // Zero-fill: create_buffer_empty gives uninitialized memory,
        // so explicitly zero it.
        unsafe {
            let ptr = msg_send_ptr(buf, self.sels.contents) as *mut u8;
            std::ptr::write_bytes(ptr, 0, byte_len);
        }
        Self::wrap(buf, byte_len, rows, cols)
    }

    fn upload_mask(&self, mask: &[u32]) -> DeviceTensor {
        let byte_len = mask.len() * std::mem::size_of::<u32>();
        let buf = unsafe { self.create_buffer_u32(mask) };
        Self::wrap(buf, byte_len, 1, mask.len())
    }

    fn apply_attention_mask(&self, scores: &mut DeviceTensor, mask: &DeviceTensor) {
        let rows = scores.rows;
        let cols = scores.cols;

        unsafe {
            let enc = self.ensure_encoder(self.pso_attention_mask_f16);

            self.set_buffer(enc, Self::buf_id(scores), 0);
            self.set_buffer(enc, Self::buf_id(mask), 1);
            self.set_u32(enc, rows as u32, 2);
            self.set_u32(enc, cols as u32, 3);

            self.dispatch_2d(enc, cols, rows, 16, 16);
        }
    }

    fn mean_pool(&self, hidden: &DeviceTensor, mask: &DeviceTensor) -> Vec<f32> {
        let rows = hidden.rows;
        let cols = hidden.cols;
        let out_bytes = cols * std::mem::size_of::<f32>(); // output is f32

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_mean_pool_f16);

            self.set_buffer(enc, Self::buf_id(hidden), 0);
            self.set_buffer(enc, Self::buf_id(mask), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, rows as u32, 3);
            self.set_u32(enc, cols as u32, 4);

            self.dispatch_1d(enc, cols);

            // CPU needs the result — flush all pending GPU work.
            self.flush();

            let result = self.read_buffer_f32(out_buf, cols);

            // Release temporary output buffer.
            msg_send_void(out_buf, self.sels.release);

            result
        }
    }

    fn batched_matmul_transpose(
        &self,
        a: &DeviceTensor,
        b: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
    ) -> DeviceTensor {
        let k = a.cols;
        let out_count = batch_size * seq_len * seq_len;
        let out_bytes = out_count * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_batched_gemm_transpose_f16);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, seq_len as u32, 3);
            self.set_u32(enc, k as u32, 4);

            // simdgroup_matrix: 32x32 tiles, 128 threads (4 simdgroups)
            let gx = (seq_len + 31) / 32;
            let gy = (seq_len + 31) / 32;
            msg_send_dispatch(
                enc,
                self.sels.dispatch_threadgroups,
                gx, gy, batch_size, // threadgroups
                128, 1, 1,          // threads per threadgroup
            );

            Self::wrap(out_buf, out_bytes, batch_size * seq_len, seq_len)
        }
    }

    fn batched_matmul(
        &self,
        a: &DeviceTensor,
        b: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
    ) -> DeviceTensor {
        let k = b.cols;
        let out_count = batch_size * seq_len * k;
        let out_bytes = out_count * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_batched_gemm_f16);

            self.set_buffer(enc, Self::buf_id(a), 0);
            self.set_buffer(enc, Self::buf_id(b), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, seq_len as u32, 3);
            self.set_u32(enc, k as u32, 4);

            // simdgroup_matrix: 32x32 tiles, 128 threads (4 simdgroups)
            let gx = (k + 31) / 32;
            let gy = (seq_len + 31) / 32;
            msg_send_dispatch(
                enc,
                self.sels.dispatch_threadgroups,
                gx, gy, batch_size, // threadgroups
                128, 1, 1,          // threads per threadgroup
            );

            Self::wrap(out_buf, out_bytes, batch_size * seq_len, k)
        }
    }

    fn batched_attention_mask(
        &self,
        scores: &mut DeviceTensor,
        mask: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
    ) {
        let total_rows = batch_size * seq_len;

        unsafe {
            let enc = self.ensure_encoder(self.pso_batched_attention_mask_f16);

            self.set_buffer(enc, Self::buf_id(scores), 0);
            self.set_buffer(enc, Self::buf_id(mask), 1);
            self.set_u32(enc, total_rows as u32, 2);
            self.set_u32(enc, seq_len as u32, 3);

            self.dispatch_2d(enc, seq_len, total_rows, 16, 16);
        }
    }

    fn batched_mean_pool(
        &self,
        hidden: &DeviceTensor,
        mask: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
    ) -> Vec<Vec<f32>> {
        let cols = hidden.cols;
        let out_count = batch_size * cols;
        let out_bytes = out_count * std::mem::size_of::<f32>(); // output is f32

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_batched_mean_pool_f16);

            self.set_buffer(enc, Self::buf_id(hidden), 0);
            self.set_buffer(enc, Self::buf_id(mask), 1);
            self.set_buffer(enc, out_buf, 2);
            self.set_u32(enc, seq_len as u32, 3);
            self.set_u32(enc, cols as u32, 4);

            self.dispatch_rows(enc, batch_size, 256);

            // CPU needs the result — flush all pending GPU work.
            self.flush();

            let flat = self.read_buffer_f32(out_buf, out_count);

            // Release temporary output buffer.
            msg_send_void(out_buf, self.sels.release);

            flat.chunks(cols).map(|c| c.to_vec()).collect()
        }
    }

    fn transpose_heads(
        &self,
        t: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
        num_heads: usize,
        head_dim: usize,
    ) -> DeviceTensor {
        let total_in_rows = batch_size * seq_len;
        debug_assert_eq!(t.rows, total_in_rows);
        debug_assert_eq!(t.cols, num_heads * head_dim);

        let total_out_rows = batch_size * num_heads * seq_len;
        let out_bytes = total_out_rows * head_dim * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_transpose_heads_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, out_buf, 1);
            self.set_u32(enc, batch_size as u32, 2);
            self.set_u32(enc, seq_len as u32, 3);
            self.set_u32(enc, num_heads as u32, 4);
            self.set_u32(enc, head_dim as u32, 5);

            // 3D grid: (head_dim, B*S, H)
            let gx = (head_dim + 15) / 16;
            let gy = (total_in_rows + 15) / 16;
            msg_send_dispatch(
                enc,
                self.sels.dispatch_threadgroups,
                gx, gy, num_heads, // threadgroups
                16, 16, 1,         // threads per threadgroup
            );

            Self::wrap(out_buf, out_bytes, total_out_rows, head_dim)
        }
    }

    fn untranspose_heads(
        &self,
        t: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
        num_heads: usize,
        head_dim: usize,
    ) -> DeviceTensor {
        debug_assert_eq!(t.rows, batch_size * num_heads * seq_len);
        debug_assert_eq!(t.cols, head_dim);

        let total_out_rows = batch_size * seq_len;
        let out_cols = num_heads * head_dim;
        let out_bytes = total_out_rows * out_cols * 2; // f16

        unsafe {
            let out_buf = self.create_buffer_empty(out_bytes);
            let enc = self.ensure_encoder(self.pso_untranspose_heads_f16);

            self.set_buffer(enc, Self::buf_id(t), 0);
            self.set_buffer(enc, out_buf, 1);
            self.set_u32(enc, batch_size as u32, 2);
            self.set_u32(enc, seq_len as u32, 3);
            self.set_u32(enc, num_heads as u32, 4);
            self.set_u32(enc, head_dim as u32, 5);

            // 3D grid: (head_dim, B*S, H)
            let gx = (head_dim + 15) / 16;
            let gy = (total_out_rows + 15) / 16;
            msg_send_dispatch(
                enc,
                self.sels.dispatch_threadgroups,
                gx, gy, num_heads, // threadgroups
                16, 16, 1,         // threads per threadgroup
            );

            Self::wrap(out_buf, out_bytes, total_out_rows, out_cols)
        }
    }

    fn multi_head_batched_attention_mask(
        &self,
        scores: &mut DeviceTensor,
        mask: &DeviceTensor,
        batch_size: usize,
        seq_len: usize,
        num_heads: usize,
    ) {
        let total_rows = batch_size * num_heads * seq_len;

        unsafe {
            let enc = self.ensure_encoder(self.pso_multi_head_batched_attention_mask_f16);

            self.set_buffer(enc, Self::buf_id(scores), 0);
            self.set_buffer(enc, Self::buf_id(mask), 1);
            self.set_u32(enc, total_rows as u32, 2);
            self.set_u32(enc, seq_len as u32, 3);
            self.set_u32(enc, num_heads as u32, 4);

            self.dispatch_2d(enc, seq_len, total_rows, 16, 16);
        }
    }

    fn name(&self) -> &'static str {
        "Metal"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tensor::Tensor;

    fn backend() -> MetalBackend {
        MetalBackend::try_new().expect("Metal device required for these tests")
    }

    // ---------------------------------------------------------------
    // f32 ↔ f16 conversion tests
    // ---------------------------------------------------------------

    #[test]
    fn test_f16_roundtrip_common_values() {
        // Values commonly found in embedding weights / activations
        let values = [
            0.0f32, -0.0, 1.0, -1.0, 0.5, -0.5, 0.1, -0.1,
            0.001, 0.01, 0.25, 0.75, 2.0, 4.0, 8.0, 16.0,
            100.0, -100.0, 0.333, -0.707,
        ];
        for &v in &values {
            let h = f32_to_f16(v);
            let back = f16_to_f32(h);
            let tol = v.abs() * 1e-3 + 1e-4; // relative + absolute tolerance
            assert!(
                (back - v).abs() <= tol,
                "roundtrip failed for {}: f16 bits=0x{:04X}, back={}",
                v, h, back
            );
        }
    }

    #[test]
    fn test_f16_zero_preserves_sign() {
        let pos = f32_to_f16(0.0);
        let neg = f32_to_f16(-0.0);
        assert_eq!(f16_to_f32(pos), 0.0);
        assert_eq!(f16_to_f32(neg), -0.0_f32);
        // Positive zero should have sign bit 0
        assert_eq!(pos & 0x8000, 0);
        // Negative zero should have sign bit 1
        assert_eq!(neg & 0x8000, 0x8000);
    }

    #[test]
    fn test_f16_nan_preserved() {
        let nan = f32::NAN;
        let h = f32_to_f16(nan);
        let back = f16_to_f32(h);
        assert!(back.is_nan(), "NaN should survive roundtrip, got {}", back);
        // f16 NaN: exponent = 31 (0x1F), mantissa != 0
        assert_eq!(h & 0x7C00, 0x7C00, "NaN exponent should be all 1s");
        assert_ne!(h & 0x03FF, 0, "NaN mantissa should be non-zero");
    }

    #[test]
    fn test_f16_infinity_preserved() {
        let h_pos = f32_to_f16(f32::INFINITY);
        let h_neg = f32_to_f16(f32::NEG_INFINITY);
        assert_eq!(f16_to_f32(h_pos), f32::INFINITY);
        assert_eq!(f16_to_f32(h_neg), f32::NEG_INFINITY);
        assert_eq!(h_pos, 0x7C00); // +inf
        assert_eq!(h_neg, 0xFC00); // -inf
    }

    #[test]
    fn test_f16_overflow_to_inf() {
        // f16 max is 65504.0; values above should become inf
        let h = f32_to_f16(70000.0);
        assert_eq!(f16_to_f32(h), f32::INFINITY);
    }

    #[test]
    fn test_f16_underflow_to_zero() {
        // Very small values below f16 subnormal range → ±0
        let h = f32_to_f16(1e-9);
        assert_eq!(f16_to_f32(h), 0.0);
    }

    #[test]
    fn test_f16_rounding() {
        // Verify round-to-nearest-even behavior
        // 1.0 = 0x3C00 in f16. 1.0 + 1 ULP in f16 = 1.0009766.
        // Test a value exactly between two f16 values (ties go to even).
        let exact = f16_to_f32(0x3C01); // 1.0009766
        let mid = (1.0 + exact) / 2.0; // midpoint
        let h = f32_to_f16(mid);
        // Should round to even (0x3C00 is even, 0x3C01 is odd) → 0x3C00
        assert_eq!(h, 0x3C00, "tie should round to even: mid={}, h=0x{:04X}", mid, h);

        // Value slightly above midpoint should round up
        let above = mid + 1e-7;
        let h_above = f32_to_f16(above);
        assert_eq!(h_above, 0x3C01, "above midpoint should round up: val={}, h=0x{:04X}", above, h_above);
    }

    #[test]
    fn test_f16_max_representable() {
        // f16 max: sign=0, exp=30, mantissa=0x3FF → 65504.0
        let h = f32_to_f16(65504.0);
        assert_eq!(h, 0x7BFF);
        assert_eq!(f16_to_f32(h), 65504.0);
    }

    /// CPU reference for transpose_heads: (B*S, H*D) -> (B*H*S, D)
    fn cpu_transpose_heads(
        data: &[f32],
        batch_size: usize,
        seq_len: usize,
        num_heads: usize,
        head_dim: usize,
    ) -> Vec<f32> {
        let mut out = vec![0.0f32; batch_size * num_heads * seq_len * head_dim];
        for b in 0..batch_size {
            for h in 0..num_heads {
                for s in 0..seq_len {
                    let src_off = (b * seq_len + s) * (num_heads * head_dim) + h * head_dim;
                    let dst_off = (b * num_heads * seq_len + h * seq_len + s) * head_dim;
                    out[dst_off..dst_off + head_dim]
                        .copy_from_slice(&data[src_off..src_off + head_dim]);
                }
            }
        }
        out
    }

    /// CPU reference for untranspose_heads: (B*H*S, D) -> (B*S, H*D)
    fn cpu_untranspose_heads(
        data: &[f32],
        batch_size: usize,
        seq_len: usize,
        num_heads: usize,
        head_dim: usize,
    ) -> Vec<f32> {
        let mut out = vec![0.0f32; batch_size * seq_len * num_heads * head_dim];
        for b in 0..batch_size {
            for h in 0..num_heads {
                for s in 0..seq_len {
                    let src_off = (b * num_heads * seq_len + h * seq_len + s) * head_dim;
                    let dst_off = (b * seq_len + s) * (num_heads * head_dim) + h * head_dim;
                    out[dst_off..dst_off + head_dim]
                        .copy_from_slice(&data[src_off..src_off + head_dim]);
                }
            }
        }
        out
    }

    /// CPU reference for multi_head_batched_attention_mask
    fn cpu_multi_head_mask(
        scores: &mut [f32],
        mask: &[u32],
        batch_size: usize,
        seq_len: usize,
        num_heads: usize,
    ) {
        let total_rows = batch_size * num_heads * seq_len;
        let group_size = num_heads * seq_len;
        for r in 0..total_rows {
            let group = r / group_size;
            for c in 0..seq_len {
                let mask_idx = group * seq_len + c;
                if mask[mask_idx] == 0 {
                    scores[r * seq_len + c] = -10000.0;
                }
            }
        }
    }

    fn assert_vecs_close(a: &[f32], b: &[f32], tol: f32, label: &str) {
        assert_eq!(a.len(), b.len(), "{}: length mismatch {} vs {}", label, a.len(), b.len());
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert!(
                (x - y).abs() <= tol,
                "{}: mismatch at index {}: {} vs {} (diff {})",
                label,
                i,
                x,
                y,
                (x - y).abs()
            );
        }
    }

    // ---------------------------------------------------------------
    // transpose_heads tests
    // ---------------------------------------------------------------

    #[test]
    fn test_transpose_heads_minilm_dims() {
        // MiniLM-L6: 12 heads, 32 head_dim, batch=1, seq=7
        let b = backend();
        let (batch, seq, heads, dim) = (1, 7, 12, 32);
        let n = batch * seq * heads * dim;
        let data: Vec<f32> = (0..n).map(|i| i as f32 * 0.01).collect();

        let input = b.upload(&Tensor::from_slice(&data, batch * seq, heads * dim));
        let result = b.transpose_heads(&input, batch, seq, heads, dim);
        let out = b.download(&result);

        let expected = cpu_transpose_heads(&data, batch, seq, heads, dim);
        assert_eq!(out.rows, batch * heads * seq);
        assert_eq!(out.cols, dim);
        assert_vecs_close(&out.data, &expected, 2e-2, "transpose_heads_minilm");
    }

    #[test]
    fn test_transpose_heads_batched() {
        // batch=4, seq=11 (not multiple of 16), heads=8, dim=16
        let b = backend();
        let (batch, seq, heads, dim) = (4, 11, 8, 16);
        let n = batch * seq * heads * dim;
        let data: Vec<f32> = (0..n).map(|i| (i as f32).sin()).collect();

        let input = b.upload(&Tensor::from_slice(&data, batch * seq, heads * dim));
        let result = b.transpose_heads(&input, batch, seq, heads, dim);
        let out = b.download(&result);

        let expected = cpu_transpose_heads(&data, batch, seq, heads, dim);
        assert_eq!(out.rows, batch * heads * seq);
        assert_eq!(out.cols, dim);
        assert_vecs_close(&out.data, &expected, 2e-2, "transpose_heads_batched");
    }

    #[test]
    fn test_transpose_heads_single_token() {
        // Edge case: seq_len=1
        let b = backend();
        let (batch, seq, heads, dim) = (2, 1, 4, 8);
        let n = batch * seq * heads * dim;
        let data: Vec<f32> = (0..n).map(|i| i as f32).collect();

        let input = b.upload(&Tensor::from_slice(&data, batch * seq, heads * dim));
        let result = b.transpose_heads(&input, batch, seq, heads, dim);
        let out = b.download(&result);

        let expected = cpu_transpose_heads(&data, batch, seq, heads, dim);
        assert_vecs_close(&out.data, &expected, 2e-2, "transpose_heads_single_token");
    }

    // ---------------------------------------------------------------
    // untranspose_heads tests
    // ---------------------------------------------------------------

    #[test]
    fn test_untranspose_heads_minilm_dims() {
        let b = backend();
        let (batch, seq, heads, dim) = (1, 7, 12, 32);
        let n = batch * heads * seq * dim;
        let data: Vec<f32> = (0..n).map(|i| i as f32 * 0.01).collect();

        let input = b.upload(&Tensor::from_slice(&data, batch * heads * seq, dim));
        let result = b.untranspose_heads(&input, batch, seq, heads, dim);
        let out = b.download(&result);

        let expected = cpu_untranspose_heads(&data, batch, seq, heads, dim);
        assert_eq!(out.rows, batch * seq);
        assert_eq!(out.cols, heads * dim);
        assert_vecs_close(&out.data, &expected, 2e-2, "untranspose_heads_minilm");
    }

    #[test]
    fn test_untranspose_heads_batched() {
        let b = backend();
        let (batch, seq, heads, dim) = (4, 11, 8, 16);
        let n = batch * heads * seq * dim;
        let data: Vec<f32> = (0..n).map(|i| (i as f32).cos()).collect();

        let input = b.upload(&Tensor::from_slice(&data, batch * heads * seq, dim));
        let result = b.untranspose_heads(&input, batch, seq, heads, dim);
        let out = b.download(&result);

        let expected = cpu_untranspose_heads(&data, batch, seq, heads, dim);
        assert_vecs_close(&out.data, &expected, 2e-2, "untranspose_heads_batched");
    }

    #[test]
    fn test_transpose_untranspose_roundtrip() {
        // transpose then untranspose should recover original data
        let b = backend();
        let (batch, seq, heads, dim) = (3, 9, 6, 16);
        let n = batch * seq * heads * dim;
        let data: Vec<f32> = (0..n).map(|i| i as f32 * 0.1).collect();

        let input = b.upload(&Tensor::from_slice(&data, batch * seq, heads * dim));
        let transposed = b.transpose_heads(&input, batch, seq, heads, dim);
        let roundtrip = b.untranspose_heads(&transposed, batch, seq, heads, dim);
        let out = b.download(&roundtrip);

        assert_eq!(out.rows, batch * seq);
        assert_eq!(out.cols, heads * dim);
        assert_vecs_close(&out.data, &data, 0.3, "roundtrip");
    }

    // ---------------------------------------------------------------
    // multi_head_batched_attention_mask tests
    // ---------------------------------------------------------------

    #[test]
    fn test_multi_head_mask_basic() {
        let b = backend();
        let (batch, seq, heads) = (1, 4, 2);
        let total_rows = batch * heads * seq;
        // Scores: all ones
        let mut scores_data = vec![1.0f32; total_rows * seq];
        // Mask: [1,1,0,0] — first 2 tokens valid, last 2 padding
        let mask_data: Vec<u32> = vec![1, 1, 0, 0];

        // GPU
        let mut scores_dt =
            b.upload(&Tensor::from_slice(&scores_data, total_rows, seq));
        let mask_dt = b.upload_mask(&mask_data);
        b.multi_head_batched_attention_mask(
            &mut scores_dt,
            &mask_dt,
            batch,
            seq,
            heads,
        );
        let gpu_out = b.download(&scores_dt);

        // CPU reference
        cpu_multi_head_mask(&mut scores_data, &mask_data, batch, seq, heads);

        // f16 rounds -10000.0 to -10000.0 exactly (representable), 1.0 is exact too
        assert_vecs_close(&gpu_out.data, &scores_data, 1.0, "multi_head_mask_basic");
        // Verify masked positions are -10000
        for r in 0..total_rows {
            assert!((gpu_out.data[r * seq + 2] - (-10000.0)).abs() < 1.0, "row {} col 2 should be masked, got {}", r, gpu_out.data[r * seq + 2]);
            assert!((gpu_out.data[r * seq + 3] - (-10000.0)).abs() < 1.0, "row {} col 3 should be masked, got {}", r, gpu_out.data[r * seq + 3]);
            assert!((gpu_out.data[r * seq + 0] - 1.0).abs() < 0.01, "row {} col 0 should be unmasked, got {}", r, gpu_out.data[r * seq + 0]);
            assert!((gpu_out.data[r * seq + 1] - 1.0).abs() < 0.01, "row {} col 1 should be unmasked, got {}", r, gpu_out.data[r * seq + 1]);
        }
    }

    #[test]
    fn test_multi_head_mask_multi_batch() {
        // batch=2, different mask per batch sequence
        let b = backend();
        let (batch, seq, heads) = (2, 3, 4);
        let total_rows = batch * heads * seq;
        let mut scores_data = vec![5.0f32; total_rows * seq];
        // Mask: batch 0 = [1,1,0], batch 1 = [1,0,0]
        let mask_data: Vec<u32> = vec![1, 1, 0, 1, 0, 0];

        let mut scores_dt =
            b.upload(&Tensor::from_slice(&scores_data, total_rows, seq));
        let mask_dt = b.upload_mask(&mask_data);
        b.multi_head_batched_attention_mask(
            &mut scores_dt,
            &mask_dt,
            batch,
            seq,
            heads,
        );
        let gpu_out = b.download(&scores_dt);

        cpu_multi_head_mask(&mut scores_data, &mask_data, batch, seq, heads);
        assert_vecs_close(&gpu_out.data, &scores_data, 1.0, "multi_head_mask_multi_batch");
    }

    #[test]
    fn test_multi_head_mask_minilm_dims() {
        // Realistic: batch=1, seq=15, heads=12
        let b = backend();
        let (batch, seq, heads) = (1, 15, 12);
        let total_rows = batch * heads * seq;
        let mut scores_data: Vec<f32> =
            (0..(total_rows * seq)).map(|i| (i as f32 * 0.001).sin()).collect();
        // First 10 tokens valid, last 5 padding
        let mut mask_data = vec![1u32; seq];
        for i in 10..seq {
            mask_data[i] = 0;
        }

        let mut scores_dt =
            b.upload(&Tensor::from_slice(&scores_data, total_rows, seq));
        let mask_dt = b.upload_mask(&mask_data);
        b.multi_head_batched_attention_mask(
            &mut scores_dt,
            &mask_dt,
            batch,
            seq,
            heads,
        );
        let gpu_out = b.download(&scores_dt);

        cpu_multi_head_mask(&mut scores_data, &mask_data, batch, seq, heads);
        assert_vecs_close(&gpu_out.data, &scores_data, 1.0, "multi_head_mask_minilm");
    }

    // ---------------------------------------------------------------
    // GEMM CPU reference helpers
    // ---------------------------------------------------------------

    /// CPU reference matmul: C[M,N] = A[M,K] * B[K,N]
    fn cpu_matmul(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
        let mut c = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for p in 0..k {
                    sum += a[i * k + p] * b[p * n + j];
                }
                c[i * n + j] = sum;
            }
        }
        c
    }

    /// CPU reference matmul_transpose: C[M,N] = A[M,K] * B^T, B stored as (N,K)
    fn cpu_matmul_transpose(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
        let mut c = vec![0.0f32; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0.0f32;
                for p in 0..k {
                    sum += a[i * k + p] * b[j * k + p];
                }
                c[i * n + j] = sum;
            }
        }
        c
    }

    // ---------------------------------------------------------------
    // GEMM tests
    // ---------------------------------------------------------------

    #[test]
    fn test_gemm_basic() {
        let b = backend();
        let (m, k, n) = (8, 6, 10);
        let a_data: Vec<f32> = (0..m * k).map(|i| (i as f32) * 0.1).collect();
        let b_data: Vec<f32> = (0..k * n).map(|i| (i as f32) * 0.05).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, m);
        assert_eq!(result.cols, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_basic");
    }

    #[test]
    fn test_gemm_non_aligned() {
        // Dimensions not multiples of 32
        let b = backend();
        let (m, k, n) = (17, 23, 19);
        let a_data: Vec<f32> = (0..m * k).map(|i| ((i as f32) * 0.03).sin()).collect();
        let b_data: Vec<f32> = (0..k * n).map(|i| ((i as f32) * 0.07).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_non_aligned");
    }

    #[test]
    fn test_gemm_large() {
        // Production-like dimensions
        let b = backend();
        let (m, k, n) = (512, 384, 384);
        let a_data: Vec<f32> = (0..m * k).map(|i| ((i as f32) * 0.001).sin()).collect();
        let b_data: Vec<f32> = (0..k * n).map(|i| ((i as f32) * 0.002).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_large");
    }

    #[test]
    fn test_gemm_transpose_basic() {
        let b = backend();
        let (m, k, n) = (8, 6, 10);
        let a_data: Vec<f32> = (0..m * k).map(|i| (i as f32) * 0.1).collect();
        // B stored as (N, K) for transpose
        let b_data: Vec<f32> = (0..n * k).map(|i| (i as f32) * 0.05).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, n, k));
        let c_dt = b.matmul_transpose(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul_transpose(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, m);
        assert_eq!(result.cols, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_transpose_basic");
    }

    #[test]
    fn test_gemm_transpose_non_aligned() {
        let b = backend();
        let (m, k, n) = (17, 23, 19);
        let a_data: Vec<f32> = (0..m * k).map(|i| ((i as f32) * 0.03).sin()).collect();
        let b_data: Vec<f32> = (0..n * k).map(|i| ((i as f32) * 0.07).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, n, k));
        let c_dt = b.matmul_transpose(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul_transpose(&a_data, &b_data, m, k, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_transpose_non_aligned");
    }

    #[test]
    fn test_batched_gemm_transpose() {
        let b = backend();
        let (batch, s, k) = (12, 64, 32);
        // A: (batch*S, K), B: (batch*S, K), C: (batch*S, S)
        let a_data: Vec<f32> = (0..batch * s * k).map(|i| ((i as f32) * 0.001).sin()).collect();
        let b_data: Vec<f32> = (0..batch * s * k).map(|i| ((i as f32) * 0.002).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, batch * s, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, batch * s, k));
        let c_dt = b.batched_matmul_transpose(&a_dt, &b_dt, batch, s);
        let result = b.download(&c_dt);

        // CPU reference: per-batch C[b] = A[b] * B[b]^T
        let mut expected = vec![0.0f32; batch * s * s];
        for bi in 0..batch {
            let a_off = bi * s * k;
            let b_off = bi * s * k;
            let c_off = bi * s * s;
            for i in 0..s {
                for j in 0..s {
                    let mut sum = 0.0f32;
                    for p in 0..k {
                        sum += a_data[a_off + i * k + p] * b_data[b_off + j * k + p];
                    }
                    expected[c_off + i * s + j] = sum;
                }
            }
        }

        assert_eq!(result.rows, batch * s);
        assert_eq!(result.cols, s);
        assert_vecs_close(&result.data, &expected, 0.5, "batched_gemm_transpose");
    }

    #[test]
    fn test_batched_gemm() {
        let b = backend();
        let (batch, s, k) = (12, 64, 32);
        // A: (batch*S, S), B: (batch*S, K), C: (batch*S, K)
        let a_data: Vec<f32> = (0..batch * s * s).map(|i| ((i as f32) * 0.001).sin()).collect();
        let b_data: Vec<f32> = (0..batch * s * k).map(|i| ((i as f32) * 0.002).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, batch * s, s));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, batch * s, k));
        let c_dt = b.batched_matmul(&a_dt, &b_dt, batch, s);
        let result = b.download(&c_dt);

        // CPU reference: per-batch C[b] = A[b] * B[b]
        let mut expected = vec![0.0f32; batch * s * k];
        for bi in 0..batch {
            let a_off = bi * s * s;
            let b_off = bi * s * k;
            let c_off = bi * s * k;
            for i in 0..s {
                for j in 0..k {
                    let mut sum = 0.0f32;
                    for p in 0..s {
                        sum += a_data[a_off + i * s + p] * b_data[b_off + p * k + j];
                    }
                    expected[c_off + i * k + j] = sum;
                }
            }
        }

        assert_eq!(result.rows, batch * s);
        assert_eq!(result.cols, k);
        assert_vecs_close(&result.data, &expected, 0.5, "batched_gemm");
    }

    #[test]
    fn test_gemm_single_row() {
        // Edge case: M=1
        let b = backend();
        let (m, k, n) = (1, 32, 16);
        let a_data: Vec<f32> = (0..m * k).map(|i| (i as f32) * 0.1).collect();
        let b_data: Vec<f32> = (0..k * n).map(|i| (i as f32) * 0.05).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, 1);
        assert_eq!(result.cols, n);
        assert_vecs_close(&result.data, &expected, 2.0, "gemm_single_row");
    }

    #[test]
    fn test_gemm_mixed_tiles() {
        // 48x37 x 37x50: threadgroup grid is 2x2.
        // tgid(0,0): tile covers rows 0-31, cols 0-31 — fully in-bounds (fast path)
        // tgid(1,0): tile covers rows 0-31, cols 32-63 — cols exceed N=50, edge path
        // tgid(0,1): tile covers rows 32-63, exceeds M=48, edge path
        // tgid(1,1): edge path on both dims
        // This exercises both fast and edge paths in the same dispatch.
        let b = backend();
        let (m, k, n) = (48, 37, 50);
        let a_data: Vec<f32> = (0..m * k).map(|i| ((i as f32) * 0.02).sin()).collect();
        let b_data: Vec<f32> = (0..k * n).map(|i| ((i as f32) * 0.03).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, m);
        assert_eq!(result.cols, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_mixed_tiles");
    }

    #[test]
    fn test_gemm_single_col() {
        // Edge case: N=1 (matrix-vector multiply)
        let b = backend();
        let (m, k, n) = (16, 32, 1);
        let a_data: Vec<f32> = (0..m * k).map(|i| (i as f32) * 0.1).collect();
        let b_data: Vec<f32> = (0..k * n).map(|i| (i as f32) * 0.05).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, m);
        assert_eq!(result.cols, 1);
        assert_vecs_close(&result.data, &expected, 2.0, "gemm_single_col");
    }

    #[test]
    fn test_gemm_tiny_k() {
        // Edge case: K=1 (outer product) — well below the 8-wide simdgroup block
        let b = backend();
        let (m, k, n) = (10, 1, 12);
        let a_data: Vec<f32> = (0..m).map(|i| (i as f32) + 1.0).collect();
        let b_data: Vec<f32> = (0..n).map(|i| (i as f32) * 0.5).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, k, n));
        let c_dt = b.matmul(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, m);
        assert_eq!(result.cols, n);
        // Verify specific values: C[i,j] = a[i] * b[j] (f16 tolerance)
        assert!((result.data[0] - 1.0 * 0.0).abs() < 0.01, "C[0,0] = 1*0 = 0");
        assert!((result.data[1] - 1.0 * 0.5).abs() < 0.01, "C[0,1] = 1*0.5 = 0.5");
        assert!((result.data[n] - 2.0 * 0.0).abs() < 0.01, "C[1,0] = 2*0 = 0");
        assert_vecs_close(&result.data, &expected, 0.1, "gemm_tiny_k");
    }

    #[test]
    fn test_gemm_transpose_mixed_tiles() {
        // Same mixed-tile pattern for transpose variant
        let b = backend();
        let (m, k, n) = (48, 37, 50);
        let a_data: Vec<f32> = (0..m * k).map(|i| ((i as f32) * 0.02).sin()).collect();
        let b_data: Vec<f32> = (0..n * k).map(|i| ((i as f32) * 0.03).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, m, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, n, k));
        let c_dt = b.matmul_transpose(&a_dt, &b_dt);
        let result = b.download(&c_dt);

        let expected = cpu_matmul_transpose(&a_data, &b_data, m, k, n);
        assert_eq!(result.rows, m);
        assert_eq!(result.cols, n);
        assert_vecs_close(&result.data, &expected, 0.5, "gemm_transpose_mixed_tiles");
    }

    #[test]
    fn test_batched_gemm_transpose_non_aligned() {
        // S=50 and K=25 are not multiples of 32 — exercises edge tiles in batched path
        let b = backend();
        let (batch, s, k) = (3, 50, 25);
        let a_data: Vec<f32> = (0..batch * s * k).map(|i| ((i as f32) * 0.003).sin()).collect();
        let b_data: Vec<f32> = (0..batch * s * k).map(|i| ((i as f32) * 0.004).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, batch * s, k));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, batch * s, k));
        let c_dt = b.batched_matmul_transpose(&a_dt, &b_dt, batch, s);
        let result = b.download(&c_dt);

        let mut expected = vec![0.0f32; batch * s * s];
        for bi in 0..batch {
            let a_off = bi * s * k;
            let b_off = bi * s * k;
            let c_off = bi * s * s;
            for i in 0..s {
                for j in 0..s {
                    let mut sum = 0.0f32;
                    for p in 0..k {
                        sum += a_data[a_off + i * k + p] * b_data[b_off + j * k + p];
                    }
                    expected[c_off + i * s + j] = sum;
                }
            }
        }

        assert_eq!(result.rows, batch * s);
        assert_eq!(result.cols, s);
        assert_vecs_close(&result.data, &expected, 0.5, "batched_gemm_transpose_non_aligned");
    }

    #[test]
    fn test_batched_gemm_non_aligned() {
        // S=50 and K=25 are not multiples of 32
        let b = backend();
        let (batch, s, k) = (3, 50, 25);
        let a_data: Vec<f32> = (0..batch * s * s).map(|i| ((i as f32) * 0.0003).sin()).collect();
        let b_data: Vec<f32> = (0..batch * s * k).map(|i| ((i as f32) * 0.004).cos()).collect();

        let a_dt = b.upload(&Tensor::from_slice(&a_data, batch * s, s));
        let b_dt = b.upload(&Tensor::from_slice(&b_data, batch * s, k));
        let c_dt = b.batched_matmul(&a_dt, &b_dt, batch, s);
        let result = b.download(&c_dt);

        let mut expected = vec![0.0f32; batch * s * k];
        for bi in 0..batch {
            let a_off = bi * s * s;
            let b_off = bi * s * k;
            let c_off = bi * s * k;
            for i in 0..s {
                for j in 0..k {
                    let mut sum = 0.0f32;
                    for p in 0..s {
                        sum += a_data[a_off + i * s + p] * b_data[b_off + p * k + j];
                    }
                    expected[c_off + i * k + j] = sum;
                }
            }
        }

        assert_eq!(result.rows, batch * s);
        assert_eq!(result.cols, k);
        assert_vecs_close(&result.data, &expected, 0.5, "batched_gemm_non_aligned");
    }

    // ---------------------------------------------------------------
    // Command buffer batching tests
    // ---------------------------------------------------------------

    #[test]
    fn test_chained_operations_correctness() {
        // Verify that deferred dispatch produces correct results across
        // multiple chained operations (no premature reads of in-flight data).
        let b = backend();
        let data = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];

        // Chain: upload → matmul → add_bias → gelu → download
        let a = b.upload(&Tensor::from_slice(&[1.0, 0.0, 0.0, 1.0], 2, 2));
        let x = b.upload(&Tensor::from_slice(&data, 3, 2));
        let mut result = b.matmul(&x, &a); // identity matmul
        let bias = b.upload_1d(&[10.0, 20.0]);
        b.add_bias(&mut result, &bias);
        let result = b.gelu(&result);
        let out = b.download(&result);

        // After identity matmul: same as input
        // After add_bias: [11,22, 13,24, 15,26]
        // After gelu: gelu(x) ≈ x for large positive x
        assert_eq!(out.rows, 3);
        assert_eq!(out.cols, 2);
        for val in &out.data {
            assert!(*val > 0.0, "GELU of positive input should be positive, got {}", val);
        }
        // gelu(11) ≈ 11.0 (very close for large inputs); f16 tolerance
        assert!(
            (out.data[0] - 11.0).abs() < 0.1,
            "gelu(11) should be ~11.0, got {}",
            out.data[0]
        );
    }

    #[test]
    fn test_multiple_flushes() {
        // Ensure multiple download (flush) calls work correctly
        let b = backend();
        let data = vec![1.0f32, 2.0, 3.0, 4.0];
        let t1 = b.upload(&Tensor::from_slice(&data, 2, 2));

        // First chain + download
        let sum1 = b.add_tensor(&t1, &t1);
        let out1 = b.download(&sum1);
        assert_vecs_close(&out1.data, &[2.0, 4.0, 6.0, 8.0], 0.1, "first_flush");

        // Second chain + download (reuses same backend, new command buffer)
        let sum2 = b.add_tensor(&sum1, &t1);
        let out2 = b.download(&sum2);
        assert_vecs_close(&out2.data, &[3.0, 6.0, 9.0, 12.0], 0.1, "second_flush");
    }
}
