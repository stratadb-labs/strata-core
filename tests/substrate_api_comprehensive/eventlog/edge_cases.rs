//! EventLog Edge Cases Tests
//!
//! Tests for edge cases and validation:
//! - Payload must be Object type (not other Value types)
//! - Stream name validation
//! - Empty streams
//! - Large payloads
//! - Nested objects
//! - Special values in payloads
//!
//! Some tests use data from testdata/eventlog_test_data.jsonl

use crate::test_data::load_eventlog_test_data;
use crate::*;
use std::collections::HashMap;

// =============================================================================
// PAYLOAD TYPE VALIDATION
// The EventLog contract requires payloads to be Object type
// =============================================================================

#[test]
fn test_payload_must_be_object_null_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(&run, "stream1", Value::Null);

    // Contract says: "Payload not Object" -> ConstraintViolation
    assert!(
        result.is_err(),
        "Null payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_must_be_object_bool_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(&run, "stream1", Value::Bool(true));

    assert!(
        result.is_err(),
        "Bool payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_must_be_object_int_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(&run, "stream1", Value::Int(42));

    assert!(
        result.is_err(),
        "Int payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_must_be_object_float_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(&run, "stream1", Value::Float(3.14));

    assert!(
        result.is_err(),
        "Float payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_must_be_object_string_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(&run, "stream1", Value::String("hello".into()));

    assert!(
        result.is_err(),
        "String payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_must_be_object_bytes_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(&run, "stream1", Value::Bytes(vec![1, 2, 3]));

    assert!(
        result.is_err(),
        "Bytes payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_must_be_object_array_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.event_append(
        &run,
        "stream1",
        Value::Array(vec![Value::Int(1), Value::Int(2)]),
    );

    assert!(
        result.is_err(),
        "Array payload should be rejected: {:?}",
        result
    );
}

#[test]
fn test_payload_object_accepted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("key".to_string(), Value::String("value".into()));
        m
    });

    let result = substrate.event_append(&run, "stream1", payload);

    assert!(result.is_ok(), "Object payload should be accepted: {:?}", result);
}

#[test]
fn test_payload_empty_object_accepted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let payload = Value::Object(HashMap::new());

    let result = substrate.event_append(&run, "stream1", payload);

    assert!(
        result.is_ok(),
        "Empty object payload should be accepted: {:?}",
        result
    );
}

// =============================================================================
// STREAM NAME VALIDATION
// =============================================================================

#[test]
fn test_stream_name_empty_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];
    let result = substrate.event_append(&run, "", entry.payload.clone());

    // Contract says: "Invalid stream name" -> InvalidKey
    assert!(
        result.is_err(),
        "Empty stream name should be rejected: {:?}",
        result
    );
}

#[test]
fn test_stream_name_whitespace_only() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];
    // This may or may not be rejected depending on implementation
    let result = substrate.event_append(&run, "   ", entry.payload.clone());

    // Document behavior - whitespace-only stream names might be allowed
    // but are probably not a good idea
    if result.is_err() {
        // If rejected, that's fine
    } else {
        // If accepted, verify we can read it back
        let events = substrate
            .event_range(&run, "   ", None, None, None)
            .expect("range should succeed");
        assert_eq!(events.len(), 1, "Should have 1 event in whitespace stream");
    }
}

#[test]
fn test_stream_name_very_long() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Create a very long stream name
    let long_name = "a".repeat(1000);
    let entry = &test_data.entries[0];

    let result = substrate.event_append(&run, &long_name, entry.payload.clone());

    // Document behavior - very long names might be rejected or truncated
    if result.is_ok() {
        // If accepted, verify we can read it back
        let events = substrate
            .event_range(&run, &long_name, None, None, None)
            .expect("range should succeed");
        assert_eq!(events.len(), 1, "Should have 1 event in long-named stream");
    }
}

// =============================================================================
// EMPTY STREAM EDGE CASES
// =============================================================================

#[test]
fn test_range_on_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let events = substrate
        .event_range(&run, "nonexistent", None, None, None)
        .expect("range should succeed");

    assert!(events.is_empty(), "Empty stream should return empty vec");
}

#[test]
fn test_get_on_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate
        .event_get(&run, "nonexistent", 1)
        .expect("get should succeed");

    assert!(result.is_none(), "Get on empty stream should return None");
}

#[test]
fn test_len_on_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let len = substrate
        .event_len(&run, "nonexistent")
        .expect("len should succeed");

    assert_eq!(len, 0, "Empty stream should have len 0");
}

#[test]
fn test_latest_sequence_on_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let latest = substrate
        .event_latest_sequence(&run, "nonexistent")
        .expect("latest_sequence should succeed");

    assert!(
        latest.is_none(),
        "Empty stream should have no latest sequence"
    );
}

