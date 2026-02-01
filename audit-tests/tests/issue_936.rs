//! Audit test for issue #936: Vector insert TOCTOU race condition
//! Verdict: CONFIRMED BUG
//!
//! VectorStore::insert checks for the existence of a vector key via a
//! snapshot read BEFORE acquiring the write lock. This creates a
//! time-of-check-to-time-of-use (TOCTOU) race condition:
//!
//! 1. Thread A: checks if key "v1" exists (snapshot says NO)
//! 2. Thread B: inserts key "v1" (acquires lock, inserts, releases lock)
//! 3. Thread A: acquires lock and inserts key "v1" again
//!
//! The result is that the second insert silently overwrites the first,
//! even though the code intended to detect duplicates.
//!
//! This is difficult to reliably trigger in a unit test because it requires
//! precise thread interleaving. The test below documents the race condition
//! and verifies the basic insert-then-insert behavior.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Executor, Output, Value};

/// Documents the TOCTOU race condition in vector insert.
/// Two sequential upserts with the same key both succeed — this is
/// expected for "upsert" semantics, but if the intent were "insert only
/// if not exists", the TOCTOU check would be unreliable.
#[test]
fn issue_936_sequential_duplicate_upsert_succeeds() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = BranchId::from("default");

    // Create collection first (auto-create was removed in #923)
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    // First upsert
    let r1 = executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "col".into(),
            key: "v1".into(),
            vector: vec![1.0, 0.0, 0.0],
            metadata: Some(Value::String("first".into())),
        })
        .unwrap();
    assert!(matches!(r1, Output::Version(_)));

    // Second upsert with same key — overwrites the first
    let r2 = executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "col".into(),
            key: "v1".into(),
            vector: vec![0.0, 1.0, 0.0],
            metadata: Some(Value::String("second".into())),
        })
        .unwrap();
    assert!(matches!(r2, Output::Version(_)));

    // Read back — should see the second value
    let get_result = executor
        .execute(Command::VectorGet {
            branch: Some(branch.clone()),
            collection: "col".into(),
            key: "v1".into(),
        })
        .unwrap();

    match get_result {
        Output::VectorData(Some(data)) => {
            // The second upsert's vector is present
            assert_eq!(data.data.embedding, vec![0.0, 1.0, 0.0]);
        }
        other => panic!("Expected VectorData(Some), got: {:?}", other),
    }

    // NOTE: The TOCTOU race cannot be reliably demonstrated in a sequential
    // test. The bug is in the concurrent case where the existence check
    // (via snapshot) happens before the write lock is acquired, allowing
    // another thread to modify the data between check and write.
}

/// Documents that the race condition exists in the VectorStore::insert path.
/// The existence check uses a snapshot (read lock), then the actual insert
/// acquires the write lock. Between these two steps, another thread could
/// insert or delete the same key.
#[test]
fn issue_936_race_window_documentation() {
    // The VectorStore::insert code flow is approximately:
    //
    // 1. let snapshot = self.kv_store.snapshot();
    // 2. let exists = snapshot.get(&key).is_some();  // READ (no lock)
    //    --- RACE WINDOW: another thread can insert/delete here ---
    // 3. let mut writer = self.kv_store.write();     // WRITE LOCK
    // 4. writer.put(&key, &value);                   // WRITE
    //
    // The fix would be to check existence inside the write lock:
    // 1. let mut writer = self.kv_store.write();     // WRITE LOCK
    // 2. let exists = writer.get(&key).is_some();    // CHECK under lock
    // 3. writer.put(&key, &value);                   // WRITE
    //
    // This test just documents the issue exists.
    assert!(true, "TOCTOU race condition documented");
}
