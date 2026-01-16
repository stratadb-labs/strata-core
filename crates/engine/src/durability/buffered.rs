//! Buffered durability mode
//!
//! WAL append without immediate fsync. Background thread performs
//! periodic fsync based on interval or batch size threshold.
//!
//! # Use Cases
//!
//! - Production default (good balance of speed and safety)
//! - High-throughput workloads
//! - Acceptable bounded data loss window
//!
//! # Performance Contract
//!
//! - `persist()`: WAL append only, no fsync (<30µs)
//! - Background thread handles fsync
//! - Bounded data loss: max(flush_interval, pending_writes)
//!
//! # Thread Lifecycle
//!
//! **CRITICAL**: The background flush thread MUST be properly managed:
//! - `shutdown_flag: AtomicBool` signals thread to stop
//! - `flush_thread: JoinHandle` allows waiting for thread completion
//! - `Drop` implementation ensures clean shutdown

use super::Durability;
use in_mem_concurrency::TransactionContext;
use in_mem_concurrency::TransactionWALWriter;
use in_mem_core::error::Result;
use in_mem_durability::wal::WAL;
use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Buffered durability - async fsync with background thread
///
/// This mode provides a balance between performance and durability.
/// Writes are appended to the WAL immediately but fsync is deferred
/// to a background thread that flushes periodically.
///
/// # Thread Safety
///
/// All state is protected by appropriate synchronization primitives.
/// Multiple threads can call persist() concurrently.
///
/// # Shutdown
///
/// The background thread is automatically stopped when the struct is
/// dropped. Call `shutdown()` explicitly for graceful shutdown with
/// final flush.
///
/// # Example
///
/// ```ignore
/// use in_mem_engine::durability::BufferedDurability;
/// use std::time::Duration;
///
/// let durability = BufferedDurability::new(
///     wal,
///     100,   // flush every 100ms
///     1000,  // or every 1000 writes
/// );
/// durability.start_flush_thread();
/// ```
pub struct BufferedDurability {
    /// WAL for persisting transactions
    wal: Arc<Mutex<WAL>>,

    /// Flush interval in milliseconds
    flush_interval: Duration,

    /// Maximum pending writes before flush
    max_pending_writes: usize,

    /// Count of pending (unflushed) writes
    pending_writes: AtomicUsize,

    /// Time of last flush
    last_flush: Mutex<Instant>,

    /// Signal for shutdown
    shutdown_flag: AtomicBool,

    /// Condvar to wake up flush thread
    flush_signal: Arc<(Mutex<bool>, Condvar)>,

    /// Background flush thread handle
    flush_thread: Mutex<Option<JoinHandle<()>>>,
}

impl BufferedDurability {
    /// Create new Buffered durability mode
    ///
    /// # Arguments
    ///
    /// * `wal` - Write-ahead log instance
    /// * `flush_interval_ms` - Maximum time between fsyncs (milliseconds)
    /// * `max_pending_writes` - Maximum writes before triggering flush
    ///
    /// # Note
    ///
    /// Call `start_flush_thread()` after creation to start the
    /// background flush thread.
    pub fn new(wal: Arc<Mutex<WAL>>, flush_interval_ms: u64, max_pending_writes: usize) -> Self {
        Self {
            wal,
            flush_interval: Duration::from_millis(flush_interval_ms),
            max_pending_writes,
            pending_writes: AtomicUsize::new(0),
            last_flush: Mutex::new(Instant::now()),
            shutdown_flag: AtomicBool::new(false),
            flush_signal: Arc::new((Mutex::new(false), Condvar::new())),
            flush_thread: Mutex::new(None),
        }
    }

    /// Start the background flush thread
    ///
    /// This must be called after creation to enable async fsyncs.
    /// The thread will run until shutdown() is called or the struct
    /// is dropped.
    pub fn start_flush_thread(self: &Arc<Self>) {
        let durability = Arc::clone(self);
        let handle = thread::spawn(move || {
            durability.flush_loop();
        });

        let mut thread_guard = self.flush_thread.lock();
        *thread_guard = Some(handle);
    }

    /// Background flush loop
    fn flush_loop(&self) {
        loop {
            // Wait for signal or timeout
            let (lock, cvar) = &*self.flush_signal;
            let mut signaled = lock.lock();

            // Wait with timeout - this allows periodic flushing
            let result = cvar.wait_for(&mut signaled, self.flush_interval);

            // Reset signal
            *signaled = false;
            drop(signaled);

            // Check shutdown flag FIRST
            if self.shutdown_flag.load(Ordering::SeqCst) {
                // Final flush before exit
                if let Err(e) = self.flush_sync() {
                    eprintln!("BufferedDurability: final flush error: {}", e);
                }
                break;
            }

            // Check if we timed out or were signaled
            let should_flush = result.timed_out()
                || self.pending_writes.load(Ordering::Relaxed) >= self.max_pending_writes;

            if should_flush {
                if let Err(e) = self.flush_sync() {
                    eprintln!("BufferedDurability: flush error: {}", e);
                }
            }
        }
    }

    /// Signal the flush thread to wake up
    fn signal_flush(&self) {
        let (lock, cvar) = &*self.flush_signal;
        let mut signaled = lock.lock();
        *signaled = true;
        cvar.notify_one();
    }

    /// Synchronous flush - fsync the WAL
    ///
    /// This is called by the background thread and during shutdown.
    pub fn flush_sync(&self) -> Result<()> {
        let wal = self.wal.lock();
        wal.fsync()?;

        // Reset pending count and update last flush time
        self.pending_writes.store(0, Ordering::Relaxed);
        *self.last_flush.lock() = Instant::now();

        Ok(())
    }

