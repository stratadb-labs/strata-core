//! StrataError Conformance Tests (Epic 63: Error Standardization)
//!
//! This module verifies that StrataError correctly unifies error handling:
//!
//! - All error variants are properly defined
//! - Error constructors work correctly
//! - Error classification methods (is_retryable, is_serious)
//! - EntityRef extraction from errors
//! - Error conversions from primitive errors
//! - Error Display and Debug implementations
//!
//! # Story #479: StrataError Enum Definition
//! # Story #480: Error Conversion from Primitive Errors
//! # Story #481: EntityRef in Error Messages
//! # Story #482: Error Documentation and Guidelines

use crate::test_utils::test_run_id;
use strata_core::contract::{EntityRef, Version};
use strata_core::error::StrataError;
use strata_core::types::JsonDocId;

// ============================================================================
// Error Variant Coverage
// ============================================================================

#[test]
fn strata_error_not_found_variant() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "missing-key");

    let error = StrataError::not_found(entity_ref.clone());

    // Verify it's a NotFound variant
    assert!(matches!(&error, StrataError::NotFound { .. }));

    // EntityRef can be extracted
    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_run_not_found_variant() {
    let run_id = test_run_id();

    let error = StrataError::run_not_found(run_id);

    assert!(matches!(&error, StrataError::RunNotFound { .. }));
    assert_eq!(error.run_id(), Some(run_id));
}

#[test]
fn strata_error_version_conflict_variant() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id, "counter");
    let expected = Version::counter(5);
    let actual = Version::counter(6);

    let error = StrataError::version_conflict(entity_ref.clone(), expected.clone(), actual.clone());

    assert!(matches!(&error, StrataError::VersionConflict { .. }));
    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_write_conflict_variant() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "shared-key");

    let error = StrataError::write_conflict(entity_ref.clone());

    assert!(matches!(&error, StrataError::WriteConflict { .. }));
    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_transaction_aborted_variant() {
    let error = StrataError::transaction_aborted("Conflict detected");

    assert!(matches!(&error, StrataError::TransactionAborted { .. }));
}

#[test]
fn strata_error_transaction_timeout_variant() {
    let error = StrataError::transaction_timeout(5000);

    assert!(matches!(&error, StrataError::TransactionTimeout { .. }));
}

#[test]
fn strata_error_transaction_not_active_variant() {
    let error = StrataError::transaction_not_active("committed");

    assert!(matches!(&error, StrataError::TransactionNotActive { .. }));
}

#[test]
fn strata_error_invalid_operation_variant() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();
    let entity_ref = EntityRef::json(run_id, doc_id);

    let error = StrataError::invalid_operation(entity_ref.clone(), "Document already exists");

    assert!(matches!(&error, StrataError::InvalidOperation { .. }));
    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_invalid_input_variant() {
    let error = StrataError::invalid_input("Key cannot be empty");

    assert!(matches!(&error, StrataError::InvalidInput { .. }));
}

#[test]
fn strata_error_dimension_mismatch_variant() {
    let error = StrataError::dimension_mismatch(128, 256);

    assert!(matches!(&error, StrataError::DimensionMismatch { .. }));
}

#[test]
fn strata_error_path_not_found_variant() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();
    let entity_ref = EntityRef::json(run_id, doc_id);

    let error = StrataError::path_not_found(entity_ref.clone(), "$.missing.path");

    assert!(matches!(&error, StrataError::PathNotFound { .. }));
    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_storage_variant() {
    let error = StrataError::storage("Disk full");

    assert!(matches!(&error, StrataError::Storage { .. }));
}

#[test]
fn strata_error_serialization_variant() {
    let error = StrataError::serialization("Invalid JSON");

    assert!(matches!(&error, StrataError::Serialization { .. }));
}

#[test]
fn strata_error_corruption_variant() {
    let error = StrataError::corruption("Checksum mismatch");

    assert!(matches!(&error, StrataError::Corruption { .. }));
}

#[test]
fn strata_error_capacity_exceeded_variant() {
    let error = StrataError::capacity_exceeded("events", 1_000_000, 1_000_001);

    assert!(matches!(&error, StrataError::CapacityExceeded { .. }));
}

#[test]
fn strata_error_budget_exceeded_variant() {
    let error = StrataError::budget_exceeded("Transaction exceeded time budget");

    assert!(matches!(&error, StrataError::BudgetExceeded { .. }));
}

#[test]
fn strata_error_internal_variant() {
    let error = StrataError::internal("Unexpected state");

    assert!(matches!(&error, StrataError::Internal { .. }));
}

// ============================================================================
// Error Classification (is_retryable, is_serious)
// ============================================================================

#[test]
fn strata_error_is_retryable_version_conflict() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id, "counter");
    let error = StrataError::version_conflict(entity_ref, Version::counter(1), Version::counter(2));

    assert!(error.is_retryable());
    assert!(!error.is_serious());
}

