//! KV Scan Operations Tests
//!
//! Tests for key enumeration and scanning operations:
//! - kv_keys: List keys matching a prefix
//! - kv_scan: Paginated scan with cursor support
//!
//! ## Status: IMPLEMENTED
//!
//! These operations are implemented in the substrate API:
//!
//! ```rust
//! // List keys with optional prefix filter
//! fn kv_keys(&self, run: &ApiRunId, prefix: &str, limit: Option<usize>)
//!     -> StrataResult<Vec<String>>;
//!
//! // Paginated scan with cursor support
//! fn kv_scan(&self, run: &ApiRunId, prefix: &str, limit: usize, cursor: Option<&str>)
//!     -> StrataResult<KVScanResult>;
//! ```

use crate::test_data::load_kv_test_data;
use crate::*;
use std::collections::HashSet;

// =============================================================================
// KV_KEYS TESTS (List keys by prefix)
// =============================================================================

/// Basic key listing should return all keys
#[test]
fn test_kv_keys_lists_all_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Insert some entries
    let entries: Vec<_> = test_data.get_run(0).iter().take(5).collect();
    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    let keys = substrate.kv_keys(&run, "", None).unwrap();
    assert_eq!(keys.len(), 5, "Should list all 5 keys");
    for entry in &entries {
        assert!(keys.contains(&entry.key), "Should contain key '{}'", entry.key);
    }
}

/// Key listing with prefix filter
#[test]
fn test_kv_keys_with_prefix_filter() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create keys with different prefixes
    substrate.kv_put(&run, "user:1:name", Value::String("Alice".into())).unwrap();
    substrate.kv_put(&run, "user:1:email", Value::String("alice@example.com".into())).unwrap();
    substrate.kv_put(&run, "user:2:name", Value::String("Bob".into())).unwrap();
    substrate.kv_put(&run, "session:abc", Value::String("data".into())).unwrap();

    // Filter by "user:1:" prefix
    let user1_keys = substrate.kv_keys(&run, "user:1:", None).unwrap();
    assert_eq!(user1_keys.len(), 2);
    assert!(user1_keys.contains(&"user:1:name".to_string()));
    assert!(user1_keys.contains(&"user:1:email".to_string()));

    // Filter by "user:" prefix (all users)
    let all_user_keys = substrate.kv_keys(&run, "user:", None).unwrap();
    assert_eq!(all_user_keys.len(), 3);

    // Filter by "session:" prefix
    let session_keys = substrate.kv_keys(&run, "session:", None).unwrap();
    assert_eq!(session_keys.len(), 1);
}

/// Key listing respects limit parameter
#[test]
fn test_kv_keys_respects_limit() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create 20 keys
    for i in 0..20 {
        substrate.kv_put(&run, &format!("key:{:02}", i), Value::Int(i)).unwrap();
    }

    let keys = substrate.kv_keys(&run, "", Some(5)).unwrap();
    assert_eq!(keys.len(), 5, "Should return only 5 keys due to limit");
}

/// Key listing returns empty for no matches
#[test]
fn test_kv_keys_empty_for_no_matches() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.kv_put(&run, "existing:key", Value::Int(1)).unwrap();

    let keys = substrate.kv_keys(&run, "nonexistent:", None).unwrap();
    assert!(keys.is_empty(), "Should return empty vec for no matches");
}

/// Key listing excludes deleted keys
#[test]
fn test_kv_keys_excludes_deleted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.kv_put(&run, "keep:a", Value::Int(1)).unwrap();
    substrate.kv_put(&run, "keep:b", Value::Int(2)).unwrap();
    substrate.kv_put(&run, "delete:c", Value::Int(3)).unwrap();
    substrate.kv_delete(&run, "delete:c").unwrap();

    let keys = substrate.kv_keys(&run, "", None).unwrap();
    assert_eq!(keys.len(), 2, "Should not include deleted key");
    assert!(!keys.contains(&"delete:c".to_string()));
}

/// Key listing is isolated per run
#[test]
fn test_kv_keys_run_isolation() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();

    substrate.kv_put(&run1, "run1:key", Value::Int(1)).unwrap();
    substrate.kv_put(&run2, "run2:key", Value::Int(2)).unwrap();

    let run1_keys = substrate.kv_keys(&run1, "", None).unwrap();
    let run2_keys = substrate.kv_keys(&run2, "", None).unwrap();

    assert_eq!(run1_keys.len(), 1);
    assert!(run1_keys.contains(&"run1:key".to_string()));

    assert_eq!(run2_keys.len(), 1);
    assert!(run2_keys.contains(&"run2:key".to_string()));
}

// =============================================================================
// KV_SCAN TESTS (Paginated scanning with cursor)
// =============================================================================

/// Basic scan returns keys and values
#[test]
fn test_kv_scan_basic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Insert entries
    let entries: Vec<_> = test_data.get_run(0).iter().take(5).collect();
    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    let result = substrate.kv_scan(&run, "", 100, None).unwrap();
    assert_eq!(result.entries.len(), 5, "Should return all 5 entries");
    assert!(result.cursor.is_none(), "No cursor needed for complete scan");
}

