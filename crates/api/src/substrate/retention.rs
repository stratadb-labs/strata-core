//! Retention Substrate - Retention policy operations
//!
//! This module provides substrate-level operations for managing retention policies.
//!
//! ## Retention Policies
//!
//! Strata supports flexible retention policies that control version history:
//!
//! - `KeepAll`: Keep all versions indefinitely (default)
//! - `KeepLast(n)`: Keep the N most recent versions
//! - `KeepFor(duration)`: Keep versions within the time window
//! - `Composite`: Union of multiple policies (most permissive wins)
//!
//! ## Scope
//!
//! - Retention is configured per-run
//! - Per-key retention is NOT supported in M11
//! - Retention applies to all primitives within a run

use strata_core::StrataResult;
use crate::substrate::types::{ApiRunId, RetentionPolicy};

/// Version information for retention policy
#[derive(Debug, Clone)]
pub struct RetentionVersion {
    /// The retention policy
    pub policy: RetentionPolicy,
    /// Version number when this policy was set
    pub version: u64,
    /// Timestamp when this policy was set (microseconds)
    pub timestamp: u64,
}

/// Retention Substrate - retention policy operations
///
/// All operations require explicit `run_id` parameter.
///
/// ## Design
///
/// - Retention is configured at the run level
/// - Per-key retention is not supported in M11
/// - The default policy is `KeepAll`
/// - Changing retention policy does not immediately trigger garbage collection
pub trait RetentionSubstrate {
    /// Get the retention policy for a run
    ///
    /// Returns `None` if no explicit policy is set (defaults apply).
    ///
    /// ## Parameters
    ///
    /// - `run`: The run to query retention for
    ///
    /// ## Returns
    ///
    /// The current retention policy with version info, or `None` if
    /// using the default policy.
    fn retention_get(&self, run: &ApiRunId) -> StrataResult<Option<RetentionVersion>>;

    /// Set the retention policy for a run
    ///
    /// ## Parameters
    ///
    /// - `run`: The run to set retention for
    /// - `policy`: The retention policy to apply
    ///
    /// ## Returns
    ///
    /// The version number of the policy update.
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // Keep last 100 versions
    /// substrate.retention_set(&run, RetentionPolicy::KeepLast(100))?;
    ///
    /// // Keep versions from the last 7 days
    /// substrate.retention_set(&run, RetentionPolicy::KeepFor(Duration::from_secs(7 * 24 * 3600)))?;
    ///
    /// // Composite: keep last 10 OR anything from last hour
    /// substrate.retention_set(&run, RetentionPolicy::Composite(vec![
    ///     RetentionPolicy::KeepLast(10),
    ///     RetentionPolicy::KeepFor(Duration::from_secs(3600)),
    /// ]))?;
    /// ```
    fn retention_set(&self, run: &ApiRunId, policy: RetentionPolicy) -> StrataResult<u64>;

    /// Clear the retention policy for a run (revert to default)
    ///
    /// After clearing, the run will use the default `KeepAll` policy.
    fn retention_clear(&self, run: &ApiRunId) -> StrataResult<bool>;
}

/// Statistics about retention for a run
#[derive(Debug, Clone, Default)]
pub struct RetentionStats {
    /// Total versions across all keys
    pub total_versions: u64,
    /// Versions eligible for garbage collection
    pub gc_eligible_versions: u64,
    /// Estimated bytes that could be reclaimed
    pub estimated_reclaimable_bytes: u64,
}

/// Extended retention operations (optional)
///
/// These operations are not required for M11 but provide
/// useful diagnostics.
pub trait RetentionSubstrateExt: RetentionSubstrate {
    /// Get retention statistics for a run
    ///
    /// Returns statistics about version retention in the run.
    fn retention_stats(&self, run: &ApiRunId) -> StrataResult<RetentionStats>;

    /// Trigger garbage collection for a run
    ///
    /// Normally, garbage collection happens automatically in the background.
    /// This method triggers an immediate collection cycle.
    fn retention_gc(&self, run: &ApiRunId) -> StrataResult<RetentionStats>;
}

// =============================================================================
// Implementation
// =============================================================================

use super::impl_::SubstrateImpl;
use strata_core::{Key, Namespace, RunId, Value};

/// System namespace prefix for retention policies
const RETENTION_KEY_PREFIX: &str = "_system/retention/";

/// Create a system key for storing retention policy
fn retention_key(run: &ApiRunId) -> String {
    format!("{}{}", RETENTION_KEY_PREFIX, run.as_str())
}

/// System namespace for internal storage
/// Uses a well-known "system" run ID (all zeros)
fn system_namespace() -> Namespace {
    Namespace::new(
        "_system".to_string(),
        "_system".to_string(),
        "_system".to_string(),
        RunId::from_bytes([0u8; 16]),
    )
}

