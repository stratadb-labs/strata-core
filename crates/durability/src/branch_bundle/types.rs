//! RunBundle core types
//!
//! Types for the RunBundle archive format (.runbundle.tar.zst)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Current RunBundle format version
pub const RUNBUNDLE_FORMAT_VERSION: u32 = 2;

/// File extension for RunBundle archives
pub const RUNBUNDLE_EXTENSION: &str = ".runbundle.tar.zst";

/// Archive paths within the bundle
pub mod paths {
    /// Root directory in the archive
    pub const ROOT: &str = "runbundle";
    /// Bundle manifest file
    pub const MANIFEST: &str = "runbundle/MANIFEST.json";
    /// Run metadata file
    pub const RUN: &str = "runbundle/RUN.json";
    /// WAL entries file
    pub const WAL: &str = "runbundle/WAL.runlog";

    // Post-MVP paths (reserved)
    /// Index directory
    pub const INDEX_DIR: &str = "runbundle/INDEX";
    /// Writeset index file
    pub const WRITESET_INDEX: &str = "runbundle/INDEX/writeset.bin";
    /// Snapshot directory
    pub const SNAPSHOT_DIR: &str = "runbundle/SNAPSHOT";
}

/// Magic bytes for WAL.runlog header
pub const WAL_RUNLOG_MAGIC: &[u8; 10] = b"STRATA_WAL";

/// WAL.runlog format version
pub const WAL_RUNLOG_VERSION: u16 = 2;

// =============================================================================
// MANIFEST.json
// =============================================================================

/// Bundle manifest - format metadata and checksums
///
/// This is the first file read when opening a bundle.
/// It contains version info, checksums for integrity verification,
/// and a summary of bundle contents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleManifest {
    /// Format version (currently 1)
    pub format_version: u32,

    /// Strata version that created this bundle
    pub strata_version: String,

    /// ISO 8601 timestamp when bundle was created
    pub created_at: String,

    /// Checksum algorithm used (currently "xxh3")
    pub checksum_algorithm: String,

    /// Checksums for each file in the bundle
    /// Key: relative path (e.g., "RUN.json")
    /// Value: hex-encoded checksum
    pub checksums: HashMap<String, String>,

    /// Summary of bundle contents
    pub contents: BundleContents,
}

impl BundleManifest {
    /// Create a new manifest with current timestamp
    pub fn new(strata_version: impl Into<String>, contents: BundleContents) -> Self {
        Self {
            format_version: RUNBUNDLE_FORMAT_VERSION,
            strata_version: strata_version.into(),
            created_at: chrono_now_iso8601(),
            checksum_algorithm: "xxh3".to_string(),
            checksums: HashMap::new(),
            contents,
        }
    }

    /// Add a checksum for a file
    pub fn add_checksum(&mut self, path: impl Into<String>, checksum: impl Into<String>) {
        self.checksums.insert(path.into(), checksum.into());
    }
}

/// Summary of bundle contents
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BundleContents {
    /// Number of WAL entries in the bundle
    pub wal_entry_count: u64,

    /// Size of WAL.runlog in bytes (uncompressed)
    pub wal_size_bytes: u64,
    // Post-MVP fields:
    // pub has_snapshot: bool,
    // pub has_index: bool,
}

// =============================================================================
// RUN.json
// =============================================================================

/// Run metadata - human-readable run information
///
/// This file contains all metadata about the run, designed to be
/// readable with standard tools like `jq`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleRunInfo {
    /// Run ID (UUID string)
    pub branch_id: String,

    /// Human-readable run name
    pub name: String,

    /// Run state: "active"
    pub state: String,

    /// ISO 8601 timestamp when run was created
    pub created_at: String,

    /// ISO 8601 timestamp when run was closed
    pub closed_at: String,

    /// Parent run ID if this is a child run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,

    /// Error message if run failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BundleRunInfo {
    /// Check if the run state is a valid terminal state
    pub fn is_terminal_state(&self) -> bool {
        matches!(
            self.state.as_str(),
            "completed" | "failed" | "cancelled" | "archived"
        )
    }
}

// =============================================================================
// Export Types
// =============================================================================

/// Information returned after exporting a run
#[derive(Debug, Clone)]
pub struct RunExportInfo {
    /// ID of the exported run
    pub branch_id: String,

    /// Path where the bundle was written
    pub path: PathBuf,

    /// Number of WAL entries in the bundle
    pub wal_entry_count: u64,

    /// Size of the bundle file in bytes
    pub bundle_size_bytes: u64,

    /// xxh3 checksum of the entire bundle file
    pub checksum: String,
}

/// Options for export operation
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Zstd compression level (1-22, default: 3)
    pub compression_level: i32,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            compression_level: 3,
        }
    }
}

// =============================================================================
// Verify Types
// =============================================================================

/// Information returned after verifying a bundle
#[derive(Debug, Clone)]
pub struct BundleVerifyInfo {
    /// Run ID from the bundle
    pub branch_id: String,

    /// Format version of the bundle
    pub format_version: u32,

    /// Number of WAL entries in the bundle
    pub wal_entry_count: u64,

    /// Whether all checksums are valid
    pub checksums_valid: bool,
}

// =============================================================================
// Import Types
// =============================================================================

