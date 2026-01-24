//! EventLog Concurrency Tests
//!
//! Tests for multi-threaded safety and ordering:
//! - Concurrent appends are serialized correctly
//! - No lost events under contention
//! - Sequence ordering is total
//! - Concurrent reads are safe
//!
//! All test data is loaded from testdata/eventlog_test_data.jsonl

use crate::test_data::load_eventlog_test_data;
use crate::*;
use std::collections::HashMap;
use std::sync::{Arc, Barrier};
use std::thread;

// =============================================================================
// CONCURRENT APPENDS
// =============================================================================

#[test]
fn test_concurrent_appends_no_lost_events() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and prepare payloads for threads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(100).map(|e| e.payload.clone()).collect()
    );

    let thread_count = 4;
    let events_per_thread = 25;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                for i in 0..events_per_thread {
                    // Use payload from test data, cycling through available payloads
                    let payload_idx = (thread_id * events_per_thread + i) % payloads.len();
                    let payload = payloads[payload_idx].clone();
                    substrate
                        .event_append(&run, "conc_stream1", payload)
                        .expect("append should succeed");
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify no events lost
    let substrate = SubstrateImpl::new((*db).clone());
    let len = substrate
        .event_len(&run, "conc_stream1")
        .expect("len should succeed");

    let expected = (thread_count * events_per_thread) as u64;
    assert_eq!(
        len, expected,
        "Should have {} events, got {}",
        expected, len
    );
}

#[test]
fn test_concurrent_appends_sequence_monotonic() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and prepare payloads for threads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(40).map(|e| e.payload.clone()).collect()
    );

    let thread_count = 4;
    let events_per_thread = 10;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                let mut sequences = Vec::new();
                for i in 0..events_per_thread {
                    let payload_idx = (thread_id * events_per_thread + i) % payloads.len();
                    let payload = payloads[payload_idx].clone();
                    let version = substrate
                        .event_append(&run, "seq_mono_stream", payload)
                        .expect("append should succeed");

                    if let Version::Sequence(seq) = version {
                        sequences.push(seq);
                    }
                }
                sequences
            })
        })
        .collect();

    let mut all_sequences: Vec<u64> = Vec::new();
    for handle in handles {
        let seqs = handle.join().expect("Thread should complete");
        all_sequences.extend(seqs);
    }

    // Verify all sequences are unique
    let mut sorted_seqs = all_sequences.clone();
    sorted_seqs.sort();
    sorted_seqs.dedup();
    assert_eq!(
        sorted_seqs.len(),
        all_sequences.len(),
        "All sequences should be unique"
    );

    // Verify no gaps in sequence space
    let min_seq = *sorted_seqs.first().unwrap();
    let max_seq = *sorted_seqs.last().unwrap();
    let expected_count = (max_seq - min_seq + 1) as usize;
    assert_eq!(
        sorted_seqs.len(),
        expected_count,
        "Sequences should be contiguous"
    );
}

#[test]
fn test_concurrent_appends_to_different_streams() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and prepare payloads for threads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(100).map(|e| e.payload.clone()).collect()
    );

    let thread_count = 4;
    let events_per_thread = 25;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                let stream = format!("diff_stream_{}", thread_id);
                barrier.wait();

                for i in 0..events_per_thread {
                    let payload_idx = (thread_id * events_per_thread + i) % payloads.len();
                    let payload = payloads[payload_idx].clone();
                    substrate
                        .event_append(&run, &stream, payload)
                        .expect("append should succeed");
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify each stream has correct count
    let substrate = SubstrateImpl::new((*db).clone());
    for thread_id in 0..thread_count {
        let stream = format!("diff_stream_{}", thread_id);
        let len = substrate
            .event_len(&run, &stream)
            .expect("len should succeed");
        assert_eq!(
            len, events_per_thread as u64,
            "Stream {} should have {} events",
            stream, events_per_thread
        );
    }
}

// =============================================================================
// CONCURRENT READS
// =============================================================================

