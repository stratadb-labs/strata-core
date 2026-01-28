//! M12 Unified API Surface Tests
//!
//! Tests for the new unified API entry point (`strata` crate).
//! Validates progressive disclosure pattern and ergonomic API design.

use stratadb::prelude::*;
use stratadb::DistanceMetric;
use tempfile::TempDir;

// ============================================================================
// Database Lifecycle Tests
// ============================================================================

mod lifecycle {
    use super::*;

    #[test]
    fn test_open_database() {
        let temp_dir = TempDir::new().unwrap();
        let db = Strata::open(temp_dir.path().join("test_db")).unwrap();
        assert!(temp_dir.path().join("test_db").exists());
        drop(db);
    }

    #[test]
    fn test_builder_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let db = StrataBuilder::new()
            .path(temp_dir.path().join("builder_db"))
            .open()
            .unwrap();
        assert!(temp_dir.path().join("builder_db").exists());
        drop(db);
    }

    #[test]
    fn test_builder_in_memory() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();
        // Should work without explicit path
        drop(db);
    }
}

// ============================================================================
// KV Primitive Tests - Progressive Disclosure
// ============================================================================

mod kv_tests {
    use super::*;

    #[test]
    fn test_kv_simple_set_get() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // Level 1: Simple API - clean ergonomic syntax!
        db.kv.set("key1", "value1").unwrap();
        let value = db.kv.get("key1").unwrap();
        assert!(value.is_some());
    }

    #[test]
    fn test_kv_run_scoped() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // Create a run
        let run = db.runs.create(None).unwrap();

        // Level 2: Run-scoped API
        db.kv.set_in(&run, "key1", "value1").unwrap();
        let value = db.kv.get_in(&run, "key1").unwrap();
        assert!(value.is_some());
    }

    #[test]
    fn test_kv_full_control() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run = db.runs.create(None).unwrap();

        // Level 3: Full control API - returns Version
        let _version = db.kv.put(&run, "key1", "value1").unwrap();

        let versioned = db.kv.get_in(&run, "key1").unwrap();
        assert!(versioned.is_some());
    }

    #[test]
    fn test_kv_exists_and_delete() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        db.kv.set("test_key", "test_value").unwrap();
        assert!(db.kv.exists("test_key").unwrap());

        db.kv.delete("test_key").unwrap();
        assert!(!db.kv.exists("test_key").unwrap());
    }

    #[test]
    fn test_kv_with_different_types() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // String
        db.kv.set("string_key", "hello").unwrap();

        // Integer
        db.kv.set("int_key", 42i64).unwrap();

        // Boolean
        db.kv.set("bool_key", true).unwrap();

        // Float
        db.kv.set("float_key", 3.14f64).unwrap();

        // Verify values
        let s = db.kv.get("string_key").unwrap().unwrap();
        assert_eq!(s.value.as_str(), Some("hello"));

        let i = db.kv.get("int_key").unwrap().unwrap();
        assert_eq!(i.value.as_int(), Some(42));

        let b = db.kv.get("bool_key").unwrap().unwrap();
        assert_eq!(b.value.as_bool(), Some(true));

        let f = db.kv.get("float_key").unwrap().unwrap();
        assert!((f.value.as_float().unwrap() - 3.14).abs() < 0.001);
    }
}

// ============================================================================
// JSON Primitive Tests - Progressive Disclosure
// ============================================================================

mod json_tests {
    use super::*;

    #[test]
    fn test_json_simple_set_get() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // JSON primitive uses Value, construct manually
        let mut obj = std::collections::HashMap::new();
        obj.insert("name".to_string(), Value::String("Alice".into()));
        obj.insert("age".to_string(), Value::Int(30));