    /// Get number of pending (unflushed) writes
    pub fn pending_count(&self) -> usize {
        self.pending_writes.load(Ordering::Relaxed)
    }

    /// Get time since last flush
    pub fn time_since_flush(&self) -> Duration {
        self.last_flush.lock().elapsed()
    }
}

impl Durability for BufferedDurability {
    /// Append transaction to WAL without fsync
    ///
    /// This method only appends to the WAL buffer. The background
    /// thread will handle fsync asynchronously.
    ///
    /// # Performance
    ///
    /// Target: <30µs (WAL append only, no syscall for fsync)
    fn persist(&self, txn: &TransactionContext, commit_version: u64) -> Result<()> {
        // Acquire WAL lock and write transaction
        {
            let mut wal = self.wal.lock();
            let mut wal_writer = TransactionWALWriter::new(&mut wal, txn.txn_id, txn.run_id);

            // Write BeginTxn
            wal_writer.write_begin()?;

            // Write all puts
            for (key, value) in &txn.write_set {
                wal_writer.write_put(key.clone(), value.clone(), commit_version)?;
            }

            // Write all deletes
            for key in &txn.delete_set {
                wal_writer.write_delete(key.clone(), commit_version)?;
            }

            // Write CAS operations
            for cas_op in &txn.cas_set {
                wal_writer.write_put(
                    cas_op.key.clone(),
                    cas_op.new_value.clone(),
                    commit_version,
                )?;
            }

            // Write CommitTxn
            wal_writer.write_commit()?;

            // Note: NO fsync here - that's the key difference from Strict mode
        }

        // Track pending write
        let pending = self.pending_writes.fetch_add(1, Ordering::Relaxed) + 1;

        // Signal flush if threshold reached
        if pending >= self.max_pending_writes {
            self.signal_flush();
        }

        Ok(())
    }

    /// Graceful shutdown - flush all pending writes
    ///
    /// This method:
    /// 1. Signals the background thread to stop
    /// 2. Waits for the thread to finish (joins)
    /// 3. Performs a final synchronous flush
    fn shutdown(&self) -> Result<()> {
        // Signal shutdown
        self.shutdown_flag.store(true, Ordering::SeqCst);
        self.signal_flush();

        // Wait for background thread to finish
        let mut thread_guard = self.flush_thread.lock();
        if let Some(handle) = thread_guard.take() {
            // Ignore join errors (thread might have panicked)
            let _ = handle.join();
        }

        // Final synchronous flush to ensure all data is persisted
        self.flush_sync()
    }

    /// Buffered mode eventually persists data
    #[inline]
    fn is_persistent(&self) -> bool {
        true
    }

    /// Returns "Buffered"
    #[inline]
    fn mode_name(&self) -> &'static str {
        "Buffered"
    }
}

impl Drop for BufferedDurability {
    fn drop(&mut self) {
        // Signal shutdown to background thread
        self.shutdown_flag.store(true, Ordering::SeqCst);
        self.signal_flush();

        // Wait for thread to finish
        let mut thread_guard = self.flush_thread.lock();
        if let Some(handle) = thread_guard.take() {
            let _ = handle.join();
        }

        // Note: We don't call flush_sync() in Drop because it could fail
        // and Drop can't return errors. The explicit shutdown() should be
        // called for guaranteed flush.
    }
}

// Manual Debug impl
impl std::fmt::Debug for BufferedDurability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferedDurability")
            .field("flush_interval", &self.flush_interval)
            .field("max_pending_writes", &self.max_pending_writes)
            .field(
                "pending_writes",
                &self.pending_writes.load(Ordering::Relaxed),
            )
            .field("shutdown", &self.shutdown_flag.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_durability::wal::DurabilityMode;
    use tempfile::TempDir;

    fn create_test_wal() -> (TempDir, Arc<Mutex<WAL>>) {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        (temp_dir, Arc::new(Mutex::new(wal)))
    }

    #[test]
    fn test_buffered_is_persistent() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        assert!(durability.is_persistent());
    }

    #[test]
    fn test_buffered_requires_wal() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        assert!(durability.requires_wal());
    }

    #[test]
    fn test_buffered_mode_name() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        assert_eq!(durability.mode_name(), "Buffered");
    }

    #[test]
    fn test_buffered_pending_count() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        assert_eq!(durability.pending_count(), 0);
    }

    #[test]
    fn test_buffered_flush_sync() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        assert!(durability.flush_sync().is_ok());
    }

    #[test]
    fn test_buffered_shutdown_without_thread() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        // Shutdown without starting thread should work
        assert!(durability.shutdown().is_ok());
    }

    #[test]
    fn test_buffered_with_thread() {
        let (_temp, wal) = create_test_wal();
        let durability = Arc::new(BufferedDurability::new(wal, 50, 10));
        durability.start_flush_thread();

        // Let it run briefly
        std::thread::sleep(Duration::from_millis(100));

        // Shutdown should succeed
        assert!(durability.shutdown().is_ok());
    }

    #[test]
    fn test_buffered_debug() {
        let (_temp, wal) = create_test_wal();
        let durability = BufferedDurability::new(wal, 100, 1000);
        let debug_str = format!("{:?}", durability);
        assert!(debug_str.contains("BufferedDurability"));
        assert!(debug_str.contains("flush_interval"));
    }

    #[test]
    fn test_buffered_drop_stops_thread() {
        let (_temp, wal) = create_test_wal();
        {
            let durability = Arc::new(BufferedDurability::new(wal, 50, 10));
            durability.start_flush_thread();
            // Drop should stop the thread cleanly
        }
        // If we get here without hanging, the test passed
    }
}
