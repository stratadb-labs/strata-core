//! CUDA Driver API bindings loaded at runtime via dlopen.
//!
//! Uses the `DynLib` wrapper from `dl.rs` to load `libcuda.so.1` (Linux) or
//! `nvcuda.dll` (Windows) at runtime, avoiding any link-time CUDA dependency.
//! All Driver API function pointers are resolved once during `CudaApi::load()`
//! and cached for the lifetime of the process.

use std::ffi::CStr;
use std::os::raw::c_void;

use super::super::dl::DynLib;

// ---------------------------------------------------------------------------
// CUDA Driver API types
// ---------------------------------------------------------------------------

/// CUDA error code.
pub type CUresult = i32;
/// CUDA device ordinal.
pub type CUdevice = i32;
/// Opaque CUDA context handle.
pub type CUcontext = *mut c_void;
/// Opaque CUDA module handle.
pub type CUmodule = *mut c_void;
/// Opaque CUDA function (kernel) handle.
pub type CUfunction = *mut c_void;
/// Device pointer (64-bit address on the GPU).
pub type CUdeviceptr = u64;
/// Opaque CUDA stream handle.
pub type CUstream = *mut c_void;

/// Successful CUDA API call.
pub const CUDA_SUCCESS: CUresult = 0;

// ---------------------------------------------------------------------------
// Function pointer types for the CUDA Driver API
// ---------------------------------------------------------------------------

type FnCuInit = unsafe extern "C" fn(flags: u32) -> CUresult;
type FnCuDeviceGetCount = unsafe extern "C" fn(count: *mut i32) -> CUresult;
type FnCuDeviceGet = unsafe extern "C" fn(device: *mut CUdevice, ordinal: i32) -> CUresult;
type FnCuCtxCreate = unsafe extern "C" fn(ctx: *mut CUcontext, flags: u32, dev: CUdevice) -> CUresult;
type FnCuCtxDestroy = unsafe extern "C" fn(ctx: CUcontext) -> CUresult;
type FnCuMemAlloc = unsafe extern "C" fn(dptr: *mut CUdeviceptr, bytesize: usize) -> CUresult;
type FnCuMemFree = unsafe extern "C" fn(dptr: CUdeviceptr) -> CUresult;
type FnCuMemcpyHtoD =
    unsafe extern "C" fn(dst: CUdeviceptr, src: *const c_void, bytesize: usize) -> CUresult;
type FnCuMemcpyDtoH =
    unsafe extern "C" fn(dst: *mut c_void, src: CUdeviceptr, bytesize: usize) -> CUresult;
type FnCuModuleLoadData =
    unsafe extern "C" fn(module: *mut CUmodule, image: *const c_void) -> CUresult;
type FnCuModuleGetFunction =
    unsafe extern "C" fn(func: *mut CUfunction, module: CUmodule, name: *const i8) -> CUresult;
type FnCuLaunchKernel = unsafe extern "C" fn(
    f: CUfunction,
    grid_x: u32,
    grid_y: u32,
    grid_z: u32,
    block_x: u32,
    block_y: u32,
    block_z: u32,
    shared_mem: u32,
    stream: CUstream,
    params: *mut *mut c_void,
    extra: *mut *mut c_void,
) -> CUresult;
type FnCuCtxSynchronize = unsafe extern "C" fn() -> CUresult;
type FnCuStreamCreate = unsafe extern "C" fn(stream: *mut CUstream, flags: u32) -> CUresult;
type FnCuStreamSynchronize = unsafe extern "C" fn(stream: CUstream) -> CUresult;
type FnCuStreamDestroy = unsafe extern "C" fn(stream: CUstream) -> CUresult;
type FnCuModuleUnload = unsafe extern "C" fn(module: CUmodule) -> CUresult;
type FnCuMemsetD32 =
    unsafe extern "C" fn(dptr: CUdeviceptr, value: u32, count: usize) -> CUresult;

// ---------------------------------------------------------------------------
// CudaApi — resolved driver function pointers
// ---------------------------------------------------------------------------

