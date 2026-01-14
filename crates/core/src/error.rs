//! Error types for in-mem database
//!
//! This module defines all error types used throughout the system.
//! We use `thiserror` for automatic `Display` and `Error` trait implementations.

use crate::types::{Key, RunId};
use std::io;
use thiserror::Error;

/// Result type alias for in-mem operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for the in-mem database
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error (file operations, network, etc.)
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Key not found in storage
    #[error("Key not found: {0:?}")]
    KeyNotFound(Key),

    /// Version mismatch (for CAS operations)
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch {
        /// Expected version
        expected: u64,
        /// Actual version found
        actual: u64,
    },

    /// Data corruption detected
    #[error("Data corruption: {0}")]
    Corruption(String),

    /// Invalid operation or state
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Transaction aborted due to conflict (M2)
    #[error("Transaction aborted for run {0:?}")]
    TransactionAborted(RunId),

    /// Storage layer error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Invalid state transition (M2 transactions)
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Transaction conflict detected during commit (M2)
    #[error("Transaction conflict: {0}")]
    TransactionConflict(String),

    /// Transaction exceeded timeout (M2)
    #[error("Transaction timeout: {0}")]
    TransactionTimeout(String),
}

impl From<bincode::Error> for Error {
    fn from(e: bincode::Error) -> Self {
        Error::SerializationError(e.to_string())
    }
}

impl Error {
    /// Check if this error is a transaction conflict
    ///
    /// Used for retry logic - only conflict errors should be retried.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::TransactionConflict(_))
    }

    /// Check if this error is a transaction timeout
    ///
    /// Used to identify when a transaction was aborted due to exceeding
    /// its time limit.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Error::TransactionTimeout(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Namespace, RunId, TypeTag};

    #[test]
    fn test_error_display_io() {
        let err = Error::IoError(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        let msg = err.to_string();
        assert!(msg.contains("I/O error"));
    }

    #[test]
    fn test_error_display_serialization() {
        let err = Error::SerializationError("invalid format".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Serialization error"));
        assert!(msg.contains("invalid format"));
    }

    #[test]
    fn test_error_display_key_not_found() {
        let run_id = RunId::new();
        let namespace = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new(namespace, TypeTag::KV, b"test-key".to_vec());
        let err = Error::KeyNotFound(key);
        let msg = err.to_string();
        assert!(msg.contains("Key not found"));
    }

    #[test]
    fn test_error_display_version_mismatch() {
        let err = Error::VersionMismatch {
            expected: 42,
            actual: 43,
        };
        let msg = err.to_string();
        assert!(msg.contains("Version mismatch"));
        assert!(msg.contains("42"));
        assert!(msg.contains("43"));
    }

    #[test]
    fn test_error_display_corruption() {
        let err = Error::Corruption("CRC check failed".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Data corruption"));
        assert!(msg.contains("CRC check failed"));
    }

    #[test]
    fn test_error_display_invalid_operation() {
        let err = Error::InvalidOperation("cannot delete while locked".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid operation"));
        assert!(msg.contains("cannot delete while locked"));
    }

    #[test]
    fn test_error_display_transaction_aborted() {
        let run_id = RunId::new();
        let err = Error::TransactionAborted(run_id);
        let msg = err.to_string();
        assert!(msg.contains("Transaction aborted"));
    }

    #[test]
    fn test_error_display_storage() {
        let err = Error::StorageError("write failed".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Storage error"));
        assert!(msg.contains("write failed"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::IoError(_)));
    }

    #[test]
    fn test_error_from_bincode() {
        // Create a serialization error by using invalid bincode data
        let invalid_data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

        // Try to deserialize invalid data (will fail)
        let result: Result<String> = bincode::deserialize(&invalid_data).map_err(|e| e.into());

        assert!(matches!(result, Err(Error::SerializationError(_))));
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_result() -> Result<i32> {
            Ok(42)
        }

        fn returns_error() -> Result<i32> {
            Err(Error::InvalidOperation("test".to_string()))
        }

        assert_eq!(returns_result().unwrap(), 42);
        assert!(returns_error().is_err());
    }

    #[test]
    fn test_error_pattern_matching() {
        let err = Error::VersionMismatch {
            expected: 10,
            actual: 11,
        };

        match err {
            Error::VersionMismatch { expected, actual } => {
                assert_eq!(expected, 10);
                assert_eq!(actual, 11);
            }
            _ => panic!("Wrong error variant"),
        }
    }

    #[test]
    fn test_error_display_invalid_state() {
        let err = Error::InvalidState("transaction not active".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid state"));
        assert!(msg.contains("transaction not active"));
    }

    #[test]
    fn test_error_display_transaction_conflict() {
        let err = Error::TransactionConflict("read-write conflict on key".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Transaction conflict"));
        assert!(msg.contains("read-write conflict on key"));
    }

    #[test]
    fn test_is_conflict() {
        let conflict = Error::TransactionConflict("conflict".to_string());
        let not_conflict = Error::InvalidState("state".to_string());

        assert!(conflict.is_conflict());
        assert!(!not_conflict.is_conflict());
    }

    #[test]
    fn test_error_display_transaction_timeout() {
        let err = Error::TransactionTimeout("exceeded 5s limit".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Transaction timeout"));
        assert!(msg.contains("exceeded 5s limit"));
    }

    #[test]
    fn test_is_timeout() {
        let timeout = Error::TransactionTimeout("timed out".to_string());
        let not_timeout = Error::InvalidState("state".to_string());
        let conflict = Error::TransactionConflict("conflict".to_string());

        assert!(timeout.is_timeout());
        assert!(!not_timeout.is_timeout());
        assert!(!conflict.is_timeout());
    }
}
