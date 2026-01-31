//! Run module for run lifecycle management and handles
//!
//! This module contains:
//! - `index`: BranchIndex for creating, deleting, and managing runs
//! - `handle`: BranchHandle facade for branch-scoped operations

mod index;
mod handle;

pub use index::{BranchIndex, BranchMetadata, BranchStatus};
pub use handle::{BranchHandle, EventHandle, JsonHandle, KvHandle, StateHandle, VectorHandle};
