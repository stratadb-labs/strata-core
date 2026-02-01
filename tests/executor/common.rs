//! Common test utilities for executor tests

use std::sync::Arc;
use strata_engine::Database;
use strata_executor::{Executor, Output, Session, Strata};

/// Create an executor with an in-memory database
pub fn create_executor() -> Executor {
    let db = Database::ephemeral().unwrap();
    Executor::new(db)
}

/// Create a Strata API wrapper with an in-memory database
pub fn create_strata() -> Strata {
    Strata::open_temp().unwrap()
}

/// Create a Session with an in-memory database
pub fn create_session() -> Session {
    let db = Database::ephemeral().unwrap();
    Session::new(db)
}

/// Create a database for shared use
pub fn create_db() -> Arc<Database> {
    Database::ephemeral().unwrap()
}

/// Helper to create an event payload (must be an Object)
pub fn event_payload(key: &str, value: strata_core::Value) -> strata_core::Value {
    strata_core::Value::Object([(key.to_string(), value)].into_iter().collect())
}

/// Extract version from Output::Version
#[allow(dead_code)]
pub fn extract_version(output: &Output) -> u64 {
    match output {
        Output::Version(v) => *v,
        _ => panic!("Expected Output::Version, got {:?}", output),
    }
}

/// Extract bool from Output::Bool
#[allow(dead_code)]
pub fn extract_bool(output: &Output) -> bool {
    match output {
        Output::Bool(b) => *b,
        _ => panic!("Expected Output::Bool, got {:?}", output),
    }
}
