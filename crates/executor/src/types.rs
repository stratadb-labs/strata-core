//! Supporting types for commands and outputs.
//!
//! These types are used in command parameters and output values.
//! All types are serializable for cross-language use.

use serde::{Deserialize, Serialize};
use strata_core::Value;

// =============================================================================
// Run Types
// =============================================================================

/// Run identifier.
///
/// Can be "default" for the default run, or a UUID string for named runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub String);

impl RunId {
    /// Create a RunId for the default run.
    pub fn default() -> Self {
        RunId("default".to_string())
    }

    /// Check if this is the default run.
    pub fn is_default(&self) -> bool {
        self.0 == "default"
    }

    /// Get the string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::default()
    }
}

impl From<String> for RunId {
    fn from(s: String) -> Self {
        RunId(s)
    }
}

impl From<&str> for RunId {
    fn from(s: &str) -> Self {
        RunId(s.to_string())
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Run status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Active,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Archived,
}

/// Run information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunInfo {
    pub id: RunId,
    pub status: RunStatus,
    pub metadata: Option<Value>,
    pub created_at: u64,
    pub updated_at: u64,
    pub parent_id: Option<RunId>,
    pub tags: Vec<String>,
}

/// Versioned run information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionedRunInfo {
    pub info: RunInfo,
    pub version: u64,
    pub timestamp: u64,
}

// =============================================================================
// Versioned Types
// =============================================================================

/// A value with version metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionedValue {
    pub value: Value,
    pub version: u64,
    pub timestamp: u64,
}

// =============================================================================
// Vector Types
// =============================================================================

/// Distance metric for vector similarity search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

impl Default for DistanceMetric {
    fn default() -> Self {
        DistanceMetric::Cosine
    }
}

/// Metadata filter for vector search
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: Value,
}

/// Filter operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Contains,
}

/// Vector data (embedding + metadata)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorData {
    pub embedding: Vec<f32>,
    pub metadata: Option<Value>,
}

/// Versioned vector data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionedVectorData {
    pub key: String,
    pub data: VectorData,
    pub version: u64,
    pub timestamp: u64,
}

/// Vector search match result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorMatch {
    pub key: String,
    pub score: f32,
    pub metadata: Option<Value>,
}

/// Vector batch entry (for batch operations)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorEntry {
    pub key: String,
    pub embedding: Vec<f32>,
    pub metadata: Option<Value>,
}

/// Vector batch result entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorBatchEntry {
    pub key: String,
    pub result: Result<u64, String>, // version or error message
}

/// Vector collection information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub count: u64,
}

// =============================================================================
// Event Types
// =============================================================================

/// Event stream information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub name: String,
    pub length: u64,
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
}

/// Chain verification result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    pub valid: bool,
    pub checked_count: u64,
    pub error: Option<String>,
}

// =============================================================================
// JSON Types
// =============================================================================

/// JSON search hit
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSearchHit {
    pub key: String,
    pub score: f32,
    pub highlights: Vec<String>,
}

// =============================================================================
// Transaction Types
// =============================================================================

/// Transaction options
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TxnOptions {
    pub read_only: bool,
}

/// Transaction information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub id: String,
    pub status: TxnStatus,
    pub started_at: u64,
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxnStatus {
    Active,
    Committed,
    RolledBack,
}

// =============================================================================
// Retention Types
// =============================================================================

/// Retention policy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionPolicyInfo {
    KeepAll,
    KeepLast { count: u64 },
    KeepFor { duration_secs: u64 },
}

impl Default for RetentionPolicyInfo {
    fn default() -> Self {
        RetentionPolicyInfo::KeepAll
    }
}

/// Retention version information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionVersionInfo {
    pub policy: RetentionPolicyInfo,
    pub version: u64,
}

// =============================================================================
// Database Types
// =============================================================================

/// Database information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub version: String,
    pub uptime_secs: u64,
    pub run_count: u64,
    pub total_keys: u64,
}
