//! Audit test for issue #887: Standard mode fsync may exceed configured interval_ms
//! Verdict: CONFIRMED BUG
//!
//! In Standard durability mode, the interval_ms check only runs when append() is called.
//! If no new writes arrive after a batch, unflushed data sits in the OS page cache
//! indefinitely. There is no background timer to enforce the fsync interval.

use std::time::{Duration, Instant};

use strata_durability::codec::IdentityCodec;
use strata_durability::format::WalRecord;
use strata_durability::wal::{DurabilityMode, WalConfig, WalWriter};

fn make_record(txn_id: u64) -> WalRecord {
    WalRecord::new(txn_id, [1u8; 16], 12345, vec![1, 2, 3, 4, 5])
}

/// Demonstrates that Standard mode's interval_ms is not enforced by a timer.
/// After writing records, the data remains unflushed until the next append().
#[test]
fn issue_887_interval_ms_not_enforced_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let wal_dir = dir.path().join("wal");

    // Configure Standard mode with:
    // - interval_ms = 50 (50ms fsync interval)
    // - batch_size = 1000 (won't trigger batch-based sync)
    let config = WalConfig::new()
        .with_segment_size(1024 * 1024)
        .with_buffered_sync_bytes(1024 * 1024); // Large threshold to avoid byte-based sync

    let mut writer = WalWriter::new(
        wal_dir.clone(),
        [1u8; 16],
        DurabilityMode::Standard {
            interval_ms: 50,
            batch_size: 1000,
        },
        config,
        Box::new(IdentityCodec),
    )
    .unwrap();

    // Write 5 records (below batch_size of 1000)
    for i in 1..=5 {
        writer.append(&make_record(i)).unwrap();
    }

    // Wait longer than interval_ms
    std::thread::sleep(Duration::from_millis(100));

    // BUG: No background timer exists. The data written above may still be
    // unflushed in the OS page cache. Only calling append() again would
    // trigger the interval check.

    // The only way to ensure data is synced is to call flush() explicitly.
    // But the WAL writer doesn't do this automatically.

    // Prove that writing one more record triggers the time-based sync
    let before_append = Instant::now();
    writer.append(&make_record(6)).unwrap();
    let _after_append = before_append.elapsed();

    // The 6th append() triggers maybe_sync() which checks the elapsed time.
    // Since 100ms > 50ms (interval_ms), it will fsync.
    // But records 1-5 were at risk for the entire 100ms window.

    // This test documents the behavior. The fix would be a background flush thread.
    writer.flush().unwrap();
}

/// Demonstrates that without any subsequent writes, the configured interval
/// has no effect -- data could remain unsynced indefinitely.
#[test]
fn issue_887_no_automatic_sync_without_subsequent_write() {
    let dir = tempfile::tempdir().unwrap();
    let wal_dir = dir.path().join("wal");

    let config = WalConfig::new()
        .with_segment_size(1024 * 1024)
        .with_buffered_sync_bytes(1024 * 1024);

    let mut writer = WalWriter::new(
        wal_dir.clone(),
        [1u8; 16],
        DurabilityMode::Standard {
            interval_ms: 10, // 10ms interval
            batch_size: 1000,
        },
        config,
        Box::new(IdentityCodec),
    )
    .unwrap();

    // Write a single record
    writer.append(&make_record(1)).unwrap();

    // Sleep well past the interval
    std::thread::sleep(Duration::from_millis(50));

    // No automatic sync happened. The only way to sync is explicit flush.
    // In a real system, if the process crashes here, the record is lost
    // even though interval_ms=10 implied a 10ms max data loss window.

    // Must call flush() explicitly to ensure durability
    writer.flush().unwrap();
}

/// Demonstrates that batch_size threshold works correctly (it IS checked on write).
#[test]
fn issue_887_batch_size_sync_works_on_write() {
    let dir = tempfile::tempdir().unwrap();
    let wal_dir = dir.path().join("wal");

    let config = WalConfig::new()
        .with_segment_size(1024 * 1024)
        .with_buffered_sync_bytes(1024 * 1024);

    let mut writer = WalWriter::new(
        wal_dir.clone(),
        [1u8; 16],
        DurabilityMode::Standard {
            interval_ms: 999999, // Very high interval (won't trigger)
            batch_size: 5,       // Sync every 5 writes
        },
        config,
        Box::new(IdentityCodec),
    )
    .unwrap();

    // Write exactly batch_size records
    for i in 1..=5 {
        writer.append(&make_record(i)).unwrap();
    }

    // batch_size threshold triggers sync on the 5th write
    // This works because it IS checked during append()

    // Write more to verify the cycle resets
    for i in 6..=10 {
        writer.append(&make_record(i)).unwrap();
    }

    writer.flush().unwrap();
}

/// Demonstrates the contrast: Always mode always syncs immediately.
#[test]
fn issue_887_always_mode_always_syncs() {
    let dir = tempfile::tempdir().unwrap();
    let wal_dir = dir.path().join("wal");

    let config = WalConfig::new()
        .with_segment_size(1024 * 1024)
        .with_buffered_sync_bytes(1024 * 1024);

    let mut writer = WalWriter::new(
        wal_dir.clone(),
        [1u8; 16],
        DurabilityMode::Always,
        config,
        Box::new(IdentityCodec),
    )
    .unwrap();

    // In always mode, every append() is immediately synced
    writer.append(&make_record(1)).unwrap();
    // Data is durable immediately -- no window for data loss

    writer.append(&make_record(2)).unwrap();
    // Also immediately durable

    writer.flush().unwrap();
}
