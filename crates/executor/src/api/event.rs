//! Event log operations (4 MVP).
//!
//! MVP: append, read, read_by_type, len

use super::Strata;
use crate::{Command, Error, Output, Result, Value};
use crate::types::*;

impl Strata {
    // =========================================================================
    // Event Operations (4 MVP)
    // =========================================================================

    /// Append an event to the log.
    pub fn event_append(&self, event_type: &str, payload: Value) -> Result<u64> {
        match self.executor.execute(Command::EventAppend {
            run: self.branch_id(),
            event_type: event_type.to_string(),
            payload,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventAppend".into(),
            }),
        }
    }

    /// Read a specific event by sequence number.
    pub fn event_read(&self, sequence: u64) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::EventRead {
            run: self.branch_id(),
            sequence,
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRead".into(),
            }),
        }
    }

    /// Read all events of a specific type.
    pub fn event_read_by_type(&self, event_type: &str) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::EventReadByType {
            run: self.branch_id(),
            event_type: event_type.to_string(),
        })? {
            Output::VersionedValues(events) => Ok(events),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventReadByType".into(),
            }),
        }
    }

    /// Get the total count of events in the log.
    pub fn event_len(&self) -> Result<u64> {
        match self.executor.execute(Command::EventLen {
            run: self.branch_id(),
        })? {
            Output::Uint(len) => Ok(len),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventLen".into(),
            }),
        }
    }
}
