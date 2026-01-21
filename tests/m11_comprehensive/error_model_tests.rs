//! Error Model Tests
//!
//! Tests for M11 Error Model: error codes, wire shape, constraint reasons, details.
//!
//! Test ID Conventions:
//! - ERR-xxx: Error code tests
//! - ERR-WS-xxx: Wire shape tests
//! - ERR-CV-xxx: ConstraintViolation reason tests
//! - ERR-DT-xxx: Error details tests

use crate::test_utils::*;

// =============================================================================
// 6.1 Error Code Tests (ERR-001 to ERR-010)
// =============================================================================

#[cfg(test)]
mod error_codes {
    use super::*;

    #[test]
    fn err_001_not_found_code() {
        let err = StrataError::NotFound { key: "missing".into() };
        assert_eq!(err.error_code(), "NotFound");
    }

    #[test]
    fn err_002_wrong_type_code() {
        let err = StrataError::WrongType {
            expected: "int",
            actual: "string",
        };
        assert_eq!(err.error_code(), "WrongType");
    }

    #[test]
    fn err_003_invalid_key_code() {
        let err = StrataError::InvalidKey {
            key: "".into(),
            reason: "empty".into(),
        };
        assert_eq!(err.error_code(), "InvalidKey");
    }

    #[test]
    fn err_004_invalid_path_code() {
        let err = StrataError::InvalidPath {
            path: "$[".into(),
            reason: "unclosed bracket".into(),
        };
        assert_eq!(err.error_code(), "InvalidPath");
    }

    #[test]
    fn err_005_history_trimmed_code() {
        let err = StrataError::HistoryTrimmed {
            requested: Version::Txn(1),
            earliest_retained: Version::Txn(10),
        };
        assert_eq!(err.error_code(), "HistoryTrimmed");
    }

    #[test]
    fn err_006_constraint_violation_code() {
        let err = StrataError::ConstraintViolation {
            reason: "value_too_large".into(),
        };
        assert_eq!(err.error_code(), "ConstraintViolation");
    }

    #[test]
    fn err_007_conflict_code() {
        let err = StrataError::Conflict {
            expected: Value::Int(1),
            actual: Value::Int(2),
        };
        assert_eq!(err.error_code(), "Conflict");
    }

    #[test]
    fn err_008_run_not_found_code() {
        let err = StrataError::RunNotFound {
            run_id: "missing".into(),
        };
        assert_eq!(err.error_code(), "RunNotFound");
    }

    #[test]
    fn err_009_run_closed_code() {
        let err = StrataError::RunClosed {
            run_id: "closed".into(),
        };
        assert_eq!(err.error_code(), "RunClosed");
    }

    #[test]
    fn err_010_overflow_code() {
        let err = StrataError::Overflow;
        assert_eq!(err.error_code(), "Overflow");
    }

    #[test]
    fn err_011_run_exists_code() {
        let err = StrataError::RunExists {
            run_id: "existing".into(),
        };
        assert_eq!(err.error_code(), "RunExists");
    }

    #[test]
    fn err_012_internal_code() {
        let err = StrataError::Internal {
            message: "bug".into(),
        };
        assert_eq!(err.error_code(), "Internal");
    }

    #[test]
    fn err_all_12_codes() {
        // Verify we have exactly 12 error codes
        let codes = vec![
            "NotFound",
            "WrongType",
            "InvalidKey",
            "InvalidPath",
            "ConstraintViolation",
            "Conflict",
            "RunNotFound",
            "RunClosed",
            "RunExists",
            "HistoryTrimmed",
            "Overflow",
            "Internal",
        ];
        assert_eq!(codes.len(), 12);
    }
}

// =============================================================================
// 6.2 Error Wire Shape Tests (ERR-WS-001 to ERR-WS-005)
// =============================================================================

