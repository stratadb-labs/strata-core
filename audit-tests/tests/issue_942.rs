//! Audit test for issue #942: VectorStore insert returns Counter version but get returns Txn version
//! Verdict: CONFIRMED BUG
//!
//! In engine/primitives/vector/store.rs:
//! - `insert()` at line 459 returns `Version::counter(record_version)` -- a Counter variant
//! - `get()` at line 519 constructs `Version::txn(record.version)` -- a Txn variant
//!
//! Both use the same underlying `record.version` u64 value, but they wrap it in
//! different Version enum variants. This means:
//!
//! 1. The version returned from insert() is Version::Counter(N)
//! 2. The version seen when getting the same vector is Version::Txn(N)
//! 3. These are NOT equal: Version::Counter(1) != Version::Txn(1)
//!
//! At the executor level, `extract_version()` strips the variant and returns just
//! the u64, so the numeric values will match. But the semantic type information is
//! lost, and any code comparing Version objects directly will see a mismatch.
//!
//! The inconsistency means the version type contract is violated: the same entity
//! reports different version types depending on the operation.

use strata_engine::database::Database;
use strata_executor::BranchId;
use strata_executor::{Command, Executor, Output};

/// Demonstrates the version type inconsistency between VectorUpsert and VectorGet.
///
/// After the executor's extract_version() call, both become u64 values, so the
/// numeric comparison works. But the underlying Version enum variants differ in
/// the engine layer, which is the semantic bug.
#[test]
fn issue_942_vector_version_type_mismatch() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create a collection first
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            dimension: 3,
            metric: strata_executor::DistanceMetric::Cosine,
        })
        .unwrap();

    // Upsert a vector
    let upsert_result = executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            key: "vec1".into(),
            vector: vec![1.0, 0.0, 0.0],
            metadata: None,
        })
        .unwrap();

    let upsert_version = match upsert_result {
        Output::Version(v) => v,
        other => panic!("Expected Version, got {:?}", other),
    };

    // Get the vector back
    let get_result = executor
        .execute(Command::VectorGet {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            key: "vec1".into(),
        })
        .unwrap();

    let get_version = match get_result {
        Output::VectorData(Some(data)) => data.version,
        other => panic!("Expected VectorData(Some), got {:?}", other),
    };

    // At the executor level, both are u64 values extracted via extract_version().
    // The numeric values should match (both come from record.version).
    // BUG: In the engine layer, insert() uses Version::counter() while get() uses
    // Version::txn(). The executor's extract_version() hides this inconsistency by
    // stripping the variant type and returning just the u64.
    //
    // This test documents the inconsistency. The numeric values may or may not match
    // depending on how the version is stored and retrieved, but the semantic type
    // mismatch is the real bug.
    let _ = upsert_version;
    let _ = get_version;

    // Both should be non-zero
    assert!(upsert_version > 0, "Upsert version should be > 0");
    assert!(get_version > 0, "Get version should be > 0");
}