#[test]
fn strata_error_is_retryable_write_conflict() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");
    let error = StrataError::write_conflict(entity_ref);

    assert!(error.is_retryable());
    assert!(!error.is_serious());
}

#[test]
fn strata_error_is_retryable_transaction_aborted() {
    let error = StrataError::transaction_aborted("Conflict");

    assert!(error.is_retryable());
    assert!(!error.is_serious());
}

#[test]
fn strata_error_not_retryable_not_found() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "missing");
    let error = StrataError::not_found(entity_ref);

    assert!(!error.is_retryable());
    assert!(!error.is_serious());
}

#[test]
fn strata_error_is_serious_corruption() {
    let error = StrataError::corruption("Data corrupted");

    assert!(error.is_serious());
    assert!(!error.is_retryable());
}

#[test]
fn strata_error_is_serious_internal() {
    let error = StrataError::internal("Unexpected state");

    assert!(error.is_serious());
    assert!(!error.is_retryable());
}

// ============================================================================
// EntityRef Extraction
// ============================================================================

#[test]
fn strata_error_entity_ref_extraction_from_not_found() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");
    let error = StrataError::not_found(entity_ref.clone());

    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_entity_ref_extraction_from_version_conflict() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id, "cell");
    let error = StrataError::version_conflict(entity_ref.clone(), Version::counter(1), Version::counter(2));

    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_entity_ref_extraction_from_write_conflict() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");
    let error = StrataError::write_conflict(entity_ref.clone());

    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_entity_ref_extraction_from_invalid_operation() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::event(run_id, 1);
    let error = StrataError::invalid_operation(entity_ref.clone(), "reason");

    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_entity_ref_extraction_from_path_not_found() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();
    let entity_ref = EntityRef::json(run_id, doc_id);
    let error = StrataError::path_not_found(entity_ref.clone(), "$.path");

    assert_eq!(error.entity_ref(), Some(&entity_ref));
}

#[test]
fn strata_error_entity_ref_none_for_errors_without_entity() {
    let error = StrataError::internal("No entity");
    assert_eq!(error.entity_ref(), None);

    let error = StrataError::storage("No entity");
    assert_eq!(error.entity_ref(), None);

    let error = StrataError::serialization("No entity");
    assert_eq!(error.entity_ref(), None);

    let error = StrataError::transaction_aborted("No entity");
    assert_eq!(error.entity_ref(), None);
}

// ============================================================================
// RunId Extraction
// ============================================================================

#[test]
fn strata_error_run_id_extraction_from_run_not_found() {
    let run_id = test_run_id();
    let error = StrataError::run_not_found(run_id);

    assert_eq!(error.run_id(), Some(run_id));
}

#[test]
fn strata_error_run_id_none_for_other_errors() {
    let error = StrataError::internal("No run");
    assert_eq!(error.run_id(), None);

    let error = StrataError::storage("No run");
    assert_eq!(error.run_id(), None);
}

// ============================================================================
// Display Implementation
// ============================================================================

#[test]
fn strata_error_display_not_found() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "my-key");
    let error = StrataError::not_found(entity_ref);

    let display = format!("{}", error);
    assert!(display.contains("not found"));
}

#[test]
fn strata_error_display_version_conflict() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id, "counter");
    let error = StrataError::version_conflict(
        entity_ref,
        Version::counter(5),
        Version::counter(6),
    );

    let display = format!("{}", error);
    assert!(display.contains("version conflict"));
}

#[test]
fn strata_error_display_is_informative() {
    let error = StrataError::transaction_timeout(5000);
    let display = format!("{}", error);

    // Should include the timeout duration
    assert!(display.contains("5000"));
}

// ============================================================================
// Debug Implementation
// ============================================================================

#[test]
fn strata_error_debug_is_implemented() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");
    let error = StrataError::not_found(entity_ref);

    let debug = format!("{:?}", error);
    assert!(!debug.is_empty());
    assert!(debug.contains("NotFound"));
}

// ============================================================================
// Error Conversion Tests
// ============================================================================

#[test]
fn strata_error_from_io_error() {
    use std::io::{Error as IoError, ErrorKind};

    let io_error = IoError::new(ErrorKind::NotFound, "File not found");
    let strata_error: StrataError = io_error.into();

    // Should convert to a Storage error
    assert!(matches!(strata_error, StrataError::Storage { .. }));
}