#[test]
fn test_concurrent_reads_safe() {
    let (db, substrate) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and pre-populate with events
    let test_data = load_eventlog_test_data();
    for entry in test_data.take(50) {
        substrate
            .event_append(&run, "read_safe_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    let thread_count = 8;
    let reads_per_thread = 20;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|_| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                let mut results = Vec::new();
                for _ in 0..reads_per_thread {
                    let events = substrate
                        .event_range(&run, "read_safe_stream", None, None, None)
                        .expect("range should succeed");
                    results.push(events.len());

                    let len = substrate
                        .event_len(&run, "read_safe_stream")
                        .expect("len should succeed");
                    results.push(len as usize);
                }
                results
            })
        })
        .collect();

    for handle in handles {
        let results = handle.join().expect("Thread should complete");
        // All reads should see 50 events
        for result in results {
            assert_eq!(result, 50, "Concurrent reads should see consistent data");
        }
    }
}

#[test]
fn test_concurrent_reads_and_writes() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and prepare payloads for threads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(100).map(|e| e.payload.clone()).collect()
    );

    let writer_count = 2;
    let reader_count = 4;
    let events_per_writer = 50;
    let reads_per_reader = 20;
    let barrier = Arc::new(Barrier::new(writer_count + reader_count));

    // Writer threads
    let writer_handles: Vec<_> = (0..writer_count)
        .map(|writer_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                for i in 0..events_per_writer {
                    let payload_idx = (writer_id * events_per_writer + i) % payloads.len();
                    let payload = payloads[payload_idx].clone();
                    substrate
                        .event_append(&run, "rw_stream", payload)
                        .expect("append should succeed");
                }
            })
        })
        .collect();

    // Reader threads
    let reader_handles: Vec<_> = (0..reader_count)
        .map(|_| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                let mut lens = Vec::new();
                for _ in 0..reads_per_reader {
                    let len = substrate
                        .event_len(&run, "rw_stream")
                        .expect("len should succeed");
                    lens.push(len);
                    // Small delay to spread reads over write period
                    thread::yield_now();
                }
                lens
            })
        })
        .collect();

    // Wait for writers
    for handle in writer_handles {
        handle.join().expect("Writer thread should complete");
    }

    // Wait for readers
    for handle in reader_handles {
        let lens = handle.join().expect("Reader thread should complete");
        // Lengths should be monotonically non-decreasing
        for window in lens.windows(2) {
            assert!(
                window[1] >= window[0],
                "Event count should never decrease: {} -> {}",
                window[0],
                window[1]
            );
        }
    }

    // Final verification
    let substrate = SubstrateImpl::new((*db).clone());
    let final_len = substrate
        .event_len(&run, "rw_stream")
        .expect("len should succeed");
    let expected = (writer_count * events_per_writer) as u64;
    assert_eq!(
        final_len, expected,
        "Should have {} total events",
        expected
    );
}

// =============================================================================
// RUN ISOLATION UNDER CONCURRENCY
// =============================================================================

#[test]
fn test_run_isolation_under_concurrency() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);

    // Load test data and prepare payloads for threads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(100).map(|e| e.payload.clone()).collect()
    );

    let thread_count = 4;
    let events_per_thread = 25;
    let barrier = Arc::new(Barrier::new(thread_count));

    // Each thread uses its own run
    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                let run = ApiRunId::new(); // Each thread has its own run
                barrier.wait();

                for i in 0..events_per_thread {
                    let payload_idx = (thread_id * events_per_thread + i) % payloads.len();
                    let payload = payloads[payload_idx].clone();
                    substrate
                        .event_append(&run, "iso_conc_stream", payload)
                        .expect("append should succeed");
                }

                // Verify this run only sees its own events
                let len = substrate
                    .event_len(&run, "iso_conc_stream")
                    .expect("len should succeed");
                (run, len)
            })
        })
        .collect();

    for handle in handles {
        let (run, len) = handle.join().expect("Thread should complete");
        assert_eq!(
            len, events_per_thread as u64,
            "Run {:?} should have exactly {} events",
            run, events_per_thread
        );
    }
}

