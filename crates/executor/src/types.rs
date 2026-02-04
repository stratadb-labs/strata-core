//! Supporting types for commands and outputs.
//!
//! These types are used in command parameters and output values.
//! All types are serializable for cross-language use.

use serde::{Deserialize, Serialize};
use strata_core::Value;

// =============================================================================
// Branch Types
// =============================================================================

/// Branch identifier.
///
/// Can be "default" for the default branch, or a UUID string for named branches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchId(pub String);

impl Default for BranchId {
    fn default() -> Self {
        BranchId("default".to_string())
    }
}

impl BranchId {
    /// Check if this is the default branch.
    pub fn is_default(&self) -> bool {
        self.0 == "default"
    }

    /// Get the string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for BranchId {
    fn from(s: String) -> Self {
        BranchId(s)
    }
}

impl From<&str> for BranchId {
    fn from(s: &str) -> Self {
        BranchId(s.to_string())
    }
}

impl std::fmt::Display for BranchId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Branch status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    Active,
}

/// Branch information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchInfo {
    pub id: BranchId,
    pub status: BranchStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub parent_id: Option<BranchId>,
}

/// Versioned branch information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionedBranchInfo {
    pub info: BranchInfo,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    #[default]
    Cosine,
    Euclidean,
    DotProduct,
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

/// Vector collection information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub count: u64,
    /// Index type (e.g., "brute_force", "hnsw")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_type: Option<String>,
    /// Approximate memory usage in bytes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
}

/// Batch vector entry for bulk upsert
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchVectorEntry {
    pub key: String,
    pub vector: Vec<f32>,
    pub metadata: Option<Value>,
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
// Database Types
// =============================================================================

/// Database information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseInfo {
    pub version: String,
    pub uptime_secs: u64,
    pub branch_count: u64,
    pub total_keys: u64,
}

// =============================================================================
// Bundle Types
// =============================================================================

/// Information about a branch export operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchExportResult {
    pub branch_id: String,
    pub path: String,
    pub entry_count: u64,
    pub bundle_size: u64,
}

/// Information about a branch import operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchImportResult {
    pub branch_id: String,
    pub transactions_applied: u64,
    pub keys_written: u64,
}

/// Information about bundle validation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleValidateResult {
    pub branch_id: String,
    pub format_version: u32,
    pub entry_count: u64,
    pub checksums_valid: bool,
}

// =============================================================================
// Intelligence Types
// =============================================================================

/// A single hit from a cross-primitive search
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResultHit {
    /// Entity identifier string
    pub entity: String,
    /// Primitive type that produced this hit
    pub primitive: String,
    /// Relevance score (higher = more relevant)
    pub score: f32,
    /// Rank in result set (1-indexed)
    pub rank: u32,
    /// Optional text snippet
    pub snippet: Option<String>,
}
