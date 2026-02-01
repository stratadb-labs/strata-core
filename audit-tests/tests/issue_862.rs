//! Audit test for issue #862: Value::Null ambiguous with tombstones after serde round-trip
//! Verdict: CONFIRMED BUG
//!
//! StoredValue::is_tombstone is an in-memory-only flag that is lost when converting
//! to VersionedValue (via into_versioned()). After a serde round-trip, tombstones
//! become indistinguishable from Value::Null.

use strata_core::value::Value;
use strata_core::Version;
use strata_storage::stored_value::StoredValue;

#[test]
fn issue_862_tombstone_flag_lost_on_versioned_conversion() {
    // Create a tombstone (Value::Null with is_tombstone=true)
    let tombstone = StoredValue::tombstone(Version::txn(5));
    assert!(
        tombstone.is_tombstone(),
        "StoredValue should be a tombstone"
    );
    assert_eq!(
        *tombstone.value(),
        Value::Null,
        "Tombstone value should be Null"
    );

    // Create a legitimate Null value (is_tombstone=false)
    let null_value = StoredValue::new(Value::Null, Version::txn(5), None);
    assert!(
        !null_value.is_tombstone(),
        "Regular null should not be a tombstone"
    );
    assert_eq!(
        *null_value.value(),
        Value::Null,
        "Null value should be Null"
    );

    // Convert both to VersionedValue (as happens during bundle export)
    let tombstone_vv = tombstone.into_versioned();
    let null_vv = null_value.into_versioned();

    // BUG: After conversion, they are indistinguishable
    // Both have Value::Null and the same version, with no way to know which was a tombstone
    assert_eq!(
        tombstone_vv.value, null_vv.value,
        "BUG: tombstone and null are indistinguishable after into_versioned()"
    );
    assert_eq!(
        tombstone_vv.version, null_vv.version,
        "Versions are the same"
    );
}

#[test]
fn issue_862_tombstone_lost_after_serde_roundtrip() {
    // Create a tombstone
    let tombstone = StoredValue::tombstone(Version::txn(10));
    assert!(tombstone.is_tombstone());

    // Convert to VersionedValue (simulating bundle export path)
    let vv = tombstone.into_versioned();

    // Serialize to JSON (simulating export)
    let json = serde_json::to_string(&vv).unwrap();

    // Deserialize from JSON (simulating import)
    let restored_vv: strata_core::VersionedValue = serde_json::from_str(&json).unwrap();

    // Reconstruct StoredValue from VersionedValue (as import would do)
    let restored_sv = StoredValue::from_versioned(restored_vv);

    // BUG: The tombstone flag is lost - restored value is NOT a tombstone
    assert!(
        !restored_sv.is_tombstone(),
        "BUG: tombstone flag is lost after serde round-trip via VersionedValue"
    );
    // This means a deleted key could be resurrected as Value::Null after bundle import
}

#[test]
fn issue_862_stored_value_not_serializable() {
    // StoredValue does not derive Serialize/Deserialize,
    // so the is_tombstone flag cannot survive direct serialization either.
    // This test confirms StoredValue must go through VersionedValue for any
    // serialization, which loses the tombstone flag.

    let tombstone = StoredValue::tombstone(Version::txn(1));
    let vv = tombstone.clone().into_versioned();

    // The VersionedValue has no field for is_tombstone
    // Going back through from_versioned always sets is_tombstone=false
    let round_tripped = StoredValue::from_versioned(vv);
    assert!(
        !round_tripped.is_tombstone(),
        "BUG: from_versioned always creates non-tombstone StoredValue"
    );
}