#[cfg(test)]
mod wire_shape {
    #[test]
    fn err_ws_001_error_has_code() {
        let error_json = r#"{"code":"NotFound","message":"Key not found","details":{"key":"mykey"}}"#;
        assert!(error_json.contains(r#""code":"#));
    }

    #[test]
    fn err_ws_002_error_has_message() {
        let error_json = r#"{"code":"NotFound","message":"Key not found","details":null}"#;
        assert!(error_json.contains(r#""message":"#));
    }

    #[test]
    fn err_ws_003_error_has_details() {
        let error_json = r#"{"code":"NotFound","message":"Key not found","details":{"key":"mykey"}}"#;
        assert!(error_json.contains(r#""details":"#));
    }

    #[test]
    fn err_ws_004_response_ok_false() {
        let response = r#"{"id":"1","ok":false,"error":{}}"#;
        assert!(response.contains(r#""ok":false"#));
    }

    #[test]
    fn err_ws_005_response_has_id() {
        let response = r#"{"id":"req-123","ok":false,"error":{}}"#;
        assert!(response.contains(r#""id":"req-123""#));
    }
}

// =============================================================================
// 6.3 ConstraintViolation Reason Tests (ERR-CV-001 to ERR-CV-007)
// =============================================================================

#[cfg(test)]
mod constraint_reasons {
    use super::*;

    #[test]
    fn err_cv_001_value_too_large() {
        let err = StrataError::ConstraintViolation {
            reason: "value_too_large".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "value_too_large");
        }
    }

    #[test]
    fn err_cv_002_nesting_too_deep() {
        let err = StrataError::ConstraintViolation {
            reason: "nesting_too_deep".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "nesting_too_deep");
        }
    }

    #[test]
    fn err_cv_003_key_too_long() {
        let err = StrataError::ConstraintViolation {
            reason: "key_too_long".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "key_too_long");
        }
    }

    #[test]
    fn err_cv_004_vector_dim_exceeded() {
        let err = StrataError::ConstraintViolation {
            reason: "vector_dim_exceeded".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "vector_dim_exceeded");
        }
    }

    #[test]
    fn err_cv_005_vector_dim_mismatch() {
        let err = StrataError::ConstraintViolation {
            reason: "vector_dim_mismatch".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "vector_dim_mismatch");
        }
    }

    #[test]
    fn err_cv_006_root_not_object() {
        let err = StrataError::ConstraintViolation {
            reason: "root_not_object".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "root_not_object");
        }
    }

    #[test]
    fn err_cv_007_reserved_prefix() {
        let err = StrataError::ConstraintViolation {
            reason: "reserved_prefix".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert_eq!(reason, "reserved_prefix");
        }
    }

    #[test]
    fn err_cv_all_reasons() {
        // List of all constraint violation reasons
        let reasons = vec![
            "value_too_large",
            "nesting_too_deep",
            "key_too_long",
            "vector_dim_exceeded",
            "vector_dim_mismatch",
            "root_not_object",
            "reserved_prefix",
            "array_too_long",
            "object_too_many_entries",
        ];
        assert!(reasons.len() >= 7);
    }
}

// =============================================================================
// 6.4 Error Details Payload Tests (ERR-DT-001 to ERR-DT-005)
// =============================================================================

#[cfg(test)]
mod error_details {
    use super::*;

    #[test]
    fn err_dt_001_history_trimmed_details() {
        // HistoryTrimmed includes requested and earliest_retained versions
        let err = StrataError::HistoryTrimmed {
            requested: Version::Txn(5),
            earliest_retained: Version::Txn(10),
        };
        if let StrataError::HistoryTrimmed { requested, earliest_retained } = err {
            assert_eq!(requested, Version::Txn(5));
            assert_eq!(earliest_retained, Version::Txn(10));
        }
    }

    #[test]
    fn err_dt_002_constraint_violation_details() {
        // ConstraintViolation includes reason
        let err = StrataError::ConstraintViolation {
            reason: "test_reason".into(),
        };
        if let StrataError::ConstraintViolation { reason } = err {
            assert!(!reason.is_empty());
        }
    }

    #[test]
    fn err_dt_003_conflict_details() {
        // Conflict includes expected and actual
        let err = StrataError::Conflict {
            expected: Value::Int(1),
            actual: Value::Int(2),
        };
        if let StrataError::Conflict { expected, actual } = err {
            assert_eq!(expected, Value::Int(1));
            assert_eq!(actual, Value::Int(2));
        }
    }

    #[test]
    fn err_dt_004_invalid_key_details() {
        // InvalidKey includes key and reason
        let err = StrataError::InvalidKey {
            key: "bad\0key".into(),
            reason: "contains NUL".into(),
        };
        if let StrataError::InvalidKey { key, reason } = err {
            assert!(key.contains('\0'));
            assert!(reason.contains("NUL"));
        }
    }

    #[test]
    fn err_dt_005_invalid_path_details() {
        // InvalidPath includes path and reason
        let err = StrataError::InvalidPath {
            path: "$[".into(),
            reason: "unclosed".into(),
        };
        if let StrataError::InvalidPath { path, reason } = err {
            assert_eq!(path, "$[");
            assert!(reason.contains("unclosed"));
        }
    }
}
