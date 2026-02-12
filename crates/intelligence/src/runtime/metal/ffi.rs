//! Objective-C runtime and Metal.framework FFI bindings.
//!
//! Uses raw `objc_msgSend` calls to avoid any external Objective-C crate dependency.
//! All Metal API access goes through typed wrappers around the variadic
//! `objc_msgSend` symbol, transmuted to the exact C calling convention needed
//! for each message signature.

use std::os::raw::c_void;

// ---------------------------------------------------------------------------
// Objective-C runtime types
// ---------------------------------------------------------------------------

/// Objective-C object pointer (`id`).
pub type Id = *mut c_void;
/// Objective-C selector (`SEL`).
pub type Sel = *mut c_void;
/// Objective-C class pointer (`Class`).
pub type Class = *mut c_void;
/// Objective-C `BOOL` (signed byte).
pub type BOOL = i8;
/// `NSUInteger` — pointer-sized unsigned integer.
pub type NSUInteger = usize;
/// `NSInteger` — pointer-sized signed integer.
pub type NSInteger = isize;

/// The nil object pointer.
pub const NIL: Id = std::ptr::null_mut();
/// Objective-C `YES`.
pub const YES: BOOL = 1;

/// `MTLResourceStorageModeShared` — CPU and GPU can both access the buffer.
pub const MTL_RESOURCE_STORAGE_MODE_SHARED: NSUInteger = 0;

// ---------------------------------------------------------------------------
// Linked frameworks
// ---------------------------------------------------------------------------

#[link(name = "objc", kind = "dylib")]
extern "C" {
    pub fn objc_getClass(name: *const i8) -> Class;
    pub fn sel_registerName(name: *const i8) -> Sel;
    /// The variadic Objective-C message dispatcher. Never called directly —
    /// always transmuted to a typed function pointer first.
    pub fn objc_msgSend(receiver: Id, selector: Sel, ...) -> Id;
}

#[link(name = "Metal", kind = "framework")]
extern "C" {
    /// Returns the default Metal device, or nil if Metal is not available.
    pub fn MTLCreateSystemDefaultDevice() -> Id;
}

// We also need Foundation for NSString, but it is auto-linked through Metal.
#[link(name = "Foundation", kind = "framework")]
extern "C" {}

// ---------------------------------------------------------------------------
// Typed objc_msgSend wrappers
//
// Rust cannot directly call variadic C functions with the correct ABI for
// each parameter list, so we transmute the single `objc_msgSend` symbol to
// the concrete function-pointer type required by each call site.
// ---------------------------------------------------------------------------

/// `[obj sel]` -> `Id`  (no extra args)
pub unsafe fn msg_send_id(obj: Id, sel: Sel) -> Id {
    let f: unsafe extern "C" fn(Id, Sel) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel)
}

/// `[obj sel:arg]` -> `Id`  (one Id arg)
pub unsafe fn msg_send_id_id(obj: Id, sel: Sel, arg: Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, arg)
}

/// `[obj sel:a1 :a2 :a3]` -> `Id`  (three Id args, e.g. newLibraryWithSource:options:error:)
pub unsafe fn msg_send_id_id_id_id(obj: Id, sel: Sel, a1: Id, a2: Id, a3: Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id, Id, Id) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, a1, a2, a3)
}

/// `[obj sel:arg :err]` -> `Id`  (Id + *mut Id, e.g. newComputePipelineStateWithFunction:error:)
pub unsafe fn msg_send_id_id_err(obj: Id, sel: Sel, arg: Id, err: *mut Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id, *mut Id) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, arg, err)
}

/// `[obj sel]` -> `NSUInteger`  (no args, returns unsigned integer)
pub unsafe fn msg_send_nsuinteger(obj: Id, sel: Sel) -> NSUInteger {
    let f: unsafe extern "C" fn(Id, Sel) -> NSUInteger =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel)
}

/// `[obj sel:n]` -> `Id`  (one NSUInteger arg)
pub unsafe fn msg_send_id_nsuint(obj: Id, sel: Sel, n: NSUInteger) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, NSUInteger) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, n)
}

/// `[obj sel:ptr :len :opts]` -> `Id`  (newBufferWithBytes:length:options:)
pub unsafe fn msg_send_new_buffer(
    obj: Id,
    sel: Sel,
    ptr: *const u8,
    len: NSUInteger,
    opts: NSUInteger,
) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const u8, NSUInteger, NSUInteger) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, ptr, len, opts)
}

