//! Space management command handlers.

use std::sync::Arc;

use crate::bridge::Primitives;
use crate::types::BranchId;
use crate::{Output, Result};

/// Handle SpaceList command.
pub fn space_list(_p: &Arc<Primitives>, _branch: BranchId) -> Result<Output> {
    // TODO: Implement space listing from storage
    Ok(Output::SpaceList(vec!["default".to_string()]))
}

/// Handle SpaceCreate command.
pub fn space_create(_p: &Arc<Primitives>, _branch: BranchId, _space: String) -> Result<Output> {
    // TODO: Implement space creation
    Ok(Output::Unit)
}

/// Handle SpaceDelete command.
pub fn space_delete(
    _p: &Arc<Primitives>,
    _branch: BranchId,
    _space: String,
    _force: bool,
) -> Result<Output> {
    // TODO: Implement space deletion
    Ok(Output::Unit)
}

/// Handle SpaceExists command.
pub fn space_exists(_p: &Arc<Primitives>, _branch: BranchId, _space: String) -> Result<Output> {
    // Default space always exists
    if _space == "default" {
        Ok(Output::Bool(true))
    } else {
        // TODO: Check storage for space existence
        Ok(Output::Bool(false))
    }
}
