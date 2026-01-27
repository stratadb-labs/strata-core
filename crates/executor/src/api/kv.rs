//! Key-value store operations.

use super::Strata;
use strata_core::Value;
use crate::{Command, Error, Output, Result};
use crate::types::*;

impl Strata {
    // =========================================================================
    // KV Operations (15)
    // =========================================================================

    /// Put a value in the KV store.
    pub fn kv_put(&self, key: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::KvPut {
            run: None,
            key: key.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvPut".into(),
            }),
        }
    }

    /// Get a value from the KV store.
    pub fn kv_get(&self, key: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::KvGet {
            run: None,
            key: key.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvGet".into(),
            }),
        }
    }

    /// Get a value at a specific version.
    pub fn kv_get_at(&self, key: &str, version: u64) -> Result<VersionedValue> {
        match self.executor.execute(Command::KvGetAt {
            run: None,
            key: key.to_string(),
            version,
        })? {
            Output::Versioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvGetAt".into(),
            }),
        }
    }

    /// Delete a key from the KV store.
    pub fn kv_delete(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::KvDelete {
            run: None,
            key: key.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvDelete".into(),
            }),
        }
    }

    /// Check if a key exists in the KV store.
    pub fn kv_exists(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::KvExists {
            run: None,
            key: key.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvExists".into(),
            }),
        }
    }

    /// Get version history for a key.
    pub fn kv_history(
        &self,
        key: &str,
        limit: Option<u64>,
        before: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::KvHistory {
            run: None,
            key: key.to_string(),
            limit,
            before,
        })? {
            Output::VersionedValues(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvHistory".into(),
            }),
        }
    }

    /// Increment a counter in the KV store.
    pub fn kv_incr(&self, key: &str, delta: i64) -> Result<i64> {
        match self.executor.execute(Command::KvIncr {
            run: None,
            key: key.to_string(),
            delta,
        })? {
            Output::Int(val) => Ok(val),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvIncr".into(),
            }),
        }
    }

    /// Compare-and-swap by version.
    pub fn kv_cas_version(
        &self,
        key: &str,
        expected_version: Option<u64>,
        new_value: Value,
    ) -> Result<bool> {
        match self.executor.execute(Command::KvCasVersion {
            run: None,
            key: key.to_string(),
            expected_version,
            new_value,
        })? {
            Output::Bool(ok) => Ok(ok),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvCasVersion".into(),
            }),
        }
    }

    /// Compare-and-swap by value.
    pub fn kv_cas_value(
        &self,
        key: &str,
        expected_value: Option<Value>,
        new_value: Value,
    ) -> Result<bool> {
        match self.executor.execute(Command::KvCasValue {
            run: None,
            key: key.to_string(),
            expected_value,
            new_value,
        })? {
            Output::Bool(ok) => Ok(ok),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvCasValue".into(),
            }),
        }
    }

    /// List keys with optional prefix filter.
    pub fn kv_keys(&self, prefix: &str, limit: Option<u64>) -> Result<Vec<String>> {
        match self.executor.execute(Command::KvKeys {
            run: None,
            prefix: prefix.to_string(),
            limit,
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvKeys".into(),
            }),
        }
    }

    /// Scan keys with cursor-based pagination.
    pub fn kv_scan(
        &self,
        prefix: &str,
        limit: u64,
        cursor: Option<String>,
    ) -> Result<(Vec<(String, VersionedValue)>, Option<String>)> {
        match self.executor.execute(Command::KvScan {
            run: None,
            prefix: prefix.to_string(),
            limit,
            cursor,
        })? {
            Output::KvScanResult { entries, cursor } => Ok((entries, cursor)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvScan".into(),
            }),
        }
    }

    /// Get multiple values from the KV store.
    pub fn kv_mget(&self, keys: Vec<String>) -> Result<Vec<Option<VersionedValue>>> {
        match self.executor.execute(Command::KvMget {
            run: None,
            keys,
        })? {
            Output::Values(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMget".into(),
            }),
        }
    }

    /// Put multiple values in the KV store.
    pub fn kv_mput(&self, entries: Vec<(String, Value)>) -> Result<u64> {
        match self.executor.execute(Command::KvMput {
            run: None,
            entries,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMput".into(),
            }),
        }
    }

    /// Delete multiple keys.
    pub fn kv_mdelete(&self, keys: Vec<String>) -> Result<u64> {
        match self.executor.execute(Command::KvMdelete {
            run: None,
            keys,
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMdelete".into(),
            }),
        }
    }

    /// Check existence of multiple keys.
    pub fn kv_mexists(&self, keys: Vec<String>) -> Result<u64> {
        match self.executor.execute(Command::KvMexists {
            run: None,
            keys,
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMexists".into(),
            }),
        }
    }
}