/// Holds the dynamically loaded CUDA driver library and all resolved function
/// pointers. Created once via [`CudaApi::load()`] and shared across the
/// backend via `Arc<CudaApi>`.
pub struct CudaApi {
    /// Keep the library alive for the lifetime of this struct.
    _lib: DynLib,
    /// CUDA context created on device 0.
    pub ctx: CUcontext,

    // Function pointers (private — accessed through safe wrappers below).
    cu_ctx_destroy: FnCuCtxDestroy,
    cu_mem_alloc: FnCuMemAlloc,
    cu_mem_free: FnCuMemFree,
    cu_memcpy_h_to_d: FnCuMemcpyHtoD,
    cu_memcpy_d_to_h: FnCuMemcpyDtoH,
    cu_module_load_data: FnCuModuleLoadData,
    cu_module_get_function: FnCuModuleGetFunction,
    cu_launch_kernel: FnCuLaunchKernel,
    cu_ctx_synchronize: FnCuCtxSynchronize,
    cu_stream_create: FnCuStreamCreate,
    cu_stream_synchronize: FnCuStreamSynchronize,
    cu_stream_destroy: FnCuStreamDestroy,
    cu_module_unload: FnCuModuleUnload,
    cu_memset_d32: FnCuMemsetD32,
}

// SAFETY: All CUDA Driver API functions are thread-safe per the CUDA
// programming guide (section 3.3.1 — "The driver API is thread-safe").
// The context is created on load and used across threads.
unsafe impl Send for CudaApi {}
unsafe impl Sync for CudaApi {}

/// Helper macro to resolve a symbol and transmute to the expected fn pointer.
macro_rules! load_sym {
    ($lib:expr, $name:expr) => {{
        let cname = concat!($name, "\0");
        let cstr = unsafe { CStr::from_bytes_with_nul_unchecked(cname.as_bytes()) };
        let ptr = unsafe { $lib.sym(cstr) }
            .map_err(|e| format!("failed to load {}: {}", $name, e))?;
        if ptr.is_null() {
            return Err(format!("{} resolved to null", $name));
        }
        unsafe { std::mem::transmute::<*mut c_void, _>(ptr) }
    }};
}

