//! Transaction management for M4 performance
//!
//! This module provides:
//! - Thread-local transaction pooling (zero allocations after warmup)
//! - Pool management utilities
//!
//! # Architecture
//!
//! The pool uses thread-local storage to avoid synchronization overhead:
//! - Each thread has its own pool of up to 8 TransactionContext objects
//! - Contexts are reset (not reallocated) when reused
//! - HashMap/HashSet capacity is preserved across reuse

pub mod pool;

pub use pool::{TransactionPool, MAX_POOL_SIZE};
