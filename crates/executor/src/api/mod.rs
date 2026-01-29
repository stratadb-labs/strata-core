//! High-level typed wrapper for the Executor.
//!
//! The [`Strata`] struct provides a convenient Rust API that wraps the
//! [`Executor`] and [`Command`]/[`Output`] enums with typed method calls.
//!
//! All data methods use the default run. Callers needing a specific run
//! should use `executor.execute(Command::... { run: Some(run_id), ... })`
//! directly.
//!
//! # Example
//!
//! ```ignore
//! use strata_executor::Strata;
//! use strata_core::Value;
//!
//! let db = Strata::new(substrate);
//!
//! // No run parameter - always uses default run
//! db.kv_put("key", Value::String("hello".into()))?;
//! let value = db.kv_get("key")?;
//! ```

mod db;
mod event;
mod json;
mod kv;
mod run;
mod state;
mod vector;

use std::sync::Arc;

use strata_engine::Database;

use crate::{Executor, Session};

/// High-level typed wrapper for database operations.
///
/// `Strata` provides a convenient Rust API that wraps the executor's
/// command-based interface with typed method calls. Each method:
///
/// 1. Creates the appropriate [`Command`] with `run: None`
/// 2. Executes it via the [`Executor`] (which resolves to the default run)
/// 3. Extracts and returns the typed result
pub struct Strata {
    executor: Executor,
}

impl Strata {
    /// Create a new Strata instance wrapping the given database.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            executor: Executor::new(db),
        }
    }

    /// Get the underlying executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Create a new [`Session`] for interactive transaction support.
    ///
    /// The returned session wraps a fresh executor and can manage an
    /// optional open transaction across multiple `execute()` calls.
    pub fn session(db: Arc<Database>) -> Session {
        Session::new(db)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::Value;
    use strata_engine::Database;
    use crate::types::*;

    fn create_strata() -> Strata {
        let db = Database::builder().no_durability().open_temp().unwrap();
        Strata::new(db)
    }

    #[test]
    fn test_ping() {
        let db = create_strata();
        let version = db.ping().unwrap();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_info() {
        let db = create_strata();
        let info = db.info().unwrap();
        assert!(!info.version.is_empty());
    }

    #[test]
    fn test_kv_put_get() {
        let db = create_strata();

        let version = db.kv_put("key1", Value::String("hello".into())).unwrap();
        assert!(version > 0);

        let value = db.kv_get("key1").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn test_kv_delete() {
        let db = create_strata();

        db.kv_put("key1", Value::Int(42)).unwrap();
        assert!(db.kv_get("key1").unwrap().is_some());

        let existed = db.kv_delete("key1").unwrap();
        assert!(existed);
        assert!(db.kv_get("key1").unwrap().is_none());
    }

    #[test]
    fn test_kv_list() {
        let db = create_strata();

        db.kv_put("user:1", Value::Int(1)).unwrap();
        db.kv_put("user:2", Value::Int(2)).unwrap();
        db.kv_put("task:1", Value::Int(3)).unwrap();

        let user_keys = db.kv_list(Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);

        let all_keys = db.kv_list(None).unwrap();
        assert_eq!(all_keys.len(), 3);
    }

    #[test]
    fn test_state_set_get() {
        let db = create_strata();

        db.state_set("cell", Value::String("state".into())).unwrap();
        let value = db.state_read("cell").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap().value, Value::String("state".into()));
    }

    #[test]
    fn test_event_append_range() {
        let db = create_strata();

        // Event payloads must be Objects
        db.event_append("stream", Value::Object(
            [("value".to_string(), Value::Int(1))].into_iter().collect()
        )).unwrap();
        db.event_append("stream", Value::Object(
            [("value".to_string(), Value::Int(2))].into_iter().collect()
        )).unwrap();

        let events = db.event_range("stream", None, None, None).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_vector_operations() {
        let db = create_strata();

        db.vector_create_collection("vecs", 4u64, DistanceMetric::Cosine).unwrap();
        db.vector_upsert("vecs", "v1", vec![1.0, 0.0, 0.0, 0.0], None).unwrap();
        db.vector_upsert("vecs", "v2", vec![0.0, 1.0, 0.0, 0.0], None).unwrap();

        let matches = db.vector_search("vecs", vec![1.0, 0.0, 0.0, 0.0], 10u64).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].key, "v1");
    }

    #[test]
    fn test_run_create_list() {
        let db = create_strata();

        let (info, _version) = db.run_create(
            Some("550e8400-e29b-41d4-a716-446655440099".to_string()),
            None,
        ).unwrap();
        assert_eq!(info.id.as_str(), "550e8400-e29b-41d4-a716-446655440099");

        let runs = db.run_list(None, None, None).unwrap();
        assert!(!runs.is_empty());
    }
}
