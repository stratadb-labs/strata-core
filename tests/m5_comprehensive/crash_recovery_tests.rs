//! Crash Recovery Tests
//!
//! Tests for JSON document recovery after crashes:
//! - Document state recovery
//! - Partial write recovery
//! - Multi-document recovery
//! - Cross-run recovery

use crate::test_utils::*;
use tempfile::TempDir;

// =============================================================================
// Document State Recovery Tests
// =============================================================================

/// Document can be recovered after close and reopen.
#[test]
fn test_document_recovery_basic() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_recovery.db");

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Phase 1: Create and modify document
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        store
            .set(&run_id, &doc_id, &path("name"), JsonValue::from("Alice"))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("age"), JsonValue::from(30i64))
            .unwrap();
        store
            .set(
                &run_id,
                &doc_id,
                &path("settings.theme"),
                JsonValue::from("dark"),
            )
            .unwrap();

        // Verify before close
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("name"))
                .unwrap()
                .unwrap().value.as_str(),
            Some("Alice")
        );
    }

    // Phase 2: Reopen and verify recovery
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        assert!(store.exists(&run_id, &doc_id).unwrap());
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("name"))
                .unwrap()
                .unwrap().value.as_str(),
            Some("Alice")
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("age"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(30)
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("settings.theme"))
                .unwrap()
                .unwrap().value.as_str(),
            Some("dark")
        );
    }
}

/// Version is recovered correctly.
#[test]
fn test_version_recovery() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_version.db");

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let expected_version: u64;

    // Phase 1: Create document with multiple operations
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        for i in 0..10 {
            store
                .set(
                    &run_id,
                    &doc_id,
                    &path(&format!("key{}", i)),
                    JsonValue::from(i as i64),
                )
                .unwrap();
        }

        expected_version = store.get_version(&run_id, &doc_id).unwrap().unwrap();
        assert_eq!(expected_version, 11); // 1 create + 10 sets
    }

    // Phase 2: Verify version recovery
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        let recovered_version = store.get_version(&run_id, &doc_id).unwrap().unwrap();
        assert_eq!(recovered_version, expected_version);
    }
}

/// Deleted document stays deleted after recovery.
#[test]
fn test_deleted_document_stays_deleted() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_deleted.db");

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Phase 1: Create and then destroy document
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();
        assert!(store.exists(&run_id, &doc_id).unwrap());

        store.destroy(&run_id, &doc_id).unwrap();
        assert!(!store.exists(&run_id, &doc_id).unwrap());
    }

    // Phase 2: Verify still deleted after recovery
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        assert!(!store.exists(&run_id, &doc_id).unwrap());
    }
}

// =============================================================================
// Multi-Document Recovery Tests
// =============================================================================

/// Multiple documents recover correctly.
#[test]
fn test_multi_document_recovery() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_multi.db");

    let run_id = RunId::new();
    let doc_ids: Vec<JsonDocId> = (0..5).map(|_| JsonDocId::new()).collect();

    // Phase 1: Create multiple documents
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        for (i, doc_id) in doc_ids.iter().enumerate() {
            store
                .create(&run_id, doc_id, JsonValue::from(i as i64))
                .unwrap();
        }
    }

    // Phase 2: Verify all documents recovered
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        for (i, doc_id) in doc_ids.iter().enumerate() {
            assert!(store.exists(&run_id, doc_id).unwrap());
            assert_eq!(
                store
                    .get(&run_id, doc_id, &root())
                    .unwrap()
                    .unwrap().value.as_i64(),
                Some(i as i64)
            );
        }
    }
}

