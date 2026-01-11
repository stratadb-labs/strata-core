//! Concurrency layer for in-mem
//!
//! This crate implements optimistic concurrency control (OCC) with:
//! - TransactionContext: Read/write set tracking
//! - Snapshot isolation
//! - Conflict detection at commit time
//! - Compare-and-swap (CAS) operations

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod transaction;
// pub mod snapshot;    // Story #80
// pub mod validation;  // Story #83

pub use transaction::{CASOperation, TransactionContext, TransactionStatus};
