//! Database builder for fluent configuration
//!
//! Provides a builder pattern for configuring and opening databases with
//! different durability modes.

use std::path::PathBuf;
use std::sync::Arc;
use strata_core::StrataError;
use strata_core::StrataResult;
use strata_durability::wal::DurabilityMode;

use super::Database;

// ============================================================================
// Database Builder Pattern
// ============================================================================

/// Builder for Database configuration
///
/// Provides a fluent API for configuring and opening databases with
/// different durability modes. The builder requires an explicit path.
///
/// # Three Ways to Open a Database
///
/// ```ignore
/// use strata_engine::Database;
///
/// // 1. Simple open with sensible defaults (standard durability)
/// let db = Database::open("/data/mydb")?;
///
/// // 2. Builder for custom durability
/// let db = Database::builder()
///     .path("/data/mydb")
///     .always()  // or .standard() or .cache()
///     .open()?;
///
/// // 3. Cache (no files, testing)
/// let db = Database::cache()?;
/// ```
///
/// # Key Principle
///
/// Durability modes only make sense with persistent storage. If you need
/// a database without disk files, use [`Database::cache()`] instead
/// of configuring durability options.
///
/// # Performance Targets
///
/// | Mode | Target Latency | Throughput |
/// |------|----------------|------------|
/// | Cache | <3µs put | 250K+ ops/sec |
/// | Standard | <30µs put | 50K+ ops/sec |
/// | Always | ~2ms put | ~500 ops/sec |
#[derive(Debug, Clone)]
pub struct DatabaseBuilder {
    /// Database path (required for open())
    path: Option<PathBuf>,
    /// Durability mode (controls WAL sync behavior)
    durability: DurabilityMode,
}

impl DatabaseBuilder {
    /// Create new builder with defaults
    ///
    /// Defaults to Always durability mode for backwards compatibility.
    pub fn new() -> Self {
        Self {
            path: None,
            durability: DurabilityMode::Always, // default for backwards compatibility
        }
    }

    /// Set database path
    ///
    /// Required for `open()`. Use `Database::cache()` for no-file testing.
    pub fn path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Use cache mode (no WAL sync, files still created)
    ///
    /// This sets `DurabilityMode::Cache` which bypasses WAL fsync.
    /// **Note**: Disk files are still created. For truly file-free operation,
    /// use [`Database::cache()`] instead.
    ///
    /// Target latency: <3µs for engine/put_direct
    /// Throughput: 250K+ ops/sec
    ///
    /// All data lost on crash. Use for tests, caches, ephemeral data.
    pub fn cache(mut self) -> Self {
        self.durability = DurabilityMode::Cache;
        self
    }

    /// Use Standard mode with defaults (balanced)
    ///
    /// # Default Parameters
    ///
    /// - **flush_interval_ms**: 100ms - Maximum time between fsyncs
    /// - **max_pending_writes**: 1000 - Maximum writes before forced fsync
    ///
    /// These defaults provide a good balance between performance and durability
    /// for typical production workloads. The maximum data loss window is
    /// whichever threshold is reached first (100ms OR 1000 writes).
    ///
    /// # Performance Targets
    ///
    /// - Target latency: <30µs for kvstore/put
    /// - Throughput: 50K+ ops/sec
    ///
    /// Recommended for production workloads.
    pub fn standard(mut self) -> Self {
        self.durability = DurabilityMode::standard_default();
        self
    }

    /// Use Always mode (default, safest)
    ///
    /// fsync on every commit. Zero data loss on crash.
    /// Slowest mode - use for checkpoints, metadata, audit logs.
    pub fn always(mut self) -> Self {
        self.durability = DurabilityMode::Always;
        self
    }

    /// Open the database
    ///
    /// Requires a path to be set via `.path()`. For testing without disk files,
    /// use `Database::cache()` instead.
    ///
    /// # Thread Safety
    ///
    /// If the same path is opened multiple times, returns the same Arc<Database>.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - No path was configured (use `.path()` or `Database::cache()`)
    /// - Directory creation, WAL opening, or recovery fails
    pub fn open(self) -> StrataResult<Arc<Database>> {
        let path = self.path.ok_or_else(|| {
            StrataError::invalid_input(
                "DatabaseBuilder::open() requires a path. Use Database::cache() for testing.",
            )
        })?;

        Database::open_with_mode(path, self.durability)
    }
}

impl Default for DatabaseBuilder {
    fn default() -> Self {
        Self::new()
    }
}