        db.json.set("doc1", Value::Object(obj)).unwrap();
        let doc = db.json.get("doc1").unwrap();
        assert!(doc.is_some());
    }

    #[test]
    fn test_json_run_scoped() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run = db.runs.create(None).unwrap();

        let mut obj = std::collections::HashMap::new();
        obj.insert("foo".to_string(), Value::String("bar".into()));

        db.json.set_in(&run, "doc1", Value::Object(obj)).unwrap();
        let doc = db.json.get_in(&run, "doc1").unwrap();
        assert!(doc.is_some());
    }

    #[test]
    fn test_json_run_scoped_returns_version() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run = db.runs.create(None).unwrap();

        let mut obj = std::collections::HashMap::new();
        obj.insert("x".to_string(), Value::Int(1));

        // set_in returns Version for versioning info
        let _version = db.json.set_in(&run, "doc1", Value::Object(obj)).unwrap();

        let versioned = db.json.get_in(&run, "doc1").unwrap();
        assert!(versioned.is_some());
    }
}

// ============================================================================
// Events Primitive Tests - Progressive Disclosure
// ============================================================================

mod events_tests {
    use super::*;

    #[test]
    fn test_events_simple_append_read() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let mut obj = std::collections::HashMap::new();
        obj.insert("event".to_string(), Value::String("test".into()));

        db.events.append("stream1", Value::Object(obj)).unwrap();
        let events = db.events.read("stream1", 10).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_events_run_scoped() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run = db.runs.create(None).unwrap();

        let mut obj = std::collections::HashMap::new();
        obj.insert("event".to_string(), Value::String("scoped".into()));

        db.events
            .append_in(&run, "stream1", Value::Object(obj))
            .unwrap();
        let events = db.events.read_in(&run, "stream1", 10).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_events_count() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();
        let run = db.runs.create(None).unwrap();

        // Event payloads must be Objects
        let mut obj1 = std::collections::HashMap::new();
        obj1.insert("n".to_string(), Value::Int(1));
        db.events.append_in(&run, "counter", Value::Object(obj1)).unwrap();

        let mut obj2 = std::collections::HashMap::new();
        obj2.insert("n".to_string(), Value::Int(2));
        db.events.append_in(&run, "counter", Value::Object(obj2)).unwrap();

        let mut obj3 = std::collections::HashMap::new();
        obj3.insert("n".to_string(), Value::Int(3));
        db.events.append_in(&run, "counter", Value::Object(obj3)).unwrap();

        let count = db.events.count(&run, "counter").unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_events_head() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();
        let run = db.runs.create(None).unwrap();

        // Event payloads must be Objects
        let mut obj1 = std::collections::HashMap::new();
        obj1.insert("seq".to_string(), Value::Int(1));
        db.events.append_in(&run, "stream", Value::Object(obj1)).unwrap();

        let mut obj2 = std::collections::HashMap::new();
        obj2.insert("seq".to_string(), Value::Int(2));
        db.events.append_in(&run, "stream", Value::Object(obj2)).unwrap();

        let latest = db.events.head(&run, "stream").unwrap();
        assert!(latest.is_some());
    }
}

// ============================================================================
// State Primitive Tests - Progressive Disclosure
// ============================================================================

mod state_tests {
    use super::*;

    #[test]
    fn test_state_simple_set_get() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // Clean syntax with From impl
        db.state.set("cell1", "active").unwrap();
        let state = db.state.get("cell1").unwrap();
        assert!(state.is_some());
    }

    #[test]
    fn test_state_run_scoped() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run = db.runs.create(None).unwrap();

        db.state.set_in(&run, "cell1", 0i64).unwrap();
        let state = db.state.get_in(&run, "cell1").unwrap();
        assert!(state.is_some());
    }

    #[test]
    fn test_state_set_multiple_times() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        db.state.set("counter", 0i64).unwrap();
        db.state.set("counter", 1i64).unwrap();

        let state = db.state.get("counter").unwrap().unwrap();
        // State should be updated to the latest value
        assert_eq!(state.value.as_int(), Some(1));
    }
}

// ============================================================================
// Vectors Primitive Tests - Progressive Disclosure
// ============================================================================

mod vectors_tests {
    use super::*;

    #[test]
    fn test_vectors_create_collection() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();
        let run = db.runs.create(None).unwrap();

