//! Event log / stream operations.

use super::Strata;
use crate::{Command, Error, Output, Result, Value};
use crate::types::*;

impl Strata {
    // =========================================================================
    // Event Operations (11)
    // =========================================================================

    /// Append an event to a stream.
    pub fn event_append(&self, stream: &str, payload: Value) -> Result<u64> {
        match self.executor.execute(Command::EventAppend {
            run: self.run_id(),
            stream: stream.to_string(),
            payload,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventAppend".into(),
            }),
        }
    }

    /// Append multiple events atomically.
    pub fn event_append_batch(&self, events: Vec<(String, Value)>) -> Result<Vec<u64>> {
        match self.executor.execute(Command::EventAppendBatch {
            run: self.run_id(),
            events,
        })? {
            Output::Versions(versions) => Ok(versions),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventAppendBatch".into(),
            }),
        }
    }

    /// Get events from a stream in a range.
    pub fn event_range(
        &self,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::EventRange {
            run: self.run_id(),
            stream: stream.to_string(),
            start,
            end,
            limit,
        })? {
            Output::VersionedValues(events) => Ok(events),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRange".into(),
            }),
        }
    }

    /// Get a specific event by sequence number.
    pub fn event_read(&self, stream: &str, sequence: u64) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::EventRead {
            run: self.run_id(),
            stream: stream.to_string(),
            sequence,
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRead".into(),
            }),
        }
    }

    /// Get the count of events in a stream.
    pub fn event_len(&self, stream: &str) -> Result<u64> {
        match self.executor.execute(Command::EventLen {
            run: self.run_id(),
            stream: stream.to_string(),
        })? {
            Output::Uint(len) => Ok(len),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventLen".into(),
            }),
        }
    }

    /// Get the latest sequence number in a stream.
    pub fn event_latest_sequence(&self, stream: &str) -> Result<Option<u64>> {
        match self.executor.execute(Command::EventLatestSequence {
            run: self.run_id(),
            stream: stream.to_string(),
        })? {
            Output::MaybeVersion(seq) => Ok(seq),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventLatestSequence".into(),
            }),
        }
    }

    /// Get stream metadata.
    pub fn event_stream_info(&self, stream: &str) -> Result<StreamInfo> {
        match self.executor.execute(Command::EventStreamInfo {
            run: self.run_id(),
            stream: stream.to_string(),
        })? {
            Output::StreamInfo(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventStreamInfo".into(),
            }),
        }
    }

    /// Read events from a stream in descending order (newest first).
    pub fn event_rev_range(
        &self,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::EventRevRange {
            run: self.run_id(),
            stream: stream.to_string(),
            start,
            end,
            limit,
        })? {
            Output::VersionedValues(events) => Ok(events),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRevRange".into(),
            }),
        }
    }

    /// List all event streams.
    pub fn event_streams(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::EventStreams {
            run: self.run_id(),
        })? {
            Output::Strings(streams) => Ok(streams),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventStreams".into(),
            }),
        }
    }

    /// Get the latest event (head) of a stream.
    pub fn event_head(&self, stream: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::EventHead {
            run: self.run_id(),
            stream: stream.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventHead".into(),
            }),
        }
    }

    /// Verify the hash chain integrity of the event log.
    pub fn event_verify_chain(&self) -> Result<ChainVerificationResult> {
        match self.executor.execute(Command::EventVerifyChain {
            run: self.run_id(),
        })? {
            Output::ChainVerification(result) => Ok(result),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventVerifyChain".into(),
            }),
        }
    }
}