/// Interleaved operations on multiple documents recover correctly.
#[test]
fn test_interleaved_operations_recovery() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_interleaved.db");

    let run_id = RunId::new();
    let doc1 = JsonDocId::new();
    let doc2 = JsonDocId::new();

    // Phase 1: Interleaved operations
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store.create(&run_id, &doc1, JsonValue::object()).unwrap();
        store.create(&run_id, &doc2, JsonValue::object()).unwrap();

        store
            .set(&run_id, &doc1, &path("a"), JsonValue::from(1i64))
            .unwrap();
        store
            .set(&run_id, &doc2, &path("x"), JsonValue::from(10i64))
            .unwrap();
        store
            .set(&run_id, &doc1, &path("b"), JsonValue::from(2i64))
            .unwrap();
        store
            .set(&run_id, &doc2, &path("y"), JsonValue::from(20i64))
            .unwrap();
        store.delete_at_path(&run_id, &doc1, &path("a")).unwrap();
        store
            .set(&run_id, &doc2, &path("z"), JsonValue::from(30i64))
            .unwrap();
    }

    // Phase 2: Verify interleaved operations recovered correctly
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        // doc1: a deleted, b = 2
        assert!(store.get(&run_id, &doc1, &path("a")).unwrap().is_none());
        assert_eq!(
            store
                .get(&run_id, &doc1, &path("b"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(2)
        );

        // doc2: x = 10, y = 20, z = 30
        assert_eq!(
            store
                .get(&run_id, &doc2, &path("x"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(10)
        );
        assert_eq!(
            store
                .get(&run_id, &doc2, &path("y"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(20)
        );
        assert_eq!(
            store
                .get(&run_id, &doc2, &path("z"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(30)
        );
    }
}

// =============================================================================
// Cross-Run Recovery Tests
// =============================================================================

/// Documents in different runs recover independently.
#[test]
fn test_cross_run_recovery() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_cross_run.db");

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    // Phase 1: Create same doc_id in different runs
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store
            .create(&run1, &doc_id, JsonValue::from(100i64))
            .unwrap();
        store
            .create(&run2, &doc_id, JsonValue::from(200i64))
            .unwrap();

        store
            .set(&run1, &doc_id, &root(), JsonValue::from(111i64))
            .unwrap();
        store
            .set(&run2, &doc_id, &root(), JsonValue::from(222i64))
            .unwrap();
    }

    // Phase 2: Verify run isolation preserved after recovery
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        assert_eq!(
            store
                .get(&run1, &doc_id, &root())
                .unwrap()
                .unwrap().value.as_i64(),
            Some(111)
        );
        assert_eq!(
            store
                .get(&run2, &doc_id, &root())
                .unwrap()
                .unwrap().value.as_i64(),
            Some(222)
        );
    }
}

// =============================================================================
// Complex Structure Recovery Tests
// =============================================================================

/// Complex nested JSON structure recovers correctly.
#[test]
fn test_complex_structure_recovery() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_complex.db");

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let complex_value: JsonValue = serde_json::json!({
        "users": [
            {
                "name": "Alice",
                "age": 30,
                "contacts": {
                    "email": "alice@example.com",
                    "phone": "+1234567890"
                }
            },
            {
                "name": "Bob",
                "age": 25,
                "contacts": {
                    "email": "bob@example.com"
                }
            }
        ],
        "metadata": {
            "version": 1,
            "created": "2024-01-01"
        }
    })
    .into();

    // Phase 1: Create complex document
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store
            .create(&run_id, &doc_id, complex_value.clone())
            .unwrap();

        // Modify nested value
        store
            .set(
                &run_id,
                &doc_id,
                &path("users[0].age"),
                JsonValue::from(31i64),
            )
            .unwrap();
        store
            .set(
                &run_id,
                &doc_id,
                &path("metadata.version"),
                JsonValue::from(2i64),
            )
            .unwrap();
    }

    // Phase 2: Verify complex structure recovered
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("users[0].name"))
                .unwrap()
                .unwrap().value.as_str(),
            Some("Alice")
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("users[0].age"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(31) // Modified value
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("users[1].name"))
                .unwrap()
                .unwrap().value.as_str(),
            Some("Bob")
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("metadata.version"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(2) // Modified value
        );
    }
}

// =============================================================================
// Recovery Sequence Tests
// =============================================================================

/// Multiple close/reopen cycles preserve state.
#[test]
fn test_multiple_recovery_cycles() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_cycles.db");

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Cycle 1: Create
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);
        store
            .create(&run_id, &doc_id, JsonValue::from(1i64))
            .unwrap();
    }

    // Cycle 2: Modify
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);
        store
            .set(&run_id, &doc_id, &root(), JsonValue::from(2i64))
            .unwrap();
    }

    // Cycle 3: Modify again
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);
        store
            .set(&run_id, &doc_id, &root(), JsonValue::from(3i64))
            .unwrap();
    }

    // Cycle 4: Verify
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);
        assert_eq!(
            store
                .get(&run_id, &doc_id, &root())
                .unwrap()
                .unwrap().value.as_i64(),
            Some(3)
        );
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap().unwrap(), 3);
    }
}

/// Operations continue correctly after recovery.
#[test]
fn test_operations_after_recovery() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test_continue.db");

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Phase 1: Initial operations
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
            .unwrap();
    }

    // Phase 2: Continue operations after recovery
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        // Can continue modifying
        store
            .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
            .unwrap();
        store.delete_at_path(&run_id, &doc_id, &path("a")).unwrap();

        // Version continues from where it left off
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap().unwrap(), 5);
    }

    // Phase 3: Verify final state
    {
        let db = create_persistent_db(&db_path);
        let store = JsonStore::new(db);

        assert!(store.get(&run_id, &doc_id, &path("a")).unwrap().is_none());
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("b"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(2)
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("c"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(3)
        );
    }
}
