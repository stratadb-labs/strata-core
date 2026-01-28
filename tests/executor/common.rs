//! Common test utilities for executor tests

use std::sync::Arc;
use strata_engine::Database;
use strata_executor::{Command, Executor, Output, Session, Strata};

/// Create an executor with an in-memory database
pub fn create_executor() -> Executor {
    let db = Arc::new(Database::builder().no_durability().open_temp().unwrap());
    Executor::new(db)
}

/// Create a Strata API wrapper with an in-memory database
pub fn create_strata() -> Strata {
    let db = Arc::new(Database::builder().no_durability().open_temp().unwrap());
    Strata::new(db)
}

/// Create a Session with an in-memory database
pub fn create_session() -> Session {
    let db = Arc::new(Database::builder().no_durability().open_temp().unwrap());
    Session::new(db)
}

/// Create a database for shared use
pub fn create_db() -> Arc<Database> {
    Arc::new(Database::builder().no_durability().open_temp().unwrap())
}

/// Helper to create an event payload (must be an Object)
pub fn event_payload(key: &str, value: strata_core::Value) -> strata_core::Value {
    strata_core::Value::Object(
        [(key.to_string(), value)].into_iter().collect()
    )
}

/// Extract version from Output::Version
pub fn extract_version(output: &Output) -> u64 {
    match output {
        Output::Version(v) => *v,
        _ => panic!("Expected Output::Version, got {:?}", output),
    }
}

/// Extract bool from Output::Bool
pub fn extract_bool(output: &Output) -> bool {
    match output {
        Output::Bool(b) => *b,
        _ => panic!("Expected Output::Bool, got {:?}", output),
    }
}
