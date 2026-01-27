//! State cell operations.

use super::Strata;
use strata_core::Value;
use crate::{Command, Error, Output, Result};
use crate::types::*;

impl Strata {
    // =========================================================================
    // State Operations (8)
    // =========================================================================

    /// Set a state cell value.
    pub fn state_set(&self, cell: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::StateSet {
            run: None,
            cell: cell.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateSet".into(),
            }),
        }
    }

    /// Get a state cell value.
    pub fn state_read(&self, cell: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::StateRead {
            run: None,
            cell: cell.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateRead".into(),
            }),
        }
    }

    /// Compare-and-swap on a state cell.
    pub fn state_cas(
        &self,
        cell: &str,
        expected_counter: Option<u64>,
        value: Value,
    ) -> Result<Option<u64>> {
        match self.executor.execute(Command::StateCas {
            run: None,
            cell: cell.to_string(),
            expected_counter,
            value,
        })? {
            Output::MaybeVersion(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateCas".into(),
            }),
        }
    }

    /// Delete a state cell.
    pub fn state_delete(&self, cell: &str) -> Result<bool> {
        match self.executor.execute(Command::StateDelete {
            run: None,
            cell: cell.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateDelete".into(),
            }),
        }
    }

    /// Check if a state cell exists.
    pub fn state_exists(&self, cell: &str) -> Result<bool> {
        match self.executor.execute(Command::StateExists {
            run: None,
            cell: cell.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateExists".into(),
            }),
        }
    }

    /// Get version history for a state cell.
    pub fn state_history(
        &self,
        cell: &str,
        limit: Option<u64>,
        before: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::StateHistory {
            run: None,
            cell: cell.to_string(),
            limit,
            before,
        })? {
            Output::VersionedValues(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateHistory".into(),
            }),
        }
    }

    /// Initialize a state cell (only if it doesn't exist).
    pub fn state_init(&self, cell: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::StateInit {
            run: None,
            cell: cell.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateInit".into(),
            }),
        }
    }

    /// List all state cell names.
    pub fn state_list(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::StateList {
            run: None,
        })? {
            Output::Strings(names) => Ok(names),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateList".into(),
            }),
        }
    }
}
