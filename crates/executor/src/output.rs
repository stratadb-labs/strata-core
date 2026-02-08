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
/// ```text
/// use strata_executor::{Command, Output, Executor};
///
/// let result = executor.execute(Command::KvGet { branch, key })?;
///
/// match result {
///     Output::Maybe(Some(v)) => println!("Found: {:?}", v),
///     Output::Maybe(None) => println!("Not found"),
///     _ => unreachable!("KvGet always returns Maybe"),
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Output {
    // ==================== Primitive Results ====================
    /// No return value (delete, flush, compact)
    Unit,

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

    /// Unsigned integer result (for count operations)
    Uint(u64),

    // ==================== Collections ====================
    /// List of versioned values (history operations)
    VersionedValues(Vec<VersionedValue>),

    /// Version history result (getv/readv operations).
    /// None if the key/cell/document doesn't exist.
    VersionHistory(Option<Vec<VersionedValue>>),

    /// List of keys
    Keys(Vec<String>),

    // ==================== Scan Results ====================
    /// JSON list result with cursor
    JsonListResult {
        /// Matching document keys.
        keys: Vec<String>,
        /// Cursor for fetching the next page, if more results exist.
        cursor: Option<String>,
    },

    // ==================== Search Results ====================
    /// Vector search matches
    VectorMatches(Vec<VectorMatch>),

    // ==================== Vector-specific ====================
    /// Single vector data
    VectorData(Option<VersionedVectorData>),

    /// List of vector collections
    VectorCollectionList(Vec<CollectionInfo>),

    /// Multiple version numbers (for batch operations)
    Versions(Vec<u64>),

    // ==================== Branch-specific ====================
    /// Optional versioned branch info (for branch_get which may not find a branch)
    MaybeBranchInfo(Option<VersionedBranchInfo>),

    /// List of versioned branch infos
    BranchInfoList(Vec<VersionedBranchInfo>),

    /// Branch creation result (info + version)
    BranchWithVersion {
        /// Newly created branch metadata.
        info: BranchInfo,
        /// Version number assigned to the creation event.
        version: u64,
    },

    // ==================== Transaction-specific ====================
    /// Transaction info
    TxnInfo(Option<TransactionInfo>),

    /// Transaction successfully begun
    TxnBegun,

    /// Transaction committed with version
    TxnCommitted {
        /// Commit version number.
        version: u64,
    },

    /// Transaction aborted
    TxnAborted,

    // ==================== Database-specific ====================
    /// Database info
    DatabaseInfo(DatabaseInfo),

    /// Ping response
    Pong {
        /// Database engine version string.
        version: String,
    },

    // ==================== Intelligence ====================
    /// Search results across primitives
    SearchResults(Vec<SearchResultHit>),

    // ==================== Space ====================
    /// List of space names
    SpaceList(Vec<String>),

    // ==================== Bundle ====================
    /// Branch export result
    BranchExported(BranchExportResult),

    /// Branch import result
    BranchImported(BranchImportResult),

    /// Bundle validation result
    BundleValidated(BundleValidateResult),

    /// Time range for a branch (oldest and latest timestamps in microseconds since epoch)
    TimeRange {
        /// Oldest timestamp, or None if branch has no data.
        oldest_ts: Option<u64>,
        /// Latest timestamp, or None if branch has no data.
        latest_ts: Option<u64>,
    },
}