/// Create a storage Key for the retention policy
fn storage_key(run: &ApiRunId) -> Key {
    Key::new_kv(system_namespace(), retention_key(run))
}

impl RetentionSubstrate for SubstrateImpl {
    fn retention_get(&self, run: &ApiRunId) -> StrataResult<Option<RetentionVersion>> {
        let key = storage_key(run);
        let system_run = RunId::from_bytes([0u8; 16]);

        // Read from storage using a transaction
        let result = self.db().transaction(system_run, |txn| {
            txn.get(&key)
        }).map_err(|e| strata_core::StrataError::internal(e.to_string()))?;

        match result {
            Some(value) => {
                // Deserialize RetentionVersion from stored Value
                let rv = deserialize_retention(&value)?;
                Ok(Some(rv))
            }
            None => Ok(None),
        }
    }

    fn retention_set(&self, _run: &ApiRunId, _policy: RetentionPolicy) -> StrataResult<u64> {
        // TODO: Re-implement once transaction_with_version is exposed through the new API surface
        Err(strata_core::StrataError::internal("retention_set temporarily disabled during engine re-architecture".to_string()))
    }

    fn retention_clear(&self, run: &ApiRunId) -> StrataResult<bool> {
        let key = storage_key(run);
        let system_run = RunId::from_bytes([0u8; 16]);

        let existed = self.db().transaction(system_run, |txn| {
            let existed = txn.get(&key)?.is_some();
            if existed {
                txn.delete(key.clone())?;
            }
            Ok(existed)
        }).map_err(|e| strata_core::StrataError::internal(e.to_string()))?;

        Ok(existed)
    }
}

/// Serialize RetentionVersion to Value
fn serialize_retention(rv: &RetentionVersion) -> strata_core::Result<Value> {
    // Serialize the policy to JSON string
    let json = serde_json::to_string(&rv.policy)
        .map_err(|e| strata_core::StrataError::invalid_input(format!("Serialization failed: {}", e)))?;

    // Store as an object with policy JSON, version, and timestamp
    let mut obj = std::collections::HashMap::new();
    obj.insert("policy".to_string(), Value::String(json));
    obj.insert("version".to_string(), Value::Int(rv.version as i64));
    obj.insert("timestamp".to_string(), Value::Int(rv.timestamp as i64));

    Ok(Value::Object(obj))
}

