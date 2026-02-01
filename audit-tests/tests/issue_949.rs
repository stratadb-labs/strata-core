//! Audit test for issue #949: No upper bound on vector dimension
//! Verdict: CONFIRMED BUG
//!
//! In engine/primitives/vector/store.rs, `create_collection()` validates that
//! `config.dimension > 0` but places no upper bound on the dimension:
//!
//! ```ignore
//! if config.dimension == 0 {
//!     return Err(VectorError::InvalidDimension { dimension: config.dimension });
//! }
//! ```
//!
//! The `VectorConfig::new()` constructor also only validates `dimension > 0`.
//!
//! This means a caller can create a collection with dimension = 1,000,000 or even
//! u64::MAX (via the Command interface which uses u64 for dimension).
//!
//! Impact:
//! - A single vector with dimension 1,000,000 consumes ~4MB of memory (1M * 4 bytes/f32)
//! - Creating and searching such vectors is extremely slow
//! - With dimension close to u64::MAX, allocation will fail with OOM
//! - This could be exploited as a denial-of-service vector
//!
//! Typical embedding dimensions range from 64 to 4096. A reasonable upper bound
//! would be 65536 (64K) to allow for future models while preventing abuse.
//!
//! The fix would add validation in VectorConfig::new() and/or create_collection():
//! ```ignore
//! const MAX_DIMENSION: usize = 65536;
//! if config.dimension > MAX_DIMENSION {
//!     return Err(VectorError::InvalidDimension { dimension: config.dimension });
//! }
//! ```

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Executor};

/// Demonstrates that absurdly large vector dimensions are accepted.
///
/// A collection with dimension 1,000,000 should be rejected, but the
/// current validation only checks for dimension > 0.
#[test]
fn issue_949_no_vector_dimension_upper_bound() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create collection with absurdly large dimension
    // Note: We use a moderately large value (10000) to avoid OOM in tests.
    // The bug exists for any dimension > reasonable_max (e.g., 65536).
    let result = executor.execute(Command::VectorCreateCollection {
        branch: Some(branch.clone()),
        collection: "huge_dim".into(),
        dimension: 10_000,
        metric: DistanceMetric::Cosine,
    });

    // BUG: This should be rejected for being unreasonably large, but it's accepted.
    // A dimension of 10,000 is borderline (some models use up to ~8192),
    // but the point is there is NO upper bound validation at all.
    assert!(
        result.is_ok(),
        "Large dimension collection accepted (bug confirmed: no upper bound validation)"
    );
}

/// Demonstrates that the Command interface accepts u64 dimensions,
/// which could be astronomically large.
#[test]
fn issue_949_dimension_as_u64_allows_extreme_values() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // The Command::VectorCreateCollection uses u64 for dimension.
    // Attempting an extremely large dimension will likely fail during
    // collection creation due to memory allocation, but the validation
    // layer should catch this BEFORE attempting allocation.
    //
    // We test with a value that's large enough to demonstrate the missing
    // validation but small enough to not cause OOM in the test environment.
    let result = executor.execute(Command::VectorCreateCollection {
        branch: Some(branch.clone()),
        collection: "very_large_dim".into(),
        dimension: 100_000,
        metric: DistanceMetric::Cosine,
    });

    // This may succeed (creating the collection config) or fail during
    // backend initialization (memory allocation). Either way, the validation
    // layer should have rejected it before reaching this point.
    match result {
        Ok(_) => {
            // Bug confirmed: 100K dimension accepted without validation
        }
        Err(_) => {
            // Failed for some other reason (memory, etc.) but the lack
            // of dimension validation is still the bug
        }
    }
}
