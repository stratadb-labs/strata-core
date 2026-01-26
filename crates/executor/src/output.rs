//! Output enum for command execution results.
//!
//! Every command produces exactly one output type. This mapping is deterministic:
//! the same command always produces the same output variant (though the values
//! may differ based on database state).

use serde::{Deserialize, Serialize};
use strata_core::Value;

use crate::types::*;

/// Successful command execution results.
///
/// Each [`Command`](crate::Command) variant maps to exactly one `Output` variant.
/// This mapping is deterministic and documented in the command definitions.
///
/// # Example
///
/// ```ignore
/// use strata_executor::{Command, Output, Executor};
///
/// let result = executor.execute(Command::KvGet { run, key })?;
///
/// match result {
///     Output::MaybeVersioned(Some(v)) => println!("Found: {:?}", v.value),
///     Output::MaybeVersioned(None) => println!("Not found"),
///     _ => unreachable!("KvGet always returns MaybeVersioned"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Output {
    // ==================== Primitive Results ====================
    /// No return value (delete, flush, compact)
    Unit,

    /// Single value without version info
    Value(Value),

    /// Value with version metadata
    Versioned(VersionedValue),

    /// Optional value (for get operations that may not find a key)
    Maybe(Option<Value>),

    /// Optional versioned value (most common for get operations)
    MaybeVersioned(Option<VersionedValue>),

    /// Optional version number (for CAS operations)
    MaybeVersion(Option<u64>),

    /// Version number
    Version(u64),

    /// Boolean result
    Bool(bool),

    /// Signed integer result (for incr operations)
    Int(i64),

    /// Unsigned integer result (for count operations)
    Uint(u64),

    /// Float result (for json_increment)
    Float(f64),

    // ==================== Collections ====================
    /// Multiple optional versioned values (mget operations)
    Values(Vec<Option<VersionedValue>>),

    /// List of versioned values (history operations)
    VersionedValues(Vec<VersionedValue>),

    /// List of versions
    Versions(Vec<u64>),

    /// List of keys
    Keys(Vec<String>),

    /// List of strings (tags, stream names, etc.)
    Strings(Vec<String>),

    /// List of booleans (batch delete results)
    Bools(Vec<bool>),

    // ==================== Scan Results ====================
    /// KV scan result with cursor
    KvScanResult {
        entries: Vec<(String, VersionedValue)>,
        cursor: Option<String>,
    },

    /// JSON list result with cursor
    JsonListResult {
        keys: Vec<String>,
        cursor: Option<String>,
    },

    // ==================== Search Results ====================
    /// JSON search hits
    JsonSearchHits(Vec<JsonSearchHit>),

    /// Vector search matches
    VectorMatches(Vec<VectorMatch>),

    /// Vector search with budget exhaustion flag
    VectorMatchesWithExhausted {
        matches: Vec<VectorMatch>,
        exhausted: bool,
    },

    // ==================== Vector-specific ====================
    /// Single vector data
    VectorData(Option<VersionedVectorData>),

    /// Multiple vector data
    VectorDataList(Vec<Option<VersionedVectorData>>),

    /// Vector history
    VectorDataHistory(Vec<VersionedVectorData>),

    /// Vector scan result
    VectorKeyValues(Vec<(String, VectorData)>),

    /// Vector batch upsert result
    VectorBatchResult(Vec<VectorBatchEntry>),

    /// Vector collection info
    VectorCollectionInfo(Option<CollectionInfo>),

    /// List of vector collections
    VectorCollectionList(Vec<CollectionInfo>),

    // ==================== Event-specific ====================
    /// Event stream info
    StreamInfo(StreamInfo),

    /// Chain verification result
    ChainVerification(ChainVerificationResult),

    // ==================== Run-specific ====================
    /// Single run info (unversioned)
    RunInfo(RunInfo),

    /// Versioned run info
    RunInfoVersioned(VersionedRunInfo),

    /// List of versioned run infos
    RunInfoList(Vec<VersionedRunInfo>),

    /// Run creation result (info + version)
    RunWithVersion {
        info: RunInfo,
        version: u64,
    },

    /// Optional run ID (for parent lookup)
    MaybeRunId(Option<RunId>),

    // ==================== Transaction-specific ====================
    /// Transaction ID
    TxnId(String),

    /// Transaction info
    TxnInfo(Option<TransactionInfo>),

    // ==================== Retention-specific ====================
    /// Retention version info
    RetentionVersion(Option<RetentionVersionInfo>),

    /// Retention policy
    RetentionPolicy(RetentionPolicyInfo),

    // ==================== Database-specific ====================
    /// Database info
    DatabaseInfo(DatabaseInfo),

    /// Ping response
    Pong {
        version: String,
    },
}
