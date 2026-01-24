//! JsonStore Comprehensive Test Suite
//!
//! Tests organized by functionality:
//! - basic_ops: CRUD operations (set, get, delete, exists)
//! - path_ops: Path navigation and nested operations
//! - merge_ops: JSON merge patch (RFC 7396)
//! - tier1_ops: M11B Tier 1 features (list, cas, query)
//! - tier2_ops: M11B Tier 2 features (count, batch_get, batch_create)
//! - tier3_ops: M11B Tier 3 features (array_push, increment, array_pop)
//! - durability: Persistence across restarts
//! - concurrency: Thread safety
//! - edge_cases: Validation and boundary conditions

mod basic_ops;
mod path_ops;
mod merge_ops;
mod tier1_ops;
mod tier2_ops;
mod tier3_ops;
mod durability;
mod concurrency;
mod edge_cases;
