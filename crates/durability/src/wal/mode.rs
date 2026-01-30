//! Durability mode configuration
//!
//! Controls WAL sync behavior (None, Strict, Batched).

/// Durability mode for WAL operations
///
/// Controls when the WAL is fsynced to disk. This is orthogonal to
/// PersistenceMode (which controls whether files exist at all).
///
/// # Modes
///
/// | Mode | fsync | Data Loss Window |
/// |------|-------|-----------------|
/// | None | Never | All uncommitted |
/// | Strict | Every commit | Zero |
/// | Batched | Periodic | Up to interval/batch |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityMode {
    /// No durability - all data lost on crash (fastest mode)
    ///
    /// Bypasses WAL entirely. No fsync, no file I/O.
    /// Target latency: <3µs for engine/put_direct.
    /// Use case: Tests, caches, ephemeral data, development.
    ///
    /// # Performance
    ///
    /// This mode enables 250K+ ops/sec by eliminating I/O entirely.
    None,

    /// fsync after every commit (slow, maximum durability)
    ///
    /// Use when data loss is unacceptable, even for a single write.
    /// Expect 10ms+ latency per write.
    Strict,

    /// fsync every N commits OR every T milliseconds
    ///
    /// Good balance of speed and safety. May lose up to batch_size
    /// writes or interval_ms of data on crash.
    /// Target latency: <30µs.
    Batched {
        /// Maximum time between fsyncs in milliseconds
        interval_ms: u64,
        /// Maximum writes between fsyncs
        batch_size: usize,
    },
}

impl DurabilityMode {
    /// Check if this mode requires WAL persistence
    ///
    /// Returns false for None mode, true for all others.
    pub fn requires_wal(&self) -> bool {
        !matches!(self, DurabilityMode::None)
    }

    /// Check if this mode requires immediate fsync on every commit
    ///
    /// Returns true only for Strict mode.
    pub fn requires_immediate_fsync(&self) -> bool {
        matches!(self, DurabilityMode::Strict)
    }

    /// Human-readable description of the mode
    pub fn description(&self) -> &'static str {
        match self {
            DurabilityMode::None => "No durability (fastest, all data lost on crash)",
            DurabilityMode::Strict => "Sync fsync (safest, slowest)",
            DurabilityMode::Batched { .. } => "Batched fsync (balanced speed/safety)",
        }
    }

    /// Create a buffered mode with recommended defaults
    ///
    /// Returns `Batched { interval_ms: 100, batch_size: 1000 }`.
    ///
    /// # Default Values
    ///
    /// - **interval_ms**: 100 - Maximum 100ms between fsyncs
    /// - **batch_size**: 1000 - Maximum 1000 writes before fsync
    ///
    /// # Rationale
    ///
    /// These defaults balance performance and durability:
    /// - 100ms interval keeps data loss window bounded
    /// - 1000 batch size handles burst writes efficiently
    /// - Both thresholds work together - whichever is reached first triggers fsync
    ///
    /// This is the recommended mode for production workloads.
    pub fn buffered_default() -> Self {
        DurabilityMode::Batched {
            interval_ms: 100,
            batch_size: 1000,
        }
    }
}

impl Default for DurabilityMode {
    fn default() -> Self {
        // Default: batched with 100ms interval or 1000 commits
        DurabilityMode::Batched {
            interval_ms: 100,
            batch_size: 1000,
        }
    }
}