// =============================================================================
// LARGE PAYLOAD EDGE CASES
// =============================================================================

#[test]
fn test_large_object_payload() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create a large object with many fields
    let mut m = HashMap::new();
    for i in 0..100 {
        m.insert(format!("field_{}", i), Value::String(format!("value_{}", i)));
    }
    let payload = Value::Object(m);

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(result.is_ok(), "Large object should be accepted");

    // Verify it can be read back
    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
    assert_eq!(events[0].value, payload, "Payload should match");
}

#[test]
fn test_deeply_nested_object_payload() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create a deeply nested object
    fn create_nested(depth: i32) -> Value {
        if depth == 0 {
            Value::String("leaf".into())
        } else {
            let mut m = HashMap::new();
            m.insert("nested".to_string(), create_nested(depth - 1));
            m.insert("depth".to_string(), Value::Int(depth as i64));
            Value::Object(m)
        }
    }

    let payload = create_nested(20);

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(result.is_ok(), "Deeply nested object should be accepted");

    // Verify it can be read back
    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
    assert_eq!(events[0].value, payload, "Payload should match");
}

#[test]
fn test_object_with_bytes_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Bytes within object should be allowed (per contract)
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("binary_data".to_string(), Value::Bytes(vec![0x00, 0xFF, 0xAB, 0xCD]));
        m.insert("description".to_string(), Value::String("contains binary".into()));
        m
    });

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(
        result.is_ok(),
        "Object with bytes value should be accepted: {:?}",
        result
    );

    // Verify it can be read back
    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
    // Bytes may be wrapped differently on wire, compare structure
    if let Value::Object(ref em) = events[0].value {
        assert!(em.contains_key("binary_data"), "Should have binary_data key");
        assert!(em.contains_key("description"), "Should have description key");
    }
}

#[test]
fn test_object_with_array_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Array within object should be allowed
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert(
            "items".to_string(),
            Value::Array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
            ]),
        );
        m
    });

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(result.is_ok(), "Object with array value should be accepted");

    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
    assert_eq!(events[0].value, payload, "Payload should match");
}

#[test]
fn test_object_with_null_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Null within object should be allowed
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("null_field".to_string(), Value::Null);
        m.insert("other_field".to_string(), Value::Int(42));
        m
    });

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(result.is_ok(), "Object with null value should be accepted");

    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
    assert_eq!(events[0].value, payload, "Payload should match");
}

// =============================================================================
// RANGE EDGE CASES
// =============================================================================

#[test]
fn test_range_start_beyond_end() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append some events using test data
    for entry in test_data.take(5) {
        substrate
            .event_append(&run, "range_edge_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    // Request range where start > end
    let events = substrate
        .event_range(&run, "range_edge_stream", Some(100), Some(10), None)
        .expect("range should succeed");

    // Should return empty (or error, depending on implementation)
    assert!(
        events.is_empty(),
        "Range with start > end should return empty or error"
    );
}

#[test]
fn test_range_limit_zero() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append some events using test data
    for entry in test_data.take(5) {
        substrate
            .event_append(&run, "limit_zero_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    // Request with limit 0
    let events = substrate
        .event_range(&run, "limit_zero_stream", None, None, Some(0))
        .expect("range should succeed");

    assert!(events.is_empty(), "Limit 0 should return empty vec");
}

#[test]
fn test_range_limit_larger_than_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append 3 events using test data
    for entry in test_data.take(3) {
        substrate
            .event_append(&run, "limit_large_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    // Request with limit larger than stream
    let events = substrate
        .event_range(&run, "limit_large_stream", None, None, Some(100))
        .expect("range should succeed");

    assert_eq!(events.len(), 3, "Should return all 3 events, not fail");
}

// =============================================================================
// GET EDGE CASES
// =============================================================================

#[test]
fn test_get_sequence_zero() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Sequence 0 should not exist (sequences start at 1 or higher)
    let result = substrate
        .event_get(&run, "stream1", 0)
        .expect("get should succeed");

    assert!(
        result.is_none(),
        "Sequence 0 should not exist: {:?}",
        result
    );
}

#[test]
fn test_get_max_sequence() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Getting u64::MAX sequence should not panic
    let result = substrate.event_get(&run, "stream1", u64::MAX);

    // Should either return None or an error, not panic
    match result {
        Ok(None) => {} // Expected
        Ok(Some(_)) => panic!("u64::MAX sequence should not exist"),
        Err(_) => {} // Also acceptable
    }
}

// =============================================================================
// SPECIAL VALUE EDGE CASES
// =============================================================================

#[test]
fn test_float_special_values_in_payload() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test special float values
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("infinity".to_string(), Value::Float(f64::INFINITY));
        m.insert("neg_infinity".to_string(), Value::Float(f64::NEG_INFINITY));
        m.insert("nan".to_string(), Value::Float(f64::NAN));
        m.insert("zero".to_string(), Value::Float(0.0));
        m.insert("neg_zero".to_string(), Value::Float(-0.0));
        m
    });

    let result = substrate.event_append(&run, "stream1", payload);

    // Special floats should be handled gracefully
    // (either accepted or rejected with clear error)
    if result.is_ok() {
        let events = substrate
            .event_range(&run, "stream1", None, None, None)
            .expect("range should succeed");
        assert_eq!(events.len(), 1, "Should have 1 event");
    }
}