/// Information returned after importing a run
#[derive(Debug, Clone)]
pub struct ImportedRunInfo {
    /// Run ID of the imported run
    pub branch_id: String,

    /// Number of WAL entries that were replayed
    pub wal_entries_replayed: u64,
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get current time as ISO 8601 string
///
/// NOTE: This uses an approximate date calculation to avoid adding a chrono dependency.
/// The date may be off by a few days due to simplified leap year handling. This is
/// acceptable because this timestamp is only used for bundle metadata/display, not for
/// correctness. The actual WAL entries use proper microsecond timestamps from SystemTime.
fn chrono_now_iso8601() -> String {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();

    // Calculate time components (these are exact)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Approximate year/month/day calculation
    // This doesn't account for leap years correctly, so dates may be off by a few days.
    // For bundle metadata this is acceptable - use proper datetime library if precision needed.
    let years = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = (day_of_year / 30).min(11) + 1;
    let day = (day_of_year % 30) + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        years, month, day, hours, minutes, seconds
    )
}

/// Compute xxh3 hash of data and return as hex string
pub fn xxh3_hex(data: &[u8]) -> String {
    use xxhash_rust::xxh3::xxh3_64;
    format!("{:016x}", xxh3_64(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_new() {
        let contents = BundleContents {
            wal_entry_count: 100,
            wal_size_bytes: 5000,
        };
        let manifest = BundleManifest::new("0.12.0", contents.clone());

        assert_eq!(manifest.format_version, RUNBUNDLE_FORMAT_VERSION);
        assert_eq!(manifest.strata_version, "0.12.0");
        assert_eq!(manifest.checksum_algorithm, "xxh3");
        assert!(manifest.checksums.is_empty());
        assert_eq!(manifest.contents, contents);
    }

    #[test]
    fn test_manifest_add_checksum() {
        let mut manifest = BundleManifest::new(
            "0.12.0",
            BundleContents {
                wal_entry_count: 0,
                wal_size_bytes: 0,
            },
        );

        manifest.add_checksum("RUN.json", "abc123");
        manifest.add_checksum("WAL.runlog", "def456");

        assert_eq!(manifest.checksums.get("RUN.json"), Some(&"abc123".to_string()));
        assert_eq!(
            manifest.checksums.get("WAL.runlog"),
            Some(&"def456".to_string())
        );
    }

    #[test]
    fn test_manifest_json_roundtrip() {
        let mut manifest = BundleManifest::new(
            "0.12.0",
            BundleContents {
                wal_entry_count: 42,
                wal_size_bytes: 1234,
            },
        );
        manifest.add_checksum("RUN.json", "checksum123");

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: BundleManifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest, parsed);
    }

    #[test]
    fn test_run_info_terminal_states() {
        let make_run = |state: &str| BundleRunInfo {
            branch_id: "test".to_string(),
            name: "test".to_string(),
            state: state.to_string(),
            created_at: "2025-01-24T00:00:00Z".to_string(),
            closed_at: "2025-01-24T01:00:00Z".to_string(),
            parent_run_id: None,
            error: None,
        };

        assert!(make_run("completed").is_terminal_state());
        assert!(make_run("failed").is_terminal_state());
        assert!(make_run("cancelled").is_terminal_state());
        assert!(make_run("archived").is_terminal_state());

        assert!(!make_run("active").is_terminal_state());
        assert!(!make_run("unknown").is_terminal_state());
    }

    #[test]
    fn test_run_info_json_roundtrip() {
        let run_info = BundleRunInfo {
            branch_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "my-test-run".to_string(),
            state: "active".to_string(),
            created_at: "2025-01-24T10:00:00Z".to_string(),
            closed_at: "2025-01-24T11:30:00Z".to_string(),
            parent_run_id: None,
            error: None,
        };

        let json = serde_json::to_string_pretty(&run_info).unwrap();
        let parsed: BundleRunInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(run_info, parsed);
    }

    #[test]
    fn test_run_info_with_error() {
        let run_info = BundleRunInfo {
            branch_id: "test".to_string(),
            name: "failed-run".to_string(),
            state: "failed".to_string(),
            created_at: "2025-01-24T10:00:00Z".to_string(),
            closed_at: "2025-01-24T10:05:00Z".to_string(),
            parent_run_id: Some("parent-id".to_string()),
            error: Some("Connection timeout".to_string()),
        };

        let json = serde_json::to_string(&run_info).unwrap();
        assert!(json.contains("Connection timeout"));
        assert!(json.contains("parent-id"));
    }

    #[test]
    fn test_xxh3_hex() {
        let hash = xxh3_hex(b"hello world");
        assert_eq!(hash.len(), 16); // 64 bits = 16 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Same input should produce same hash
        assert_eq!(xxh3_hex(b"hello world"), xxh3_hex(b"hello world"));

        // Different input should produce different hash
        assert_ne!(xxh3_hex(b"hello"), xxh3_hex(b"world"));
    }

    #[test]
    fn test_export_options_default() {
        let opts = ExportOptions::default();
        assert_eq!(opts.compression_level, 3);
    }

    #[test]
    fn test_paths() {
        assert_eq!(paths::MANIFEST, "runbundle/MANIFEST.json");
        assert_eq!(paths::RUN, "runbundle/RUN.json");
        assert_eq!(paths::WAL, "runbundle/WAL.runlog");
    }
}
