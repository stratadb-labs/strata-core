//! # Strata Executor
//!
//! The Command Execution Layer for Strata database.
//!
//! This crate provides a standardized interface between all external APIs
//! (Rust, Python, CLI, MCP) and the core database engine. Every operation
//! in Strata is represented as a typed, serializable [`Command`] that is
//! executed by the [`Executor`] to produce a typed [`Output`] or [`Error`].
//!
//! ## Architecture
//!
//! ```text
//! Rust SDK     Python SDK     CLI     MCP Server
//!      │            │          │           │
//!      └────────────┴──────────┴───────────┘
//!                        │
//!           ┌────────────┴────────────┐
//!           │     Command (enum)      │  ← Typed, serializable
//!           │     106 variants        │
//!           └────────────┬────────────┘
//!                        │
//!           ┌────────────┴────────────┐
//!           │       Executor          │  ← Stateless dispatch
//!           │   execute(cmd) -> Result│
//!           └────────────┬────────────┘
//!                        │
//!           ┌────────────┴────────────┐
//!           │     Output (enum)       │  ← Typed results
//!           │     ~40 variants        │
//!           └─────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```ignore
//! use strata_executor::{Command, Output, Executor, RunId};
//! use strata_core::Value;
//!
//! let executor = Executor::new(substrate);
//!
//! // Execute a KV put command
//! let cmd = Command::KvPut {
//!     run: RunId::default(),
//!     key: "user:123".into(),
//!     value: Value::String("Alice".into()),
//! };
//!
//! match executor.execute(cmd)? {
//!     Output::Version(v) => println!("Stored at version {}", v),
//!     _ => unreachable!(),
//! }
//! ```
//!
//! ## Design Principles
//!
//! 1. **Commands are complete** - Every primitive operation has a Command variant
//! 2. **Commands are self-contained** - All context is in the command, no implicit state
//! 3. **Executor is stateless** - Pure dispatch to primitives
//! 4. **Serialization is lossless** - JSON round-trip preserves exact values
//! 5. **No executable code** - Commands are data, not closures

pub(crate) mod bridge;
mod command;
mod convert;
mod error;
mod executor;
pub(crate) mod json;
mod output;
mod session;
mod api;
mod types;

// Handler modules
mod handlers;

// Test modules
#[cfg(test)]
mod tests;

// Re-export public API
pub use command::Command;
pub use error::Error;
pub use executor::Executor;
pub use output::Output;
pub use session::Session;
pub use api::Strata;
pub use types::*;

/// Result type for executor operations
pub type Result<T> = std::result::Result<T, Error>;
