//! Tier 8: WAL Entry Format Tests

use crate::test_utils::*;
use strata_durability::WalEntryType;

#[test]
fn test_wal_entry_types() {
    verify_wal_entry_types();
}

#[test]
fn test_vector_operations_use_correct_wal_types() {
    // Verify entry type codes are as specified in contract
    assert_eq!(WalEntryType::VectorCollectionCreate as u8, 0x70);
    assert_eq!(WalEntryType::VectorCollectionDelete as u8, 0x71);
    assert_eq!(WalEntryType::VectorUpsert as u8, 0x72);
    assert_eq!(WalEntryType::VectorDelete as u8, 0x73);
}

#[test]
fn test_wal_entry_serialization() {
    use strata_primitives::vector::{
        create_wal_collection_create, create_wal_upsert, create_wal_delete,
        WalVectorCollectionCreate, WalVectorUpsert, WalVectorDelete,
        VectorConfig, DistanceMetric,
    };
    use strata_core::RunId;

    let run_id = RunId::new();

    // Test collection create roundtrip
    let config = VectorConfig {
        dimension: 384,
        metric: DistanceMetric::Cosine,
        storage_dtype: strata_primitives::vector::StorageDtype::F32,
    };
    let create_payload = create_wal_collection_create(run_id, "test", &config);
    let bytes = create_payload.to_bytes().unwrap();
    let parsed = WalVectorCollectionCreate::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.collection, "test");
    assert_eq!(parsed.config.dimension, 384);

    // Test upsert roundtrip
    let embedding = vec![0.1, 0.2, 0.3, 0.4];
    let upsert_payload = create_wal_upsert(
        run_id,
        "test",
        "key1",
        strata_primitives::vector::VectorId::new(1),
        &embedding,
        None,
    );
    let bytes = upsert_payload.to_bytes().unwrap();
    let parsed = WalVectorUpsert::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.key, "key1");
    assert_eq!(parsed.embedding, embedding);
}
