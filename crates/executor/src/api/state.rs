//! State cell operations (4 MVP).
//!
//! MVP: set, read, cas, init

use super::Strata;
use crate::{Command, Error, Output, Result, Value};

impl Strata {
    // =========================================================================
    // State Operations (4 MVP)
    // =========================================================================

    /// Set a state cell value (unconditional write).
    pub fn state_set(&self, cell: &str, value: impl Into<Value>) -> Result<u64> {
        match self.executor.execute(Command::StateSet {
            run: self.branch_id(),
            cell: cell.to_string(),
            value: value.into(),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateSet".into(),
            }),
        }
    }

    /// Read a state cell value.
    pub fn state_read(&self, cell: &str) -> Result<Option<Value>> {
        match self.executor.execute(Command::StateRead {
            run: self.branch_id(),
            cell: cell.to_string(),
        })? {
            Output::Maybe(v) => Ok(v),
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
        value: impl Into<Value>,
    ) -> Result<Option<u64>> {
        match self.executor.execute(Command::StateCas {
            run: self.branch_id(),
            cell: cell.to_string(),
            expected_counter,
            value: value.into(),
        })? {
            Output::MaybeVersion(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateCas".into(),
            }),
        }
    }

    /// Initialize a state cell (only if it doesn't exist).
    pub fn state_init(&self, cell: &str, value: impl Into<Value>) -> Result<u64> {
        match self.executor.execute(Command::StateInit {
            run: self.branch_id(),
            cell: cell.to_string(),
            value: value.into(),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateInit".into(),
            }),
        }
    }
}
