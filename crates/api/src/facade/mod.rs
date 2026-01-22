//! Facade API - Redis-like convenience surface
//!
//! The Facade API provides simplified access to Strata with familiar patterns:
//! - Implicit default run targeting
//! - Auto-commit for each operation
//! - Simple return types (no version wrapping by default)
//!
//! ## Design Philosophy
//!
//! The Facade is syntactic sugar over the Substrate. Every facade call
//! desugars to exactly one substrate call pattern.
//!
//! ## Auto-Commit Mode
//!
//! By default, every operation auto-commits immediately.
//! For batching, use `batch()` to create a batch context.
//!
//! ## Module Structure
//!
//! - `types`: Facade configuration types
//! - `kv`: Key-value convenience methods
//! - `json`: JSON document convenience methods
//! - `event`: Event log convenience methods
//! - `state`: State cell convenience methods
//! - `vector`: Vector store convenience methods
//! - `trace`: Trace store convenience methods
//!
//! ## Desugaring Examples
//!
//! | Facade Call | Substrate Equivalent |
//! |-------------|---------------------|
//! | `get(key)` | `kv_get(default, key).map(\|v\| v.value)` |
//! | `set(key, value)` | `kv_put(default, key, value)` |
//! | `incr(key)` | `kv_incr(default, key, 1)` |

pub mod types;
pub mod kv;
pub mod json;
pub mod event;
pub mod state;
pub mod vector;
pub mod trace;

// Re-export facade types
pub use types::{FacadeConfig, GetOptions, SetOptions};

// Re-export facade traits
pub use kv::{KVFacade, KVFacadeBatch};
pub use json::JsonFacade;
pub use event::EventFacade;
pub use state::StateFacade;
pub use vector::VectorFacade;
pub use trace::TraceFacade;
