//! Error conversion from internal error types.
//!
//! This module provides conversions from internal Strata errors to
//! the executor's [`Error`] type.

use crate::Error;
use strata_core::StrataError;

/// Convert a StrataError to an executor Error.
///
/// This preserves all error details while mapping to the appropriate
/// executor error variant.
impl From<StrataError> for Error {
    fn from(err: StrataError) -> Self {
        match err {
            // Not Found errors
            StrataError::NotFound { entity_ref } => {
                // Parse entity ref to determine the right error type
                let entity_str = entity_ref.to_string();
                if entity_str.starts_with("kv:") || entity_str.starts_with("json:") {
                    Error::KeyNotFound { key: entity_str }
                } else if entity_str.starts_with("run:") {
                    Error::BranchNotFound { branch: entity_str }
                } else if entity_str.starts_with("collection:")
                    || entity_str.starts_with("vector:")
                {
                    Error::CollectionNotFound {
                        collection: entity_str,
                    }
                } else if entity_str.starts_with("stream:") || entity_str.starts_with("event:") {
                    Error::StreamNotFound { stream: entity_str }
                } else if entity_str.starts_with("state:") || entity_str.starts_with("cell:") {
                    Error::CellNotFound { cell: entity_str }
                } else {
                    // Generic not found
                    Error::KeyNotFound { key: entity_str }
                }
            }

            StrataError::BranchNotFound { branch_id } => Error::BranchNotFound {
                branch: branch_id.to_string(),
            },

            // Type errors
            StrataError::WrongType { expected, actual } => Error::WrongType { expected, actual },

            // Conflict errors (temporal failures)
            StrataError::Conflict { reason, .. } => Error::Conflict { reason },

            StrataError::VersionConflict {
                expected, actual, ..
            } => {
                let expected_num = version_to_u64(&expected);
                let actual_num = version_to_u64(&actual);
                Error::VersionConflict {
                    expected: expected_num,
                    actual: actual_num,
                }
            }

            StrataError::WriteConflict { entity_ref, .. } => Error::Conflict {
                reason: format!("Write conflict on {}", entity_ref),
            },

            StrataError::TransactionAborted { reason } => Error::Conflict {
                reason: format!("Transaction aborted: {}", reason),
            },

            StrataError::TransactionTimeout { duration_ms } => Error::Conflict {
                reason: format!("Transaction timeout after {}ms", duration_ms),
            },

            StrataError::TransactionNotActive { .. } => Error::TransactionNotActive,

            // Validation errors
            StrataError::InvalidOperation { entity_ref, reason } => Error::ConstraintViolation {
                reason: format!("Invalid operation on {}: {}", entity_ref, reason),
            },

            StrataError::InvalidInput { message } => Error::InvalidInput { reason: message },

            // Constraint errors
            StrataError::DimensionMismatch { expected, got } => Error::DimensionMismatch {
                expected,
                actual: got,
            },

            StrataError::CapacityExceeded {
                resource,
                limit,
                requested,
            } => Error::ConstraintViolation {
                reason: format!(
                    "Capacity exceeded for {}: limit {}, requested {}",
                    resource, limit, requested
                ),
            },

            StrataError::BudgetExceeded { operation } => Error::ConstraintViolation {
                reason: format!("Budget exceeded for operation: {}", operation),
            },

            StrataError::PathNotFound { entity_ref, path } => Error::InvalidPath {
                reason: format!("Path '{}' not found in {}", path, entity_ref),
            },

            // History errors
            StrataError::HistoryTrimmed {
                requested,
                earliest_retained,
                ..
            } => Error::HistoryTrimmed {
                requested: version_to_u64(&requested),
                earliest: version_to_u64(&earliest_retained),
            },

            // System errors
            StrataError::Storage { message, .. } => Error::Io { reason: message },

            StrataError::Serialization { message } => Error::Serialization { reason: message },

            StrataError::Corruption { message } => Error::Io {
                reason: format!("Data corruption: {}", message),
            },

            StrataError::Internal { message } => Error::Internal { reason: message },
        }
    }
}

/// Convert a strata_core::StrataResult to an executor Result.
pub fn convert_result<T>(result: strata_core::StrataResult<T>) -> crate::Result<T> {
    result.map_err(Error::from)
}

/// Extract a u64 from a Version enum.
fn version_to_u64(version: &strata_core::Version) -> u64 {
    match version {
        strata_core::Version::Txn(n) => *n,
        strata_core::Version::Sequence(n) => *n,
        strata_core::Version::Counter(n) => *n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{EntityRef, Version};

    #[test]
    fn test_not_found_kv() {
        let err = StrataError::not_found(EntityRef::kv(
            strata_core::types::BranchId::from_bytes([0; 16]),
            "mykey",
        ));
        let converted: Error = err.into();
        match converted {
            Error::KeyNotFound { key } => assert!(key.contains("mykey")),
            _ => panic!("Expected KeyNotFound"),
        }
    }

    #[test]
    fn test_version_conflict() {
        let err = StrataError::version_conflict(
            EntityRef::kv(strata_core::types::BranchId::from_bytes([0; 16]), "key"),
            Version::Txn(5),
            Version::Txn(6),
        );
        let converted: Error = err.into();
        match converted {
            Error::VersionConflict { expected, actual } => {
                assert_eq!(expected, 5);
                assert_eq!(actual, 6);
            }
            _ => panic!("Expected VersionConflict"),
        }
    }

    #[test]
    fn test_wrong_type() {
        let err = StrataError::wrong_type("Int", "String");
        let converted: Error = err.into();
        match converted {
            Error::WrongType { expected, actual } => {
                assert_eq!(expected, "Int");
                assert_eq!(actual, "String");
            }
            _ => panic!("Expected WrongType"),
        }
    }

    #[test]
    fn test_internal_error() {
        let err = StrataError::internal("something went wrong");
        let converted: Error = err.into();
        match converted {
            Error::Internal { reason } => assert!(reason.contains("something went wrong")),
            _ => panic!("Expected Internal"),
        }
    }

    #[test]
    fn test_dimension_mismatch() {
        let err = StrataError::dimension_mismatch(384, 768);
        let converted: Error = err.into();
        match converted {
            Error::DimensionMismatch { expected, actual } => {
                assert_eq!(expected, 384);
                assert_eq!(actual, 768);
            }
            _ => panic!("Expected DimensionMismatch"),
        }
    }
}