impl CudaApi {
    /// Attempt to load the CUDA driver library and resolve all required
    /// function pointers. Calls `cuInit(0)` and verifies that at least one
    /// CUDA device is present.
    pub fn load() -> Result<Self, String> {
        // --- Open the driver library ---
        #[cfg(unix)]
        let lib_name = c"libcuda.so.1";
        #[cfg(windows)]
        let lib_name = c"nvcuda.dll";
        #[cfg(not(any(unix, windows)))]
        return Err("CUDA is not supported on this platform".to_string());

        let lib = DynLib::open(lib_name)?;

        // --- Resolve all function pointers ---
        let cu_init: FnCuInit = load_sym!(lib, "cuInit");
        let cu_device_get_count: FnCuDeviceGetCount = load_sym!(lib, "cuDeviceGetCount");
        let cu_device_get: FnCuDeviceGet = load_sym!(lib, "cuDeviceGet");
        let cu_ctx_create: FnCuCtxCreate = load_sym!(lib, "cuCtxCreate_v2");
        let cu_ctx_destroy: FnCuCtxDestroy = load_sym!(lib, "cuCtxDestroy_v2");
        let cu_mem_alloc: FnCuMemAlloc = load_sym!(lib, "cuMemAlloc_v2");
        let cu_mem_free: FnCuMemFree = load_sym!(lib, "cuMemFree_v2");
        let cu_memcpy_h_to_d: FnCuMemcpyHtoD = load_sym!(lib, "cuMemcpyHtoD_v2");
        let cu_memcpy_d_to_h: FnCuMemcpyDtoH = load_sym!(lib, "cuMemcpyDtoH_v2");
        let cu_module_load_data: FnCuModuleLoadData = load_sym!(lib, "cuModuleLoadData");
        let cu_module_get_function: FnCuModuleGetFunction =
            load_sym!(lib, "cuModuleGetFunction");
        let cu_launch_kernel: FnCuLaunchKernel = load_sym!(lib, "cuLaunchKernel");
        let cu_ctx_synchronize: FnCuCtxSynchronize = load_sym!(lib, "cuCtxSynchronize");
        let cu_stream_create: FnCuStreamCreate = load_sym!(lib, "cuStreamCreate");
        let cu_stream_synchronize: FnCuStreamSynchronize =
            load_sym!(lib, "cuStreamSynchronize");
        let cu_stream_destroy: FnCuStreamDestroy = load_sym!(lib, "cuStreamDestroy_v2");
        let cu_module_unload: FnCuModuleUnload = load_sym!(lib, "cuModuleUnload");
        let cu_memset_d32: FnCuMemsetD32 = load_sym!(lib, "cuMemsetD32_v2");

        // --- Initialize the driver ---
        let rc = unsafe { cu_init(0) };
        if rc != CUDA_SUCCESS {
            return Err(format!("cuInit failed with error code {}", rc));
        }

        // --- Verify at least one device ---
        let mut count: i32 = 0;
        let rc = unsafe { cu_device_get_count(&mut count) };
        if rc != CUDA_SUCCESS {
            return Err(format!("cuDeviceGetCount failed with error code {}", rc));
        }
        if count < 1 {
            return Err("no CUDA devices found".to_string());
        }

        // --- Get device 0 ---
        let mut device: CUdevice = 0;
        let rc = unsafe { cu_device_get(&mut device, 0) };
        if rc != CUDA_SUCCESS {
            return Err(format!("cuDeviceGet(0) failed with error code {}", rc));
        }

        // --- Create a context on device 0 ---
        let mut ctx: CUcontext = std::ptr::null_mut();
        let rc = unsafe { cu_ctx_create(&mut ctx, 0, device) };
        if rc != CUDA_SUCCESS {
            return Err(format!("cuCtxCreate failed with error code {}", rc));
        }

        Ok(Self {
            _lib: lib,
            ctx,
            cu_ctx_destroy,
            cu_mem_alloc,
            cu_mem_free,
            cu_memcpy_h_to_d,
            cu_memcpy_d_to_h,
            cu_module_load_data,
            cu_module_get_function,
            cu_launch_kernel,
            cu_ctx_synchronize,
            cu_stream_create,
            cu_stream_synchronize,
            cu_stream_destroy,
            cu_module_unload,
            cu_memset_d32,
        })
    }

    // -----------------------------------------------------------------------
    // Safe wrappers — each checks CUresult and returns Result<_, String>
    // -----------------------------------------------------------------------

    /// Destroy a CUDA context.
    pub fn ctx_destroy(&self, ctx: CUcontext) -> Result<(), String> {
        let rc = unsafe { (self.cu_ctx_destroy)(ctx) };
        check(rc, "cuCtxDestroy")
    }

    /// Allocate `bytesize` bytes on the device.
    pub fn mem_alloc(&self, bytesize: usize) -> Result<CUdeviceptr, String> {
        let mut dptr: CUdeviceptr = 0;
        let rc = unsafe { (self.cu_mem_alloc)(&mut dptr, bytesize) };
        check(rc, "cuMemAlloc")?;
        Ok(dptr)
    }

    /// Free device memory.
    pub fn mem_free(&self, dptr: CUdeviceptr) -> Result<(), String> {
        let rc = unsafe { (self.cu_mem_free)(dptr) };
        check(rc, "cuMemFree")
    }

    /// Copy `bytesize` bytes from host `src` to device `dst`.
    pub fn memcpy_h_to_d(
        &self,
        dst: CUdeviceptr,
        src: *const c_void,
        bytesize: usize,
    ) -> Result<(), String> {
        let rc = unsafe { (self.cu_memcpy_h_to_d)(dst, src, bytesize) };
        check(rc, "cuMemcpyHtoD")
    }