/// `[obj sel:len :opts]` -> `Id`  (newBufferWithLength:options:)
pub unsafe fn msg_send_new_buffer_length(
    obj: Id,
    sel: Sel,
    len: NSUInteger,
    opts: NSUInteger,
) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, NSUInteger, NSUInteger) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, len, opts)
}

/// `[encoder setComputePipelineState:pso]`  (void return, one Id arg)
pub unsafe fn msg_send_void_id(obj: Id, sel: Sel, arg: Id) {
    let f: unsafe extern "C" fn(Id, Sel, Id) =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, arg);
}

/// `[encoder setBuffer:buf offset:off atIndex:idx]`
pub unsafe fn msg_send_set_buffer(
    obj: Id,
    sel: Sel,
    buf: Id,
    offset: NSUInteger,
    index: NSUInteger,
) {
    let f: unsafe extern "C" fn(Id, Sel, Id, NSUInteger, NSUInteger) =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, buf, offset, index);
}

/// `[encoder setBytes:ptr length:len atIndex:idx]`
pub unsafe fn msg_send_set_bytes(
    obj: Id,
    sel: Sel,
    ptr: *const u8,
    len: NSUInteger,
    index: NSUInteger,
) {
    let f: unsafe extern "C" fn(Id, Sel, *const u8, NSUInteger, NSUInteger) =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, ptr, len, index);
}

/// MTLSize is passed by value as three NSUInteger fields.
/// `[encoder dispatchThreadgroups:groups threadsPerThreadgroup:threads]`
pub unsafe fn msg_send_dispatch(
    obj: Id,
    sel: Sel,
    gx: NSUInteger,
    gy: NSUInteger,
    gz: NSUInteger,
    tx: NSUInteger,
    ty: NSUInteger,
    tz: NSUInteger,
) {
    let f: unsafe extern "C" fn(
        Id,
        Sel,
        NSUInteger,
        NSUInteger,
        NSUInteger,
        NSUInteger,
        NSUInteger,
        NSUInteger,
    ) = std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, gx, gy, gz, tx, ty, tz);
}

/// `[obj sel]` -> `void`  (no args, void return)
pub unsafe fn msg_send_void(obj: Id, sel: Sel) {
    let f: unsafe extern "C" fn(Id, Sel) = std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel);
}

/// `[obj sel]` -> `*mut c_void`  (e.g. [buffer contents])
pub unsafe fn msg_send_ptr(obj: Id, sel: Sel) -> *mut c_void {
    let f: unsafe extern "C" fn(Id, Sel) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel)
}

/// `[NSString stringWithUTF8String:cstr]` -> `Id`
pub unsafe fn msg_send_class_cstr(cls: Class, sel: Sel, cstr: *const i8) -> Id {
    let f: unsafe extern "C" fn(Class, Sel, *const i8) -> Id =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(cls, sel, cstr)
}

// ---------------------------------------------------------------------------
// Selector cache
// ---------------------------------------------------------------------------

/// Pre-registered Objective-C selectors used throughout the Metal backend.
///
/// Created once at backend init time to avoid repeated `sel_registerName` calls.
pub struct Selectors {
    // MTLDevice
    pub new_command_queue: Sel,
    /// `newLibraryWithSource:options:error:`
    pub new_library_with_source: Sel,
    /// `newFunctionWithName:`
    pub new_function_with_name: Sel,
    /// `newComputePipelineStateWithFunction:error:`
    pub new_compute_pipeline: Sel,
    /// `newBufferWithBytes:length:options:`
    pub new_buffer_with_bytes: Sel,
    /// `newBufferWithLength:options:`
    pub new_buffer_with_length: Sel,

    // MTLCommandQueue / MTLCommandBuffer / MTLComputeCommandEncoder
    pub command_buffer: Sel,
    pub compute_command_encoder: Sel,
    /// `setComputePipelineState:`
    pub set_compute_pipeline: Sel,
    /// `setBuffer:offset:atIndex:`
    pub set_buffer: Sel,
    /// `setBytes:length:atIndex:`
    pub set_bytes: Sel,
    /// `dispatchThreadgroups:threadsPerThreadgroup:`
    pub dispatch_threadgroups: Sel,
    pub end_encoding: Sel,
    pub commit: Sel,
    pub wait_until_completed: Sel,