        db.vectors
            .create_collection(&run, "embeddings", 128, DistanceMetric::Cosine)
            .unwrap();
        let collections = db.vectors.list_collections(&run).unwrap();
        assert!(collections.iter().any(|c| c.name == "embeddings"));
    }

    #[test]
    fn test_vectors_upsert_and_search() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();
        let run = db.runs.create(None).unwrap();

        db.vectors
            .create_collection(&run, "test", 4, DistanceMetric::Cosine)
            .unwrap();

        // Upsert vectors
        db.vectors
            .upsert(&run, "test", "vec1", &[1.0, 0.0, 0.0, 0.0], None)
            .unwrap();
        db.vectors
            .upsert(&run, "test", "vec2", &[0.0, 1.0, 0.0, 0.0], None)
            .unwrap();

        // Search
        let results = db
            .vectors
            .search(&run, "test", &[1.0, 0.0, 0.0, 0.0], 5, None)
            .unwrap();
        assert!(!results.is_empty());
        // First result should be vec1 since it's the exact match
        assert_eq!(results[0].key, "vec1");
    }
}

// ============================================================================
// Runs Primitive Tests
// ============================================================================

mod runs_tests {
    use super::*;

    #[test]
    fn test_runs_create_and_get() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run_id = db.runs.create(None).unwrap();
        let run_info = db.runs.get(&run_id).unwrap();
        assert!(run_info.is_some());
    }

    #[test]
    fn test_runs_list() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let _run1 = db.runs.create(None).unwrap();
        let _run2 = db.runs.create(None).unwrap();

        let runs = db.runs.list(None, Some(10)).unwrap();
        assert!(runs.len() >= 2);
    }

    #[test]
    fn test_runs_close() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        let run_id = db.runs.create(None).unwrap();
        db.runs.close(&run_id).unwrap();

        let run_info = db.runs.get(&run_id).unwrap().unwrap();
        // Run should be marked as closed
        assert!(!run_info.value.state.is_active());
    }
}

// ============================================================================
// Cross-Primitive Integration Tests
// ============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_all_primitives_in_single_run() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // Create a run to scope all operations
        let run = db.runs.create(None).unwrap();

        // Use all primitives within the same run - clean syntax!
        db.kv.set_in(&run, "config", "enabled").unwrap();

        let mut user_obj = std::collections::HashMap::new();
        user_obj.insert("name".to_string(), Value::from("Test"));
        db.json.set_in(&run, "user", user_obj).unwrap();

        let mut event_obj = std::collections::HashMap::new();
        event_obj.insert("action".to_string(), Value::from("create"));
        db.events
            .append_in(&run, "audit", Value::Object(event_obj))
            .unwrap();

        db.state.set_in(&run, "status", "running").unwrap();

        // Create vector collection and store
        db.vectors
            .create_collection(&run, "features", 4, DistanceMetric::Cosine)
            .unwrap();
        db.vectors
            .upsert(&run, "features", "f1", &[1.0, 0.0, 0.0, 0.0], None)
            .unwrap();

        // Verify all data is accessible
        assert!(db.kv.get_in(&run, "config").unwrap().is_some());
        assert!(db.json.get_in(&run, "user").unwrap().is_some());
        assert_eq!(db.events.count(&run, "audit").unwrap(), 1);
        assert!(db.state.get_in(&run, "status").unwrap().is_some());
        assert!(!db
            .vectors
            .search(&run, "features", &[1.0, 0.0, 0.0, 0.0], 1, None)
            .unwrap()
            .is_empty());

        // Close the run
        db.runs.close(&run).unwrap();
    }

    #[test]
    fn test_metrics_access() {
        let db = StrataBuilder::new().in_memory().open_temp().unwrap();

        // Clean syntax - just pass the value directly
        db.kv.set("k1", "v1").unwrap();
        db.kv.set("k2", "v2").unwrap();

        let metrics = db.metrics();
        // Should have some operations recorded
        assert!(metrics.operations > 0);
    }
}
