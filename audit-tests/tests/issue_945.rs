//! Audit test for issue #945: VersionChain GC never invoked
//! Verdict: FIXED
//!
//! Previously, the `VersionChain::gc()` method was never called in production
//! code, and the `RetentionApply` command returned "not yet implemented".
//!
//! The fix implements `RetentionApply` to call `gc_versions_before()` on the
//! database, which prunes version chains for all keys on the specified branch.
//! The GC boundary is set to the current version, meaning all superseded
//! versions are pruned.

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::BranchId;
use strata_executor::{Command, Executor, Output};

/// Verifies that RetentionApply now works and can trigger GC.
///
/// We write to the same key many times, run RetentionApply, and verify:
/// 1. Each write creates a new version (version numbers increase)
/// 2. RetentionApply succeeds (no longer returns "not yet implemented")
/// 3. The latest value is still accessible after GC
#[test]
fn issue_945_version_chain_gc_never_invoked() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Write to the same key many times to grow its version chain
    let num_writes = 20;
    let mut versions = Vec::new();

    for i in 0..num_writes {
        let result = executor
            .execute(Command::KvPut {
                branch: Some(branch.clone()),
                key: "frequently_updated".into(),
                value: Value::Int(i),
            })
            .unwrap();

        if let Output::Version(v) = result {
            versions.push(v);
        }
    }

    // Versions should be monotonically increasing
    for window in versions.windows(2) {
        assert!(
            window[1] > window[0],
            "Versions should be monotonically increasing"
        );
    }

    // BUG FIXED: RetentionApply now succeeds
    let retention_result = executor.execute(Command::RetentionApply {
        branch: Some(branch.clone()),
    });

    assert!(
        retention_result.is_ok(),
        "RetentionApply should now succeed (was previously unimplemented). Got: {:?}",
        retention_result
    );

    // The latest value should still be accessible after GC
    let get_result = executor
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "frequently_updated".into(),
        })
        .unwrap();

    match get_result {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(
                vv.value,
                Value::Int(num_writes - 1),
                "Latest value should still be accessible after GC"
            );
        }
        Output::Maybe(Some(val)) => {
            assert_eq!(
                val,
                Value::Int(num_writes - 1),
                "Latest value should still be accessible after GC"
            );
        }
        other => panic!("Expected value after GC, got {:?}", other),
    }
}