    // MTLBuffer
    pub contents: Sel,
    pub length: Sel,

    // Memory management
    pub release: Sel,
    pub retain: Sel,

    // NSString
    /// `stringWithUTF8String:` (class method)
    pub string_with_utf8: Sel,
    pub utf8_string: Sel,
    /// `description` (for error reporting)
    pub description_sel: Sel,
}

impl Selectors {
    /// Register all selectors. Must be called from the main thread or after
    /// the Objective-C runtime has been initialized.
    pub unsafe fn new() -> Self {
        Self {
            new_command_queue: sel_registerName(b"newCommandQueue\0".as_ptr() as _),
            new_library_with_source: sel_registerName(
                b"newLibraryWithSource:options:error:\0".as_ptr() as _,
            ),
            new_function_with_name: sel_registerName(b"newFunctionWithName:\0".as_ptr() as _),
            new_compute_pipeline: sel_registerName(
                b"newComputePipelineStateWithFunction:error:\0".as_ptr() as _,
            ),
            new_buffer_with_bytes: sel_registerName(
                b"newBufferWithBytes:length:options:\0".as_ptr() as _,
            ),
            new_buffer_with_length: sel_registerName(
                b"newBufferWithLength:options:\0".as_ptr() as _,
            ),
            command_buffer: sel_registerName(b"commandBuffer\0".as_ptr() as _),
            compute_command_encoder: sel_registerName(
                b"computeCommandEncoder\0".as_ptr() as _,
            ),
            set_compute_pipeline: sel_registerName(
                b"setComputePipelineState:\0".as_ptr() as _,
            ),
            set_buffer: sel_registerName(b"setBuffer:offset:atIndex:\0".as_ptr() as _),
            set_bytes: sel_registerName(b"setBytes:length:atIndex:\0".as_ptr() as _),
            dispatch_threadgroups: sel_registerName(
                b"dispatchThreadgroups:threadsPerThreadgroup:\0".as_ptr() as _,
            ),
            end_encoding: sel_registerName(b"endEncoding\0".as_ptr() as _),
            commit: sel_registerName(b"commit\0".as_ptr() as _),
            wait_until_completed: sel_registerName(b"waitUntilCompleted\0".as_ptr() as _),
            contents: sel_registerName(b"contents\0".as_ptr() as _),
            length: sel_registerName(b"length\0".as_ptr() as _),
            release: sel_registerName(b"release\0".as_ptr() as _),
            retain: sel_registerName(b"retain\0".as_ptr() as _),
            string_with_utf8: sel_registerName(b"stringWithUTF8String:\0".as_ptr() as _),
            utf8_string: sel_registerName(b"UTF8String\0".as_ptr() as _),
            description_sel: sel_registerName(b"description\0".as_ptr() as _),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create an autoreleased `NSString` from a Rust `&str`.
///
/// The returned object is autoreleased — callers must retain it if they need
/// it to outlive the current autorelease pool scope.
pub unsafe fn ns_string(s: &str) -> Id {
    // We need a NUL-terminated C string.
    let mut buf = Vec::with_capacity(s.len() + 1);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
    let cls = objc_getClass(b"NSString\0".as_ptr() as _);
    let sel = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as _);
    msg_send_class_cstr(cls, sel, buf.as_ptr() as _)
}

/// Read the `-[NSObject description]` of an Objective-C object as a Rust `String`.
/// Useful for extracting `NSError` messages.
pub unsafe fn obj_description(obj: Id) -> String {
    if obj == NIL {
        return "<nil>".to_string();
    }
    let sel = sel_registerName(b"description\0".as_ptr() as _);
    let ns = msg_send_id(obj, sel);
    if ns == NIL {
        return "<nil description>".to_string();
    }
    let utf8_sel = sel_registerName(b"UTF8String\0".as_ptr() as _);
    let cstr = msg_send_ptr(ns, utf8_sel) as *const i8;
    if cstr.is_null() {
        return "<null UTF8String>".to_string();
    }
    std::ffi::CStr::from_ptr(cstr)
        .to_string_lossy()
        .into_owned()
}
