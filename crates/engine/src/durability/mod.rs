//! Durability modes for M4 performance optimization
//!
//! This module provides the durability abstraction layer that enables
//! trading off latency vs durability guarantees.
//!
//! # Durability Modes
//!
//! | Mode | WAL | fsync | Target Latency | Data Loss Window |
//! |------|-----|-------|----------------|------------------|
//! | InMemory | None | None | <3µs | All (on crash) |
//! | Buffered | Append | Periodic | <30µs | Bounded |
//! | Strict | Append | Every write | ~2ms | Zero |
//!
//! # Usage
//!
//! ```ignore
//! use in_mem_engine::Database;
//! use in_mem_durability::wal::DurabilityMode;
//!
//! // InMemory mode for fastest performance
//! let db = Database::builder()
//!     .in_memory()
//!     .open_temp()?;
//!
//! // Buffered mode for production (default)
//! let db = Database::builder()
//!     .buffered()
//!     .open()?;
//!
//! // Strict mode for maximum durability
//! let db = Database::builder()
//!     .strict()
//!     .open()?;
//! ```
//!
//! # Architecture
//!
//! The durability layer sits between transaction validation and storage apply:
//!
//! ```text
//! commit_transaction():
//!   ┌─────────────────┐
//!   │   Validate OCC  │
//!   └────────┬────────┘
//!            │
//!   ┌────────▼────────┐
//!   │ Allocate Version│
//!   └────────┬────────┘
//!            │
//!   ┌────────▼────────┐
//!   │  Durability::   │  ← Mode-specific
//!   │    persist()    │
//!   └────────┬────────┘
//!            │
//!   ┌────────▼────────┐
//!   │  Apply Storage  │
//!   └────────┬────────┘
//!            │
//!   ┌────────▼────────┐
//!   │ Mark Committed  │
//!   └─────────────────┘
//! ```

mod traits;
mod inmemory;
mod strict;

// Will be added in Story #224
// mod buffered;

pub use traits::{CommitData, Durability, DurabilityExt};
pub use inmemory::InMemoryDurability;
pub use strict::StrictDurability;

// Re-export DurabilityMode from durability crate for convenience
pub use in_mem_durability::wal::DurabilityMode;
