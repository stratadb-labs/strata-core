# Production Readiness Checklist for Rust Files

This document defines best practices and standards for making each Rust file production-ready in the Strata codebase.

---

## Table of Contents

1. [File Documentation](#1-file-documentation)
2. [Method Documentation](#2-method-documentation)
3. [Error Handling](#3-error-handling)
4. [Logging](#4-logging)
5. [Code Organization](#5-code-organization)
6. [Safety & Robustness](#6-safety--robustness)
7. [Performance Considerations](#7-performance-considerations)
8. [File-by-File Checklist](#8-file-by-file-checklist)

---

## 1. File Documentation

Every Rust file should have module-level documentation at the top.

### Checklist

- [ ] **Module doc comment** (`//!`) at the top of the file
- [ ] **Purpose statement**: What this module does in 1-2 sentences
- [ ] **API overview**: List of main types/functions provided
- [ ] **Usage example**: A practical code snippet showing common usage
- [ ] **Related modules**: Links to related modules if applicable

### Template

```rust
//! # Module Name
//!
//! Brief description of what this module provides.
//!
//! ## Overview
//!
//! - `TypeA` - Description of TypeA
//! - `TypeB` - Description of TypeB
//! - `function_x()` - Description of function_x
//!
//! ## Example
//!
//! ```ignore
//! use crate::module_name::TypeA;
//!
//! let item = TypeA::new();
//! item.do_something()?;
//! ```
//!
//! ## See Also
//!
//! - [`related_module`] - For related functionality
```

### Current Status

| File | Has Module Docs | Has Example | Has Overview |
|------|-----------------|-------------|--------------|
| `lib.rs` | Yes | Yes | Yes |
| `prelude.rs` | Yes | Yes | Yes |
| `error.rs` | Yes | No | Yes |
| `types.rs` | Yes | No | Yes |
| `database.rs` | Yes | Yes | Yes |
| `primitives/mod.rs` | Yes | No | Yes |
| `primitives/kv.rs` | Yes | Yes | Yes |
| `primitives/json.rs` | Yes | Yes | Yes |
| `primitives/events.rs` | Yes | Yes | Yes |
| `primitives/state.rs` | Yes | Yes | Yes |
| `primitives/vectors.rs` | Yes | Yes | Yes |
| `primitives/runs.rs` | Yes | Yes | Yes |

---

## 2. Method Documentation

Every public method should be documented with doc comments.

### Checklist

- [ ] **Summary line**: Brief description of what the method does
- [ ] **Detailed description**: Additional context if behavior is non-obvious
- [ ] **Parameters**: Document each parameter's purpose and valid values
- [ ] **Returns**: Document what the method returns, including error cases
- [ ] **Example**: Code snippet showing typical usage
- [ ] **Panics**: Document any panic conditions (should be rare/none)
- [ ] **Errors**: Document error conditions that can occur

### Template

```rust
/// Brief summary of what this method does.
///
/// More detailed explanation if needed. Describe any important
/// behavior, side effects, or constraints.
///
/// # Arguments
///
/// * `arg1` - Description of first argument
/// * `arg2` - Description of second argument
///
/// # Returns
///
/// Description of return value. For `Result` types, describe both
/// success and error cases.
///
/// # Example
///
/// ```ignore
/// let result = object.method("arg1", 42)?;
/// assert_eq!(result, expected);
/// ```
///
/// # Errors
///
/// Returns an error if:
/// - Condition A occurs
/// - Condition B occurs
pub fn method(&self, arg1: &str, arg2: i32) -> Result<Output> {
    // implementation
}
```

### Documentation Levels by Complexity

| Method Complexity | Required Documentation |
|-------------------|------------------------|
| Simple getter/setter | Summary line only |
| Standard operation | Summary + Returns + Example |
| Complex logic | Full documentation with Errors section |
| Unsafe or panic-possible | Full docs + Panics/Safety sections |

---

## 3. Error Handling

Consistent error handling is critical for production code.

### Checklist

- [ ] **Use `Result<T>` for fallible operations**: Never panic on recoverable errors
- [ ] **Use the unified `Error` type**: All errors should map to `crate::Error`
- [ ] **Provide context in errors**: Include relevant data (key names, versions, etc.)
- [ ] **Categorize errors correctly**: Use appropriate error variants
- [ ] **No `unwrap()` or `expect()` in library code**: Use `?` operator
- [ ] **Document error conditions**: In method docs under `# Errors`

### Error Categories

| Variant | When to Use |
|---------|-------------|
| `NotFound` | Resource doesn't exist |
| `WrongType` | Type mismatch (expected X, got Y) |
| `InvalidKey` | Key format/validation failure |
| `InvalidPath` | JSON path format error |
| `Conflict` | Version/CAS conflict |
| `ConstraintViolation` | Business rule violation |
| `RunError` | Run lifecycle error |
| `Io` | File system errors |
| `Serialization` | JSON/data format errors |
| `Storage` | Storage layer errors |
| `Internal` | Unexpected internal errors |

### Error Handling Patterns

```rust
// GOOD: Propagate with context
pub fn get(&self, key: &str) -> Result<Option<Value>> {
    self.inner.get(key).map_err(Error::from)
}

// GOOD: Add context to errors
fn validate_key(key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(Error::InvalidKey("key cannot be empty".into()));
    }
    Ok(())
}

// BAD: Don't use unwrap in library code
fn bad_example(key: &str) -> Value {
    self.inner.get(key).unwrap()  // Don't do this!
}
```

### Error Conversion

When converting from external error types:

```rust
impl From<ExternalError> for Error {
    fn from(e: ExternalError) -> Self {
        match e {
            ExternalError::NotFound(msg) => Error::NotFound(msg),
            ExternalError::Other(msg) => Error::Internal(msg),
        }
    }
}
```

---

## 4. Logging

Logging strategy for the library vs application code.

### Library Logging Philosophy

For **library code** (like `src/`):
- [ ] **Minimal logging**: Libraries should be quiet by default
- [ ] **Use `tracing` crate**: For structured, leveled logging
- [ ] **Debug/Trace only**: Most logs should be debug or trace level
- [ ] **No stdout/stderr**: Never use `println!` or `eprintln!`

For **application/binary code**:
- [ ] **Configure log levels**: Set appropriate levels per module
- [ ] **Structured logging**: Use key-value pairs for searchability
- [ ] **Error logging**: Log errors with full context

### Logging Levels

| Level | When to Use |
|-------|-------------|
| `error!` | Unrecoverable errors, data corruption |
| `warn!` | Recoverable errors, degraded operation |
| `info!` | Significant state changes, startup/shutdown |
| `debug!` | Detailed operation info, useful for debugging |
| `trace!` | Very detailed, per-operation logging |

### Implementation Pattern

```rust
use tracing::{debug, error, info, trace, warn, instrument};

/// Function with automatic span creation
#[instrument(skip(self), fields(key = %key))]
pub fn get(&self, key: &str) -> Result<Option<Value>> {
    trace!("looking up key");

    match self.inner.get(key) {
        Ok(value) => {
            debug!(found = value.is_some(), "get completed");
            Ok(value)
        }
        Err(e) => {
            warn!(error = %e, "get failed");
            Err(e.into())
        }
    }
}
```

### Logging Checklist

- [ ] Add `tracing` to dependencies (if not present)
- [ ] Add `#[instrument]` to public methods for automatic spans
- [ ] Log at appropriate levels (see table above)
- [ ] Include relevant context as structured fields
- [ ] Never log sensitive data (passwords, tokens, PII)

### Current Status

The public API (`src/`) currently has **no logging**, which is acceptable for a thin wrapper library. Consider adding `tracing` instrumentation to internal crates (`crates/*`) for debugging.

---

## 5. Code Organization

Consistent structure within files.

### Checklist

- [ ] **Imports organized**: std, external crates, internal crates, local modules
- [ ] **Section comments**: Clear headers for logical groups
- [ ] **Consistent ordering**: Types, then impls, then functions
- [ ] **Related items grouped**: Keep related methods together

### Import Organization

```rust
// Standard library
use std::collections::HashMap;
use std::sync::Arc;

// External crates
use serde::{Deserialize, Serialize};
use thiserror::Error;

// Internal crates (workspace)
use strata_core::types::Value;
use strata_engine::Database;

// Local modules
use crate::error::{Error, Result};
use crate::primitives::KV;
```

### Section Headers

```rust
// =========================================================================
// Types
// =========================================================================

pub struct MyType { ... }

// =========================================================================
// Construction
// =========================================================================

impl MyType {
    pub fn new() -> Self { ... }
    pub fn with_config(config: Config) -> Self { ... }
}

// =========================================================================
// Simple API
// =========================================================================

impl MyType {
    pub fn get(&self, key: &str) -> Result<Value> { ... }
    pub fn set(&self, key: &str, value: Value) -> Result<()> { ... }
}

// =========================================================================
// Advanced API
// =========================================================================

impl MyType {
    pub fn get_with_version(&self, key: &str) -> Result<Versioned<Value>> { ... }
}

// =========================================================================
// Private Helpers
// =========================================================================

impl MyType {
    fn validate_key(&self, key: &str) -> Result<()> { ... }
}
```

---

## 6. Safety & Robustness

Production code must be safe and robust.

### Checklist

- [ ] **No `unsafe` code**: Unless absolutely necessary and well-documented
- [ ] **No `unwrap()` or `expect()`**: Use `?` or handle explicitly
- [ ] **No `panic!()` in library code**: Return errors instead
- [ ] **Validate inputs at boundaries**: Check early, fail fast
- [ ] **Handle all match arms**: No `_ => unreachable!()`
- [ ] **Thread safety**: Use `Arc`, `Mutex`, `RwLock` appropriately

### Input Validation Pattern

```rust
pub fn set(&self, key: &str, value: Value) -> Result<()> {
    // Validate at entry point
    self.validate_key(key)?;
    self.validate_value(&value)?;

    // Proceed with validated inputs
    self.inner.set(key, value).map_err(Error::from)
}

fn validate_key(&self, key: &str) -> Result<()> {
    if key.is_empty() {
        return Err(Error::InvalidKey("key cannot be empty".into()));
    }
    if key.len() > MAX_KEY_LENGTH {
        return Err(Error::InvalidKey(format!(
            "key exceeds maximum length of {} bytes",
            MAX_KEY_LENGTH
        )));
    }
    Ok(())
}
```

### Current Status

| Check | Status |
|-------|--------|
| No `unwrap()` in src/ | Pass |
| No `expect()` in src/ | Pass |
| No `panic!()` in src/ | Pass |
| No `unsafe` in src/ | Pass |
| All `Result` returns | Pass |

---

## 7. Performance Considerations

Document performance characteristics.

### Checklist

- [ ] **Document complexity**: Note O(n) vs O(1) operations in docs
- [ ] **Batch operations**: Provide batch APIs for bulk operations
- [ ] **Avoid unnecessary allocations**: Use references where possible
- [ ] **Consider async**: For I/O-bound operations

### Performance Documentation Pattern

```rust
/// Get multiple values in a single operation.
///
/// This is more efficient than calling `get()` multiple times
/// as it batches the underlying storage operations.
///
/// # Performance
///
/// - Time complexity: O(n) where n is the number of keys
/// - Single storage round-trip regardless of key count
///
/// # Example
///
/// ```ignore
/// let values = db.kv.mget(&["key1", "key2", "key3"])?;
/// ```
pub fn mget(&self, keys: &[&str]) -> Result<Vec<Option<Versioned<Value>>>> {
    // implementation
}
```

---

## 8. File-by-File Checklist

Use this checklist when reviewing each file for production readiness.

### Quick Reference Checklist

```
[ ] FILE DOCUMENTATION
    [ ] Module doc comment (//!) present
    [ ] Purpose clearly stated
    [ ] Usage example included
    [ ] Related modules linked

[ ] METHOD DOCUMENTATION
    [ ] All public methods documented
    [ ] Parameters documented
    [ ] Return values documented
    [ ] Examples for complex methods
    [ ] Error conditions documented

[ ] ERROR HANDLING
    [ ] Uses Result<T> for fallible ops
    [ ] Uses unified Error type
    [ ] No unwrap/expect
    [ ] Errors include context

[ ] LOGGING (if applicable)
    [ ] Uses tracing crate
    [ ] Appropriate log levels
    [ ] No println!/eprintln!
    [ ] Sensitive data not logged

[ ] CODE ORGANIZATION
    [ ] Imports organized
    [ ] Section headers present
    [ ] Consistent structure

[ ] SAFETY
    [ ] No unsafe code
    [ ] Input validation
    [ ] Thread-safe if needed
```

### Per-File Status

| File | Docs | Methods | Errors | Logging | Safety |
|------|------|---------|--------|---------|--------|
| `lib.rs` | Done | Done | N/A | N/A | Done |
| `prelude.rs` | Done | N/A | N/A | N/A | Done |
| `error.rs` | Done | Done | Done | N/A | Done |
| `types.rs` | Done | N/A | N/A | N/A | Done |
| `database.rs` | Done | Done | Done | Pending | Done |
| `primitives/mod.rs` | Done | N/A | N/A | N/A | Done |
| `primitives/kv.rs` | Done | Done | Done | Pending | Done |
| `primitives/json.rs` | Done | Done | Done | Pending | Done |
| `primitives/events.rs` | Done | Done | Done | Pending | Done |
| `primitives/state.rs` | Done | Done | Done | Pending | Done |
| `primitives/vectors.rs` | Done | Done | Done | Pending | Done |
| `primitives/runs.rs` | Done | Done | Done | Pending | Done |

**Legend**: Done = Complete, Pending = Needs work, N/A = Not applicable

---

## Summary

The Strata codebase is already well-documented and follows many best practices:

### Strengths
- Excellent file and method documentation
- Unified error handling with proper conversions
- No unsafe code or panics in public API
- Clean, consistent code organization

### Areas for Enhancement
1. **Logging**: Add `tracing` instrumentation for debugging
2. **Performance docs**: Document complexity for batch operations

### Priority Order
1. Add tracing instrumentation (helps debugging)
2. Document performance characteristics (nice to have)

---

*Last updated: 2026-01-25*
