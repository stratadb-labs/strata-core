//! Key-value store operations.

use super::Strata;
use crate::{Command, Error, Output, Result, Value};

impl Strata {
    // =========================================================================
    // KV Operations (4 MVP)
    // =========================================================================

    /// Put a value in the KV store.
    ///
    /// Creates the key if it doesn't exist, overwrites if it does.
    /// Returns the version created by this write operation.
    ///
    /// Accepts any type that implements `Into<Value>`:
    /// - `&str`, `String` → `Value::String`
    /// - `i32`, `i64` → `Value::Int`
    /// - `f32`, `f64` → `Value::Float`
    /// - `bool` → `Value::Bool`
    /// - `Vec<u8>`, `&[u8]` → `Value::Bytes`
    ///
    /// # Example
    ///
    /// ```ignore
    /// db.kv_put("name", "Alice")?;
    /// db.kv_put("age", 30i64)?;
    /// db.kv_put("score", 95.5)?;
    /// db.kv_put("active", true)?;
    /// ```
    pub fn kv_put(&self, key: &str, value: impl Into<Value>) -> Result<u64> {
        match self.executor.execute(Command::KvPut {
            run: self.branch_id(),
            key: key.to_string(),
            value: value.into(),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvPut".into(),
            }),
        }
    }

    /// Get a value from the KV store.
    ///
    /// Returns the latest value for the key, or None if it doesn't exist.
    ///
    /// Reads from the current run context.
    pub fn kv_get(&self, key: &str) -> Result<Option<Value>> {
        match self.executor.execute(Command::KvGet {
            run: self.branch_id(),
            key: key.to_string(),
        })? {
            Output::Maybe(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvGet".into(),
            }),
        }
    }

    /// Delete a key from the KV store.
    ///
    /// Returns `true` if the key existed and was deleted, `false` if it didn't exist.
    ///
    /// Deletes from the current run context.
    pub fn kv_delete(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::KvDelete {
            run: self.branch_id(),
            key: key.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvDelete".into(),
            }),
        }
    }

    /// List keys with optional prefix filter.
    ///
    /// Returns all keys matching the prefix (or all keys if prefix is None).
    ///
    /// Lists from the current run context.
    pub fn kv_list(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        match self.executor.execute(Command::KvList {
            run: self.branch_id(),
            prefix: prefix.map(|s| s.to_string()),
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvList".into(),
            }),
        }
    }
}