#[test]
fn test_empty_string_key_in_payload() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Empty string as key in object
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("".to_string(), Value::String("empty key".into()));
        m.insert("normal_key".to_string(), Value::String("normal value".into()));
        m
    });

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(result.is_ok(), "Empty string key in payload should be accepted");

    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
}

#[test]
fn test_unicode_key_in_payload() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Unicode keys in object
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("日本語".to_string(), Value::String("Japanese".into()));
        m.insert("العربية".to_string(), Value::String("Arabic".into()));
        m.insert("emoji_🎉".to_string(), Value::String("Emoji".into()));
        m
    });

    let result = substrate.event_append(&run, "stream1", payload.clone());
    assert!(result.is_ok(), "Unicode keys in payload should be accepted");

    let events = substrate
        .event_range(&run, "stream1", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1, "Should have 1 event");
    assert_eq!(events[0].value, payload, "Payload should match");
}

// =============================================================================
// NEGATIVE TESTS - INVALID PAYLOADS FROM TEST DATA
// =============================================================================

#[test]
fn test_all_invalid_payloads_rejected() {
    // Test that ALL invalid payloads from the test data file are rejected with errors
    // Invalid payloads return Err (not silently dropped) and nothing is stored
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut rejection_count = 0;

    for invalid in test_data.get_invalid_payloads() {
        let result = substrate.event_append(&run, "invalid_test_stream", invalid.payload.clone());

        match &result {
            Err(e) => {
                // Verify the error message indicates payload validation failure
                let error_msg = format!("{}", e);
                assert!(
                    error_msg.contains("Object") || error_msg.contains("payload") || error_msg.contains("invalid"),
                    "Error for '{}' should mention Object/payload/invalid: {}",
                    invalid.name,
                    error_msg
                );
                rejection_count += 1;
            }
            Ok(_) => {
                panic!(
                    "Invalid payload '{}' ({:?}) should be rejected but was accepted",
                    invalid.name,
                    invalid.payload
                );
            }
        }
    }

    // Verify NO events were stored (rejections don't silently drop, they error out)
    let len = substrate
        .event_len(&run, "invalid_test_stream")
        .expect("len should succeed");

    assert_eq!(len, 0, "No invalid payloads should be stored - got {} events", len);
    assert!(rejection_count >= 8, "Should have rejected at least 8 invalid payloads, rejected {}", rejection_count);
}

#[test]
fn test_invalid_payload_error_message() {
    // Verify the specific error message returned for invalid payloads
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test each invalid type and verify error message
    let test_cases = vec![
        ("int", Value::Int(42)),
        ("string", Value::String("not an object".into())),
        ("float", Value::Float(3.14)),
        ("bool", Value::Bool(true)),
        ("null", Value::Null),
        ("array", Value::Array(vec![Value::Int(1)])),
        ("bytes", Value::Bytes(vec![1, 2, 3])),
    ];

    for (name, payload) in test_cases {
        let result = substrate.event_append(&run, "error_msg_test", payload);

        assert!(result.is_err(), "{} should be rejected", name);
        let error = result.unwrap_err();
        let error_string = format!("{}", error);

        // The error should clearly indicate the payload must be an Object
        assert!(
            error_string.contains("Object"),
            "{} error should mention 'Object': got '{}'",
            name,
            error_string
        );
    }

    // Verify nothing was stored
    let len = substrate.event_len(&run, "error_msg_test").expect("len");
    assert_eq!(len, 0, "No invalid payloads should be stored");
}