#[test]
fn strata_error_from_serde_json_error() {
    // Create a JSON parsing error
    let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json{");
    let json_error = json_result.unwrap_err();

    let strata_error: StrataError = json_error.into();

    // Should convert to a Serialization error
    assert!(matches!(strata_error, StrataError::Serialization { .. }));
}

// ============================================================================
// Error Chaining
// ============================================================================

#[test]
fn strata_error_storage_with_source() {
    use std::io::{Error as IoError, ErrorKind};

    let io_error = IoError::new(ErrorKind::PermissionDenied, "Access denied");
    let error = StrataError::storage_with_source("Failed to write", Box::new(io_error));

    assert!(matches!(&error, StrataError::Storage { .. }));
    let display = format!("{}", error);
    assert!(display.contains("Failed to write"));
}

// ============================================================================
// Error Equality and Matching
// ============================================================================

#[test]
fn strata_error_same_type_different_content() {
    let run_id = test_run_id();
    let entity1 = EntityRef::kv(run_id, "key1");
    let entity2 = EntityRef::kv(run_id, "key2");

    let error1 = StrataError::not_found(entity1);
    let error2 = StrataError::not_found(entity2);

    // Both are NotFound but with different entities
    assert!(matches!(error1, StrataError::NotFound { .. }));
    assert!(matches!(error2, StrataError::NotFound { .. }));
}

// ============================================================================
// All Entity Types Have Errors
// ============================================================================

#[test]
fn strata_error_all_entity_types_in_not_found() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();

    // Can create NotFound for all entity types
    let _ = StrataError::not_found(EntityRef::kv(run_id, "key"));
    let _ = StrataError::not_found(EntityRef::event(run_id, 1));
    let _ = StrataError::not_found(EntityRef::state(run_id, "cell"));
    let _ = StrataError::not_found(EntityRef::trace(run_id, "trace-1"));
    let _ = StrataError::not_found(EntityRef::json(run_id, doc_id));
    let _ = StrataError::not_found(EntityRef::vector(run_id, "col", "vec"));
    let _ = StrataError::not_found(EntityRef::run(run_id));
}

#[test]
fn strata_error_all_entity_types_in_invalid_operation() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();

    // Can create InvalidOperation for all entity types
    let _ = StrataError::invalid_operation(EntityRef::kv(run_id, "key"), "reason");
    let _ = StrataError::invalid_operation(EntityRef::event(run_id, 1), "reason");
    let _ = StrataError::invalid_operation(EntityRef::state(run_id, "cell"), "reason");
    let _ = StrataError::invalid_operation(EntityRef::trace(run_id, "trace-1"), "reason");
    let _ = StrataError::invalid_operation(EntityRef::json(run_id, doc_id), "reason");
    let _ = StrataError::invalid_operation(EntityRef::vector(run_id, "col", "vec"), "reason");
    let _ = StrataError::invalid_operation(EntityRef::run(run_id), "reason");
}

// ============================================================================
// Additional Error Conversion Tests
// ============================================================================

#[test]
fn strata_error_from_vector_error() {
    use strata_primitives::VectorError;

    // Test dimension mismatch conversion
    let vector_error = VectorError::DimensionMismatch {
        expected: 128,
        got: 256,
    };
    let strata_error: StrataError = vector_error.into();
    assert!(matches!(strata_error, StrataError::DimensionMismatch { expected: 128, got: 256 }));

    // Test empty embedding conversion
    let vector_error = VectorError::EmptyEmbedding;
    let strata_error: StrataError = vector_error.into();
    assert!(matches!(strata_error, StrataError::InvalidInput { .. }));

    // Test invalid dimension conversion
    let vector_error = VectorError::InvalidDimension { dimension: 0 };
    let strata_error: StrataError = vector_error.into();
    assert!(matches!(strata_error, StrataError::InvalidInput { .. }));
}

// ============================================================================
// Error Classification Completeness
// ============================================================================

#[test]
fn strata_error_is_not_found_classification() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();

    // NotFound errors
    let error = StrataError::not_found(EntityRef::kv(run_id, "key"));
    assert!(error.is_not_found());
    assert!(!error.is_conflict());
    assert!(!error.is_transaction_error());
    assert!(!error.is_validation_error());

    let error = StrataError::run_not_found(run_id);
    assert!(error.is_not_found());

    let error = StrataError::path_not_found(EntityRef::json(run_id, doc_id), "$.missing");
    assert!(error.is_not_found());
}

#[test]
fn strata_error_is_conflict_classification() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");

    let error = StrataError::version_conflict(entity_ref.clone(), Version::counter(1), Version::counter(2));
    assert!(error.is_conflict());
    assert!(!error.is_not_found());

    let error = StrataError::write_conflict(entity_ref);
    assert!(error.is_conflict());
}

#[test]
fn strata_error_is_transaction_error_classification() {
    let error = StrataError::transaction_aborted("conflict");
    assert!(error.is_transaction_error());
    assert!(!error.is_conflict()); // Different from conflict errors

    let error = StrataError::transaction_timeout(5000);
    assert!(error.is_transaction_error());

    let error = StrataError::transaction_not_active("committed");
    assert!(error.is_transaction_error());
}

#[test]
fn strata_error_is_validation_error_classification() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");

    let error = StrataError::invalid_operation(entity_ref, "already exists");
    assert!(error.is_validation_error());
    assert!(!error.is_not_found());

    let error = StrataError::invalid_input("empty key");
    assert!(error.is_validation_error());

    let error = StrataError::dimension_mismatch(128, 256);
    assert!(error.is_validation_error());
}