/// Deserialize RetentionVersion from Value
fn deserialize_retention(value: &Value) -> StrataResult<RetentionVersion> {
    let obj = match value {
        Value::Object(o) => o,
        _ => return Err(strata_core::StrataError::internal("Expected object for retention")),
    };

    let policy_json = match obj.get("policy") {
        Some(Value::String(s)) => s,
        _ => return Err(strata_core::StrataError::internal("Missing policy field")),
    };

    let policy: RetentionPolicy = serde_json::from_str(policy_json)
        .map_err(|e| strata_core::StrataError::internal(format!("Invalid policy JSON: {}", e)))?;

    let version = match obj.get("version") {
        Some(Value::Int(n)) => *n as u64,
        _ => 0,
    };

    let timestamp = match obj.get("timestamp") {
        Some(Value::Int(n)) => *n as u64,
        _ => 0,
    };

    Ok(RetentionVersion { policy, version, timestamp })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
    use strata_engine::Database;

    fn create_test_substrate() -> SubstrateImpl {
        let db = Arc::new(
            Database::builder()
                .in_memory()
                .open_temp()
                .expect("Failed to create test database")
        );
        SubstrateImpl::new(db)
    }

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn RetentionSubstrate) {}
    }

    #[test]
    fn test_retention_version() {
        let rv = RetentionVersion {
            policy: RetentionPolicy::KeepLast(100),
            version: 42,
            timestamp: 1234567890,
        };
        assert!(matches!(rv.policy, RetentionPolicy::KeepLast(100)));
    }

    #[test]
    fn test_retention_stats_default() {
        let stats = RetentionStats::default();
        assert_eq!(stats.total_versions, 0);
        assert_eq!(stats.gc_eligible_versions, 0);
    }

    #[test]
    fn test_retention_get_returns_none_by_default() {
        let substrate = create_test_substrate();
        let run = ApiRunId::default();

        let result = substrate.retention_get(&run).unwrap();
        assert!(result.is_none(), "Should return None when no policy is set");
    }

    #[test]
    #[ignore = "temporarily disabled during engine re-architecture"]
    fn test_retention_set_and_get_keep_last() {
        let substrate = create_test_substrate();
        let run = ApiRunId::default();

        // Set policy
        let version = substrate.retention_set(&run, RetentionPolicy::KeepLast(100)).unwrap();
        assert!(version > 0, "Should return a version number");

        // Get policy
        let result = substrate.retention_get(&run).unwrap();
        assert!(result.is_some(), "Should return the policy");

        let rv = result.unwrap();
        assert!(matches!(rv.policy, RetentionPolicy::KeepLast(100)));
        assert!(rv.timestamp > 0, "Should have a timestamp");
    }

    #[test]
    #[ignore = "temporarily disabled during engine re-architecture"]
    fn test_retention_set_and_get_keep_for() {
        let substrate = create_test_substrate();
        let run = ApiRunId::default();

        // Set policy
        let duration = Duration::from_secs(7 * 24 * 3600); // 7 days
        substrate.retention_set(&run, RetentionPolicy::KeepFor(duration)).unwrap();

        // Get policy
        let result = substrate.retention_get(&run).unwrap().unwrap();
        match result.policy {
            RetentionPolicy::KeepFor(d) => {
                assert_eq!(d.as_secs(), 7 * 24 * 3600);
            }
            _ => panic!("Expected KeepFor policy"),
        }
    }

    #[test]
    #[ignore = "temporarily disabled during engine re-architecture"]
    fn test_retention_set_and_get_keep_all() {
        let substrate = create_test_substrate();
        let run = ApiRunId::default();

        // Set explicit KeepAll
        substrate.retention_set(&run, RetentionPolicy::KeepAll).unwrap();

        // Get policy
        let result = substrate.retention_get(&run).unwrap().unwrap();
        assert!(matches!(result.policy, RetentionPolicy::KeepAll));
    }

    #[test]
    #[ignore = "temporarily disabled during engine re-architecture"]
    fn test_retention_clear() {
        let substrate = create_test_substrate();
        let run = ApiRunId::default();

        // Clear when nothing is set
        let cleared = substrate.retention_clear(&run).unwrap();
        assert!(!cleared, "Should return false when nothing to clear");

        // Set then clear
        substrate.retention_set(&run, RetentionPolicy::KeepLast(50)).unwrap();
        let cleared = substrate.retention_clear(&run).unwrap();
        assert!(cleared, "Should return true when clearing existing policy");

        // Verify it's cleared
        let result = substrate.retention_get(&run).unwrap();
        assert!(result.is_none(), "Policy should be cleared");

        // Clear again
        let cleared = substrate.retention_clear(&run).unwrap();
        assert!(!cleared, "Should return false when clearing non-existent policy");
    }

    #[test]
    #[ignore = "temporarily disabled during engine re-architecture"]
    fn test_retention_per_run_isolation() {
        let substrate = create_test_substrate();
        let run1 = ApiRunId::default();
        let run2 = ApiRunId::new();

        // Set different policies for different runs
        substrate.retention_set(&run1, RetentionPolicy::KeepLast(10)).unwrap();
        substrate.retention_set(&run2, RetentionPolicy::KeepLast(20)).unwrap();

        // Verify isolation
        let result1 = substrate.retention_get(&run1).unwrap().unwrap();
        let result2 = substrate.retention_get(&run2).unwrap().unwrap();

        match result1.policy {
            RetentionPolicy::KeepLast(n) => assert_eq!(n, 10),
            _ => panic!("Expected KeepLast(10)"),
        }

        match result2.policy {
            RetentionPolicy::KeepLast(n) => assert_eq!(n, 20),
            _ => panic!("Expected KeepLast(20)"),
        }
    }

    #[test]
    #[ignore = "temporarily disabled during engine re-architecture"]
    fn test_retention_update_policy() {
        let substrate = create_test_substrate();
        let run = ApiRunId::default();

        // Set initial policy
        substrate.retention_set(&run, RetentionPolicy::KeepLast(10)).unwrap();

        // Update policy
        substrate.retention_set(&run, RetentionPolicy::KeepLast(100)).unwrap();

        // Verify updated
        let result = substrate.retention_get(&run).unwrap().unwrap();
        match result.policy {
            RetentionPolicy::KeepLast(n) => assert_eq!(n, 100),
            _ => panic!("Expected KeepLast(100)"),
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let policies = vec![
            RetentionPolicy::KeepAll,
            RetentionPolicy::KeepLast(42),
            RetentionPolicy::KeepFor(Duration::from_secs(3600)),
            RetentionPolicy::Composite(vec![
                RetentionPolicy::KeepLast(10),
                RetentionPolicy::KeepFor(Duration::from_secs(86400)),
            ]),
        ];

        for policy in policies {
            let rv = RetentionVersion {
                policy: policy.clone(),
                version: 123,
                timestamp: 456789,
            };

            let serialized = serialize_retention(&rv).unwrap();
            let deserialized = deserialize_retention(&serialized).unwrap();

            assert_eq!(deserialized.policy, policy);
            assert_eq!(deserialized.version, 123);
            assert_eq!(deserialized.timestamp, 456789);
        }
    }
}
