//! TTL cleanup background task
//!
//! This module provides TTLCleaner that runs in a background thread
//! and periodically cleans up expired keys using the normal delete() path.
//!
//! # Design Notes
//!
//! - Uses transactions (delete()) not direct mutation for proper coordination
//! - Runs in background thread, doesn't block writes
//! - Graceful shutdown via atomic flag
//! - Configurable check interval

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use in_mem_core::Storage;

use crate::UnifiedStore;

/// Background TTL cleanup task
///
/// Periodically scans for expired keys and deletes them via
/// normal transactional path (respects locks, WAL, etc.)
///
/// # Example
///
/// ```ignore
/// use std::sync::Arc;
/// use std::time::Duration;
/// use in_mem_storage::{UnifiedStore, TTLCleaner};
///
/// let store = Arc::new(UnifiedStore::new());
/// let cleaner = TTLCleaner::new(Arc::clone(&store), Duration::from_secs(60));
/// let handle = cleaner.start();
///
/// // ... use the store ...
///
/// // Shutdown gracefully
/// cleaner.shutdown();
/// handle.join().unwrap();
/// ```
pub struct TTLCleaner {
    /// Reference to the store to clean
    store: Arc<UnifiedStore>,
    /// How often to check for expired keys
    check_interval: Duration,
    /// Shutdown signal
    shutdown: Arc<AtomicBool>,
}

impl TTLCleaner {
    /// Create a new TTL cleaner
    ///
    /// # Arguments
    ///
    /// * `store` - The UnifiedStore to clean
    /// * `check_interval` - How often to check for expired keys
    pub fn new(store: Arc<UnifiedStore>, check_interval: Duration) -> Self {
        Self {
            store,
            check_interval,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the background cleanup task
    ///
    /// Returns a JoinHandle that can be used to wait for the thread to complete.
    /// The thread will run until `shutdown()` is called.
    pub fn start(&self) -> JoinHandle<()> {
        let store = Arc::clone(&self.store);
        let shutdown = Arc::clone(&self.shutdown);
        let check_interval = self.check_interval;

        thread::spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                // Sleep first (don't cleanup immediately on start)
                // Use smaller sleep intervals to check shutdown more frequently
                let sleep_interval = Duration::from_millis(100).min(check_interval);
                let mut elapsed = Duration::ZERO;

                while elapsed < check_interval {
                    if shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(sleep_interval);
                    elapsed += sleep_interval;
                }

                // Find expired keys
                if let Ok(expired) = store.find_expired_keys() {
                    // Delete each via normal path (transactional)
                    for key in expired {
                        // Ignore errors (key might have been deleted by user)
                        let _ = store.delete(&key);
                    }
                }
            }
        })
    }

    /// Signal shutdown (for graceful termination)
    ///
    /// After calling this, the background thread will exit on its next iteration.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Check if shutdown has been signaled
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::{Key, Namespace, RunId, Value};

    #[test]
    fn test_ttl_cleaner_creation() {
        let store = Arc::new(UnifiedStore::new());
        let cleaner = TTLCleaner::new(Arc::clone(&store), Duration::from_secs(60));

        assert!(!cleaner.is_shutdown());
    }

    #[test]
    fn test_ttl_cleaner_shutdown() {
        let store = Arc::new(UnifiedStore::new());
        let cleaner = TTLCleaner::new(Arc::clone(&store), Duration::from_secs(60));

        cleaner.shutdown();
        assert!(cleaner.is_shutdown());
    }

    #[test]
    fn test_ttl_cleaner_deletes_expired() {
        let store = Arc::new(UnifiedStore::new());
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Put key with very short TTL (1 second)
        let key = Key::new_kv(ns.clone(), "temp");
        store
            .put(
                key.clone(),
                Value::Bytes(b"data".to_vec()),
                Some(Duration::from_secs(1)),
            )
            .unwrap();

        // Key should exist initially
        assert!(store.get(&key).unwrap().is_some());

        // Start cleaner with fast interval
        let cleaner = TTLCleaner::new(Arc::clone(&store), Duration::from_millis(100));
        let handle = cleaner.start();

        // Wait for TTL to expire and cleaner to run
        thread::sleep(Duration::from_millis(1500));

        // Key should be deleted
        let result = store.get(&key).unwrap();
        assert!(result.is_none());

        // Shutdown cleaner
        cleaner.shutdown();
        handle.join().unwrap();
    }

    #[test]
    fn test_ttl_cleaner_does_not_delete_non_expired() {
        let store = Arc::new(UnifiedStore::new());
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Put key with long TTL (60 seconds)
        let key = Key::new_kv(ns.clone(), "persistent");
        store
            .put(
                key.clone(),
                Value::Bytes(b"data".to_vec()),
                Some(Duration::from_secs(60)),
            )
            .unwrap();

        // Start cleaner with fast interval
        let cleaner = TTLCleaner::new(Arc::clone(&store), Duration::from_millis(100));
        let handle = cleaner.start();

        // Let cleaner run a few cycles
        thread::sleep(Duration::from_millis(500));

        // Key should still exist (not expired yet)
        let result = store.get(&key).unwrap();
        assert!(result.is_some());

        // Shutdown cleaner
        cleaner.shutdown();
        handle.join().unwrap();
    }

    #[test]
    fn test_ttl_cleaner_graceful_shutdown() {
        let store = Arc::new(UnifiedStore::new());
        let cleaner = TTLCleaner::new(Arc::clone(&store), Duration::from_secs(10));
        let handle = cleaner.start();

        // Immediately shutdown
        cleaner.shutdown();

        // Should complete quickly (not wait 10 seconds)
        let start = std::time::Instant::now();
        handle.join().unwrap();
        let elapsed = start.elapsed();

        assert!(elapsed < Duration::from_secs(1), "Should shutdown quickly");
    }
}