    /// Copy `bytesize` bytes from device `src` to host `dst`.
    pub fn memcpy_d_to_h(
        &self,
        dst: *mut c_void,
        src: CUdeviceptr,
        bytesize: usize,
    ) -> Result<(), String> {
        let rc = unsafe { (self.cu_memcpy_d_to_h)(dst, src, bytesize) };
        check(rc, "cuMemcpyDtoH")
    }

    /// Load a PTX module from a null-terminated image.
    pub fn module_load_data(&self, image: *const c_void) -> Result<CUmodule, String> {
        let mut module: CUmodule = std::ptr::null_mut();
        let rc = unsafe { (self.cu_module_load_data)(&mut module, image) };
        check(rc, "cuModuleLoadData")?;
        Ok(module)
    }

    /// Look up a kernel function by name inside a loaded module.
    pub fn module_get_function(
        &self,
        module: CUmodule,
        name: &CStr,
    ) -> Result<CUfunction, String> {
        let mut func: CUfunction = std::ptr::null_mut();
        let rc =
            unsafe { (self.cu_module_get_function)(&mut func, module, name.as_ptr()) };
        check(rc, &format!("cuModuleGetFunction({})", name.to_string_lossy()))?;
        Ok(func)
    }

    /// Launch a kernel on the given stream.
    ///
    /// # Safety
    ///
    /// `params` must point to an array of pointers to kernel arguments that
    /// match the kernel signature exactly. Lifetimes of pointed-to data must
    /// extend until the kernel completes (synchronize the stream).
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn launch_kernel(
        &self,
        func: CUfunction,
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        shared_mem: u32,
        stream: CUstream,
        params: *mut *mut c_void,
    ) -> Result<(), String> {
        let rc = (self.cu_launch_kernel)(
            func,
            grid.0,
            grid.1,
            grid.2,
            block.0,
            block.1,
            block.2,
            shared_mem,
            stream,
            params,
            std::ptr::null_mut(),
        );
        check(rc, "cuLaunchKernel")
    }

    /// Block until all work on the current context is complete.
    pub fn ctx_synchronize(&self) -> Result<(), String> {
        let rc = unsafe { (self.cu_ctx_synchronize)() };
        check(rc, "cuCtxSynchronize")
    }

    /// Create a new CUDA stream.
    pub fn stream_create(&self) -> Result<CUstream, String> {
        let mut stream: CUstream = std::ptr::null_mut();
        let rc = unsafe { (self.cu_stream_create)(&mut stream, 0) };
        check(rc, "cuStreamCreate")?;
        Ok(stream)
    }

    /// Block until all work on the given stream is complete.
    pub fn stream_synchronize(&self, stream: CUstream) -> Result<(), String> {
        let rc = unsafe { (self.cu_stream_synchronize)(stream) };
        check(rc, "cuStreamSynchronize")
    }

    /// Destroy a CUDA stream.
    pub fn stream_destroy(&self, stream: CUstream) -> Result<(), String> {
        let rc = unsafe { (self.cu_stream_destroy)(stream) };
        check(rc, "cuStreamDestroy")
    }

    /// Unload a previously loaded PTX module.
    pub fn module_unload(&self, module: CUmodule) -> Result<(), String> {
        let rc = unsafe { (self.cu_module_unload)(module) };
        check(rc, "cuModuleUnload")
    }

    /// Fill device memory with a 32-bit pattern (useful for zeroing).
    pub fn memset_d32(
        &self,
        dptr: CUdeviceptr,
        value: u32,
        count: usize,
    ) -> Result<(), String> {
        let rc = unsafe { (self.cu_memset_d32)(dptr, value, count) };
        check(rc, "cuMemsetD32")
    }
}

impl Drop for CudaApi {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            let _ = self.ctx_destroy(self.ctx);
        }
    }
}

/// Check a CUDA result code and return a descriptive error on failure.
fn check(rc: CUresult, fn_name: &str) -> Result<(), String> {
    if rc == CUDA_SUCCESS {
        Ok(())
    } else {
        Err(format!("{} failed with CUDA error code {}", fn_name, rc))
    }
}
