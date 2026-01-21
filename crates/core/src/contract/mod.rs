//! Contract types for API stability
//!
//! This module contains the core types that define the database's public contract.
//! These types express the Seven Invariants that all primitives must follow:
//!
//! 1. **Addressable**: Every entity has a stable identity via `EntityRef`
//! 2. **Versioned**: Every read returns `Versioned<T>`, every write returns `Version`
//! 3. **Transactional**: Every primitive participates in transactions
//! 4. **Lifecycle**: Every primitive follows create/exist/evolve/destroy
//! 5. **Run-scoped**: Every entity belongs to exactly one run
//! 6. **Introspectable**: Every primitive has `exists()` or equivalent
//! 7. **Read/Write**: Reads never modify state, writes always produce versions
//!
//! ## Module Structure
//!
//! - `entity_ref`: Universal entity addressing (Invariant 1)
//! - `versioned`: Generic versioned wrapper (Invariant 2)
//! - `version`: Version identifier types (Invariant 2)
//! - `timestamp`: Microsecond timestamps (Invariant 2)
//! - `primitive_type`: Primitive enumeration (Invariant 6)
//! - `run_name`: Semantic run identifier (Invariant 5)
//!
//! ## Usage
//!
//! ```
//! use strata_core::contract::{
//!     EntityRef, Versioned, Version, Timestamp, PrimitiveType, RunName
//! };
//! ```

pub mod entity_ref;
pub mod primitive_type;
pub mod run_name;
pub mod timestamp;
pub mod version;
pub mod versioned;

// Re-exports
pub use entity_ref::{DocRef, EntityRef};
pub use primitive_type::PrimitiveType;
pub use run_name::{RunName, RunNameError, MAX_RUN_NAME_LENGTH};
pub use timestamp::Timestamp;
pub use version::Version;
pub use versioned::{Versioned, VersionedValue};