/// Scan with prefix filter
#[test]
fn test_kv_scan_with_prefix() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.kv_put(&run, "user:1:profile", Value::String("Alice".into())).unwrap();
    substrate.kv_put(&run, "user:1:settings", Value::Object(HashMap::new())).unwrap();
    substrate.kv_put(&run, "user:2:profile", Value::String("Bob".into())).unwrap();
    substrate.kv_put(&run, "config:app", Value::String("value".into())).unwrap();

    let result = substrate.kv_scan(&run, "user:1:", 100, None).unwrap();
    assert_eq!(result.entries.len(), 2);

    // Verify both key and value are returned
    for (key, versioned) in &result.entries {
        assert!(key.starts_with("user:1:"));
        assert!(versioned.value != Value::Null);
    }
}

/// Scan pagination with cursor
#[test]
fn test_kv_scan_pagination() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create 25 keys
    for i in 0..25 {
        substrate.kv_put(&run, &format!("item:{:02}", i), Value::Int(i)).unwrap();
    }

    // First page
    let page1 = substrate.kv_scan(&run, "item:", 10, None).unwrap();
    assert_eq!(page1.entries.len(), 10);
    assert!(page1.cursor.is_some(), "Should have cursor for more pages");

    // Second page
    let page2 = substrate.kv_scan(&run, "item:", 10, page1.cursor.as_deref()).unwrap();
    assert_eq!(page2.entries.len(), 10);
    assert!(page2.cursor.is_some());

    // Third page (partial)
    let page3 = substrate.kv_scan(&run, "item:", 10, page2.cursor.as_deref()).unwrap();
    assert_eq!(page3.entries.len(), 5);
    assert!(page3.cursor.is_none(), "No more pages");

    // Verify no duplicates across pages
    let all_keys: HashSet<_> = page1.entries.iter()
        .chain(page2.entries.iter())
        .chain(page3.entries.iter())
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(all_keys.len(), 25, "Should have 25 unique keys across all pages");
}

/// Scan returns entries in consistent order
#[test]
fn test_kv_scan_consistent_order() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create keys
    for i in 0..10 {
        substrate.kv_put(&run, &format!("order:{:02}", i), Value::Int(i)).unwrap();
    }

    // Two scans should return same order
    let scan1 = substrate.kv_scan(&run, "order:", 100, None).unwrap();
    let scan2 = substrate.kv_scan(&run, "order:", 100, None).unwrap();

    let keys1: Vec<_> = scan1.entries.iter().map(|(k, _)| k.clone()).collect();
    let keys2: Vec<_> = scan2.entries.iter().map(|(k, _)| k.clone()).collect();

    assert_eq!(keys1, keys2, "Scans should return keys in consistent order");
}

/// Scan excludes deleted keys
#[test]
fn test_kv_scan_excludes_deleted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.kv_put(&run, "scan:a", Value::Int(1)).unwrap();
    substrate.kv_put(&run, "scan:b", Value::Int(2)).unwrap();
    substrate.kv_put(&run, "scan:c", Value::Int(3)).unwrap();
    substrate.kv_delete(&run, "scan:b").unwrap();

    let result = substrate.kv_scan(&run, "scan:", 100, None).unwrap();
    assert_eq!(result.entries.len(), 2);

    let keys: Vec<_> = result.entries.iter().map(|(k, _)| k.as_str()).collect();
    assert!(keys.contains(&"scan:a"));
    assert!(!keys.contains(&"scan:b")); // deleted
    assert!(keys.contains(&"scan:c"));
}

/// Scan is isolated per run
#[test]
fn test_kv_scan_run_isolation() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();

    substrate.kv_put(&run1, "scan:run1:a", Value::Int(1)).unwrap();
    substrate.kv_put(&run1, "scan:run1:b", Value::Int(2)).unwrap();
    substrate.kv_put(&run2, "scan:run2:a", Value::Int(3)).unwrap();

    let run1_result = substrate.kv_scan(&run1, "scan:", 100, None).unwrap();
    let run2_result = substrate.kv_scan(&run2, "scan:", 100, None).unwrap();

    assert_eq!(run1_result.entries.len(), 2);
    assert_eq!(run2_result.entries.len(), 1);
}

// =============================================================================
// CROSS-MODE EQUIVALENCE
// =============================================================================

/// kv_keys should behave identically across durability modes
#[test]
fn test_kv_keys_cross_mode() {
    test_across_modes("kv_keys_cross_mode", |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        substrate.kv_put(&run, "a:1", Value::Int(1)).unwrap();
        substrate.kv_put(&run, "a:2", Value::Int(2)).unwrap();
        substrate.kv_put(&run, "b:1", Value::Int(3)).unwrap();

        let mut keys = substrate.kv_keys(&run, "a:", None).unwrap();
        keys.sort();
        keys
    });
}

/// kv_scan should behave identically across durability modes
#[test]
fn test_kv_scan_cross_mode() {
    test_across_modes("kv_scan_cross_mode", |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        substrate.kv_put(&run, "x:1", Value::Int(1)).unwrap();
        substrate.kv_put(&run, "x:2", Value::Int(2)).unwrap();

        let result = substrate.kv_scan(&run, "x:", 100, None).unwrap();
        let mut keys: Vec<_> = result.entries.iter().map(|(k, _)| k.clone()).collect();
        keys.sort();
        keys
    });
}
