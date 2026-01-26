//! Command handlers organized by primitive category.
//!
//! Each submodule handles commands for a specific primitive:
//!
//! | Module | Commands | Primitive |
//! |--------|----------|-----------|
//! | `kv` | 15 | KVStore, KVStoreBatch |
//! | `json` | 17 | JsonStore |
//! | `event` | 11 | EventLog |
//! | `state` | 8 | StateCell |
//! | `vector` | 19 | VectorStore |
//! | `run` | 24 | RunIndex |
//! | `transaction` | 5 | TransactionControl |
//! | `retention` | 3 | RetentionSubstrate |
//! | `database` | 4 | Database-level |

pub mod kv;
pub mod json;
pub mod event;
pub mod state;
pub mod vector;
pub mod run;

// Transaction commands are deferred because the Executor is stateless by design.
// Transactions require session state management which would need additional design work.
//
// Retention commands (RetentionApply, RetentionStats, RetentionPreview) are deferred
// as they require additional infrastructure for garbage collection statistics.
//
// Database commands (Ping, Info, Flush, Compact) are implemented directly in executor.rs.