// =============================================================================
// STRESS TESTS
// =============================================================================

#[test]
fn test_high_contention_single_stream() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and prepare payloads for threads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(400).map(|e| e.payload.clone()).collect()
    );

    let thread_count = 8;
    let events_per_thread = 50;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                let mut success_count = 0;
                for i in 0..events_per_thread {
                    let payload_idx = (thread_id * events_per_thread + i) % payloads.len();
                    let payload = payloads[payload_idx].clone();
                    match substrate.event_append(&run, "hot_stream", payload) {
                        Ok(_) => success_count += 1,
                        Err(e) => panic!("Append failed under contention: {:?}", e),
                    }
                }
                success_count
            })
        })
        .collect();

    let mut total_success = 0;
    for handle in handles {
        total_success += handle.join().expect("Thread should complete");
    }

    // All appends should succeed (single-writer-ordered guarantees this)
    let expected = thread_count * events_per_thread;
    assert_eq!(
        total_success, expected,
        "All {} appends should succeed",
        expected
    );

    // Verify final count
    let substrate = SubstrateImpl::new((*db).clone());
    let final_len = substrate
        .event_len(&run, "hot_stream")
        .expect("len should succeed");
    assert_eq!(
        final_len, expected as u64,
        "Final count should be {}",
        expected
    );
}

#[test]
fn test_mixed_operations_stress() {
    let (db, _) = quick_setup();
    let db = Arc::new(db);
    let run = ApiRunId::default();

    // Load test data and prepare payloads
    let test_data = load_eventlog_test_data();
    let payloads: Arc<Vec<Value>> = Arc::new(
        test_data.entries.iter().take(200).map(|e| e.payload.clone()).collect()
    );

    // Pre-populate
    {
        let substrate = SubstrateImpl::new((*db).clone());
        for i in 0..10 {
            let payload = payloads[i % payloads.len()].clone();
            substrate
                .event_append(&run, "mixed_stream", payload)
                .expect("append should succeed");
        }
    }

    let thread_count = 6;
    let ops_per_thread = 30;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|thread_id| {
            let db = db.clone();
            let barrier = barrier.clone();
            let run = run.clone();
            let payloads = payloads.clone();

            thread::spawn(move || {
                let substrate = SubstrateImpl::new((*db).clone());
                barrier.wait();

                for i in 0..ops_per_thread {
                    match i % 4 {
                        0 => {
                            // Append
                            let payload_idx = (thread_id * ops_per_thread + i) % payloads.len();
                            let payload = payloads[payload_idx].clone();
                            substrate
                                .event_append(&run, "mixed_stream", payload)
                                .expect("append should succeed");
                        }
                        1 => {
                            // Range read
                            let _ = substrate
                                .event_range(&run, "mixed_stream", None, None, Some(10))
                                .expect("range should succeed");
                        }
                        2 => {
                            // Len
                            let _ = substrate
                                .event_len(&run, "mixed_stream")
                                .expect("len should succeed");
                        }
                        _ => {
                            // Latest sequence
                            let _ = substrate
                                .event_latest_sequence(&run, "mixed_stream")
                                .expect("latest_sequence should succeed");
                        }
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify data integrity
    let substrate = SubstrateImpl::new((*db).clone());
    let events = substrate
        .event_range(&run, "mixed_stream", None, None, None)
        .expect("range should succeed");

    // Should have initial 10 + appends from threads
    // Each thread does ops_per_thread / 4 appends
    let appends_per_thread = ops_per_thread / 4 + (if ops_per_thread % 4 > 0 { 1 } else { 0 });
    let expected_min = 10 + (thread_count * (ops_per_thread / 4));
    assert!(
        events.len() >= expected_min,
        "Should have at least {} events, got {}",
        expected_min,
        events.len()
    );
}