#[test]
fn test_dirty_data_accepted() {
    // Test that edge-case valid payloads from the dirty test data are accepted
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Test a sampling of entries from each stream (dirty data should work)
    let sample_size = 100;
    let mut accepted = 0;
    let mut rejected = 0;

    for entry in test_data.take(sample_size) {
        let result = substrate.event_append(&run, &entry.stream, entry.payload.clone());
        if result.is_ok() {
            accepted += 1;
        } else {
            rejected += 1;
            // Log which payloads are unexpectedly rejected
            eprintln!(
                "Unexpected rejection for entry {}: {:?}",
                entry.event_index, result
            );
        }
    }

    assert_eq!(
        rejected, 0,
        "All valid dirty data should be accepted, but {} of {} were rejected",
        rejected, sample_size
    );
    assert_eq!(accepted, sample_size, "All {} entries should be accepted", sample_size);
}

#[test]
fn test_security_payloads_stored_not_executed() {
    // Security test: SQL injection / XSS patterns should be stored as-is
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Find security stream entries
    let security_entries = test_data.get_stream("security");
    assert!(!security_entries.is_empty(), "Should have security stream entries");

    // Append security payloads
    for entry in security_entries.iter().take(20) {
        let result = substrate.event_append(&run, "security_test", entry.payload.clone());
        assert!(
            result.is_ok(),
            "Security payload should be stored (not executed): {:?}",
            result
        );
    }

    // Read back and verify they're stored exactly as-is
    let events = substrate
        .event_range(&run, "security_test", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 20.min(security_entries.len()), "All security events should be stored");

    for (i, event) in events.iter().enumerate() {
        assert_eq!(
            event.value, security_entries[i].payload,
            "Security payload {} should be stored exactly as-is",
            i
        );
    }
}

#[test]
fn test_unicode_payloads_preserved() {
    // Unicode data should be preserved correctly
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create a payload with various unicode
    let payload = Value::Object({
        let mut m = HashMap::new();
        m.insert("chinese".to_string(), Value::String("测试数据 中文".into()));
        m.insert("japanese".to_string(), Value::String("テストデータ 日本語".into()));
        m.insert("korean".to_string(), Value::String("테스트 데이터 한국어".into()));
        m.insert("arabic".to_string(), Value::String("اختبار البيانات".into()));
        m.insert("hebrew".to_string(), Value::String("נתוני בדיקה".into()));
        m.insert("russian".to_string(), Value::String("тестовые данные".into()));
        m.insert("emoji".to_string(), Value::String("🎉🔥💯🚀🌍".into()));
        m.insert("mixed".to_string(), Value::String("Hello 世界 مرحبا 🌍".into()));
        m
    });

    substrate
        .event_append(&run, "unicode_stream", payload.clone())
        .expect("unicode payload should be accepted");

    let events = substrate
        .event_range(&run, "unicode_stream", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].value, payload, "Unicode should be preserved exactly");
}

#[test]
fn test_deeply_nested_payloads() {
    // Deeply nested objects from test data should work
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Look for nested payloads in edge_cases stream
    let edge_cases = test_data.get_stream("edge_cases");
    let mut nested_count = 0;

    for entry in edge_cases.iter().take(50) {
        // Check if payload looks nested
        if let Value::Object(ref m) = entry.payload {
            if m.contains_key("data") || m.contains_key("nested") {
                let result = substrate.event_append(&run, "nested_test", entry.payload.clone());
                if result.is_ok() {
                    nested_count += 1;
                }
            }
        }
    }

    assert!(nested_count > 0, "Should have processed some nested payloads");

    let events = substrate
        .event_range(&run, "nested_test", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), nested_count, "All nested payloads should be retrievable");
}

#[test]
fn test_large_batch_from_dirty_data() {
    // Test appending a large batch of dirty data
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let batch_size = 1000;
    let entries: Vec<_> = test_data.take(batch_size).to_vec();

    // Append all entries
    for entry in &entries {
        substrate
            .event_append(&run, "batch_stream", entry.payload.clone())
            .expect("batch append should succeed");
    }

    // Verify count
    let len = substrate
        .event_len(&run, "batch_stream")
        .expect("len should succeed");

    assert_eq!(len, batch_size as u64, "All {} events should be stored", batch_size);

    // Verify range retrieval
    let events = substrate
        .event_range(&run, "batch_stream", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), batch_size, "All events should be retrievable");
}

// =============================================================================
// CROSS-MODE EQUIVALENCE
// =============================================================================

#[test]
fn test_edge_cases_cross_mode() {
    test_across_modes("eventlog_edge_cases", |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        // Test payload validation across modes
        let valid_payload = Value::Object({
            let mut m = HashMap::new();
            m.insert("test".to_string(), Value::Bool(true));
            m
        });
        let invalid_payload = Value::String("not an object".into());

        let valid_result = substrate.event_append(&run, "stream1", valid_payload);
        let invalid_result = substrate.event_append(&run, "stream1", invalid_payload);

        (valid_result.is_ok(), invalid_result.is_err())
    });
}
