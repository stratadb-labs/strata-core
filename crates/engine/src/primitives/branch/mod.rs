//! Run module for run lifecycle management and handles
//!
//! This module contains:
//! - `index`: BranchIndex for creating, deleting, and managing runs
//! - `handle`: BranchHandle facade for branch-scoped operations

mod handle;
mod index;

pub use handle::{BranchHandle, EventHandle, JsonHandle, KvHandle, StateHandle, VectorHandle};
pub use index::{resolve_branch_name, BranchIndex, BranchMetadata, BranchStatus};
