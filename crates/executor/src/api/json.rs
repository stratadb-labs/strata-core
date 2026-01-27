//! JSON document store operations.

use super::Strata;
use strata_core::Value;
use crate::{Command, Error, Output, Result};
use crate::types::*;

impl Strata {
    // =========================================================================
    // JSON Operations (17)
    // =========================================================================

    /// Set a JSON value at a path.
    pub fn json_set(&self, key: &str, path: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::JsonSet {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonSet".into(),
            }),
        }
    }

    /// Get a JSON value at a path.
    pub fn json_get(&self, key: &str, path: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::JsonGet {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonGet".into(),
            }),
        }
    }

    /// Delete a value at a path from a JSON document.
    pub fn json_delete(&self, key: &str, path: &str) -> Result<u64> {
        match self.executor.execute(Command::JsonDelete {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonDelete".into(),
            }),
        }
    }

    /// Merge a value at a path (RFC 7396 JSON Merge Patch).
    pub fn json_merge(&self, key: &str, path: &str, patch: Value) -> Result<u64> {
        match self.executor.execute(Command::JsonMerge {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            patch,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonMerge".into(),
            }),
        }
    }

    /// Get version history for a JSON document.
    pub fn json_history(
        &self,
        key: &str,
        limit: Option<u64>,
        before: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::JsonHistory {
            run: None,
            key: key.to_string(),
            limit,
            before,
        })? {
            Output::VersionedValues(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonHistory".into(),
            }),
        }
    }

    /// Check if a JSON document exists.
    pub fn json_exists(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::JsonExists {
            run: None,
            key: key.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonExists".into(),
            }),
        }
    }

    /// Get the current version of a JSON document.
    pub fn json_get_version(&self, key: &str) -> Result<Option<u64>> {
        match self.executor.execute(Command::JsonGetVersion {
            run: None,
            key: key.to_string(),
        })? {
            Output::MaybeVersion(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonGetVersion".into(),
            }),
        }
    }

    /// Full-text search across JSON documents.
    pub fn json_search(&self, query: &str, k: u64) -> Result<Vec<JsonSearchHit>> {
        match self.executor.execute(Command::JsonSearch {
            run: None,
            query: query.to_string(),
            k,
        })? {
            Output::JsonSearchHits(hits) => Ok(hits),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonSearch".into(),
            }),
        }
    }

    /// List JSON documents with cursor-based pagination.
    pub fn json_list(
        &self,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: u64,
    ) -> Result<(Vec<String>, Option<String>)> {
        match self.executor.execute(Command::JsonList {
            run: None,
            prefix,
            cursor,
            limit,
        })? {
            Output::JsonListResult { keys, cursor } => Ok((keys, cursor)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonList".into(),
            }),
        }
    }

    /// Compare-and-swap: update if version matches.
    pub fn json_cas(
        &self,
        key: &str,
        expected_version: u64,
        path: &str,
        value: Value,
    ) -> Result<u64> {
        match self.executor.execute(Command::JsonCas {
            run: None,
            key: key.to_string(),
            expected_version,
            path: path.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonCas".into(),
            }),
        }
    }

    /// Query documents by exact field match.
    pub fn json_query(&self, path: &str, value: Value, limit: u64) -> Result<Vec<String>> {
        match self.executor.execute(Command::JsonQuery {
            run: None,
            path: path.to_string(),
            value,
            limit,
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonQuery".into(),
            }),
        }
    }

    /// Count JSON documents in the store.
    pub fn json_count(&self) -> Result<u64> {
        match self.executor.execute(Command::JsonCount {
            run: None,
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonCount".into(),
            }),
        }
    }

    /// Batch get multiple JSON documents.
    pub fn json_batch_get(&self, keys: Vec<String>) -> Result<Vec<Option<VersionedValue>>> {
        match self.executor.execute(Command::JsonBatchGet {
            run: None,
            keys,
        })? {
            Output::Values(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonBatchGet".into(),
            }),
        }
    }

    /// Batch create multiple JSON documents atomically.
    pub fn json_batch_create(&self, docs: Vec<(String, Value)>) -> Result<Vec<u64>> {
        match self.executor.execute(Command::JsonBatchCreate {
            run: None,
            docs,
        })? {
            Output::Versions(versions) => Ok(versions),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonBatchCreate".into(),
            }),
        }
    }

    /// Atomically push values to an array at path.
    pub fn json_array_push(&self, key: &str, path: &str, values: Vec<Value>) -> Result<u64> {
        match self.executor.execute(Command::JsonArrayPush {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            values,
        })? {
            Output::Uint(len) => Ok(len),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonArrayPush".into(),
            }),
        }
    }

    /// Atomically increment a numeric value at path.
    pub fn json_increment(&self, key: &str, path: &str, delta: f64) -> Result<f64> {
        match self.executor.execute(Command::JsonIncrement {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            delta,
        })? {
            Output::Float(val) => Ok(val),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonIncrement".into(),
            }),
        }
    }

    /// Atomically pop a value from an array at path.
    pub fn json_array_pop(&self, key: &str, path: &str) -> Result<Option<Value>> {
        match self.executor.execute(Command::JsonArrayPop {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::Maybe(val) => Ok(val),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonArrayPop".into(),
            }),
        }
    }
}
