//! StrataDB - Production-grade embedded database for AI agents
//!
//! StrataDB is an embedded database designed for AI agents, providing six primitives
//! for different data patterns: KV, State, Event, JSON, Vector, and Run.
//!
//! # Quick Start
//!
//! ```ignore
//! use stratadb::{Strata, Command, Output, Value};
//!
//! // Create an in-memory database
//! let db = Strata::ephemeral()?;
//!
//! // Store a value (uses the default run)
//! db.kv_put("user:123", Value::String("Alice".into()))?;
//!
//! // Retrieve it
//! let value = db.kv_get("user:123")?;
//! ```
//!
//! # Architecture
//!
//! All operations go through the [`Executor`] which provides a command-based API.
//! The [`Strata`] struct provides a convenient high-level interface.
//!
//! Internal implementation details (storage, concurrency, durability, engine)
//! are not exposed - only the executor API is public.

// Re-export the public API from strata-executor
pub use strata_executor::*;
