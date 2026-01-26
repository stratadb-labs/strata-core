# M13 Testing Plan

This document defines the testing strategy for M13 (Command Execution Layer), following the [Testing Methodology](../../testing/TESTING_METHODOLOGY.md).

**Guiding Principle**: Tests exist to find bugs, not inflate test counts.

---

## What We're Testing

The executor is a **dispatch layer**. It takes Commands, routes them to the Substrate, and returns Outputs. The key bugs we need to catch:

| Bug Category | Example | Impact |
|--------------|---------|--------|
| **Dispatch errors** | `KvPut` routed to `json_set` | Wrong primitive called |
| **Parameter mapping** | `limit` passed as `offset` | Incorrect results |
| **Output conversion** | Version lost in conversion | Data corruption |
| **Error mapping** | `NotFound` becomes `Internal` | Wrong error to client |
| **Serialization loss** | NaN becomes null, bytes truncated | Data corruption |

---

## What NOT to Test

Following [TESTING_METHODOLOGY.md](../../testing/TESTING_METHODOLOGY.md), we explicitly skip:

### Compiler-Verified Properties
```rust
// DON'T - compiler enforces these
#[test]
fn test_command_is_clone() {
    let cmd = Command::Ping;
    let _ = cmd.clone();
}

#[test]
fn test_output_is_send_sync() {
    fn assert_send<T: Send>() {}
    assert_send::<Output>();
}
```

### Trivial Constructors
```rust
// DON'T - trivial round-trip
#[test]
fn test_kv_put_construction() {
    let cmd = Command::KvPut { run: run_id(), key: "k".into(), value: Value::Int(1) };
    assert!(matches!(cmd, Command::KvPut { .. }));
}
```

### Shallow Assertions
```rust
// DON'T - just checks is_ok()
#[test]
fn test_execute_returns_ok() {
    let result = executor.execute(Command::Ping);
    assert!(result.is_ok());
}

// DO - verify actual result
#[test]
fn test_ping_returns_version() {
    let result = executor.execute(Command::Ping).unwrap();
    match result {
        Output::Pong { version } => assert!(!version.is_empty()),
        _ => panic!("Expected Pong output"),
    }
}
```

---

## Test Categories

### 1. Parity Tests (CRITICAL)

**Purpose**: Every command produces identical results to direct Substrate calls.

**Why this catches bugs**: If the executor transforms inputs/outputs incorrectly, parity breaks.

```rust
/// KV put through executor matches direct substrate call
#[test]
fn test_kv_put_parity() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate.clone());
    let run = ApiRunId::default();

    // Direct substrate call
    let direct_version = substrate.kv_put(&run, "key", Value::Int(42)).unwrap();

    // Reset state
    substrate.kv_delete(&run, "key").unwrap();

    // Executor call
    let cmd = Command::KvPut {
        run: run.clone(),
        key: "key".into(),
        value: Value::Int(42),
    };
    let exec_result = executor.execute(cmd).unwrap();

    // Extract version from output
    let exec_version = match exec_result {
        Output::Version(v) => v,
        _ => panic!("Expected Version output"),
    };

    // Versions should follow same pattern (both Txn-based)
    assert!(matches!(direct_version, Version::Txn(_)));
    assert!(matches!(exec_version, Version::Txn(_)));
}
```

**Coverage**: One parity test per command variant (101 tests).

**Test file**: `tests/executor_comprehensive/parity/`

### 2. Error Mapping Tests (CRITICAL)

**Purpose**: Substrate errors map correctly to Executor errors.

**Why this catches bugs**: Wrong error types break client error handling.

```rust
/// NotFound error preserved through executor
#[test]
fn test_error_not_found_preserved() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate);

    let cmd = Command::KvGet {
        run: ApiRunId::new(), // Non-existent run
        key: "anything".into(),
    };

    let err = executor.execute(cmd).unwrap_err();

    assert!(
        matches!(err, Error::RunNotFound { .. }),
        "NotFound should map to RunNotFound, got: {:?}",
        err
    );
}

/// Constraint violation errors preserved
#[test]
fn test_error_constraint_violation_preserved() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate.clone());
    let run = ApiRunId::default();

    // Create and close a run
    let (info, _) = substrate.run_create(None, None).unwrap();
    substrate.run_close(&info.run_id).unwrap();

    // Try to write to closed run
    let cmd = Command::KvPut {
        run: info.run_id,
        key: "key".into(),
        value: Value::Int(1),
    };

    let err = executor.execute(cmd).unwrap_err();
    assert!(matches!(err, Error::ConstraintViolation { .. }));
}
```

**Coverage**: One test per error variant (25 tests), testing the specific condition that triggers each error.

**Test file**: `tests/executor_comprehensive/errors/`

### 3. Serialization Round-Trip Tests (CRITICAL)

**Purpose**: Commands and Outputs survive JSON serialization without data loss.

**Why this catches bugs**: Lossy serialization corrupts data for non-Rust clients.

```rust
/// Special float values survive round-trip
#[test]
fn test_serialization_special_floats() {
    let values = vec![
        ("positive_infinity", f64::INFINITY),
        ("negative_infinity", f64::NEG_INFINITY),
        ("negative_zero", -0.0_f64),
        // Note: NaN requires special handling - IEEE 754 NaN != NaN
    ];

    for (name, float_val) in values {
        let cmd = Command::KvPut {
            run: ApiRunId::default(),
            key: name.into(),
            value: Value::Float(float_val),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let restored: Command = serde_json::from_str(&json).unwrap();

        match restored {
            Command::KvPut { value: Value::Float(f), .. } => {
                if float_val.is_infinite() {
                    assert_eq!(f.is_infinite(), true);
                    assert_eq!(f.signum(), float_val.signum());
                } else if float_val == -0.0 {
                    assert!(f.is_sign_negative() && f == 0.0);
                }
            }
            _ => panic!("Command structure lost"),
        }
    }
}

/// Binary data survives round-trip
#[test]
fn test_serialization_bytes_roundtrip() {
    let test_bytes = vec![
        vec![],                           // Empty
        vec![0x00],                       // Single null byte
        vec![0x00, 0xFF, 0x00, 0xFF],     // Alternating
        (0..256).map(|i| i as u8).collect::<Vec<_>>(), // All byte values
    ];

    for bytes in test_bytes {
        let cmd = Command::KvPut {
            run: ApiRunId::default(),
            key: "bytes_test".into(),
            value: Value::Bytes(bytes.clone()),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let restored: Command = serde_json::from_str(&json).unwrap();

        match restored {
            Command::KvPut { value: Value::Bytes(b), .. } => {
                assert_eq!(b, bytes, "Bytes should survive round-trip");
            }
            _ => panic!("Command structure lost"),
        }
    }
}

/// Large integers at boundaries survive round-trip
#[test]
fn test_serialization_integer_boundaries() {
    let boundaries = vec![
        i64::MIN,
        i64::MIN + 1,
        -1,
        0,
        1,
        i64::MAX - 1,
        i64::MAX,
    ];

    for int_val in boundaries {
        let cmd = Command::KvPut {
            run: ApiRunId::default(),
            key: "int_test".into(),
            value: Value::Int(int_val),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let restored: Command = serde_json::from_str(&json).unwrap();

        match restored {
            Command::KvPut { value: Value::Int(i), .. } => {
                assert_eq!(i, int_val, "Integer {} should survive round-trip", int_val);
            }
            _ => panic!("Command structure lost"),
        }
    }
}
```

**Coverage**: All 8 Value types, edge cases for each.

**Test file**: `tests/executor_comprehensive/serialization/`

### 4. Dispatch Coverage Tests (HIGH)

**Purpose**: Every command variant reaches the correct handler.

**Why this catches bugs**: Copy-paste errors in match arms.

```rust
/// Each command type calls the correct substrate method
#[test]
fn test_dispatch_kv_commands_reach_kv_substrate() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate.clone());
    let run = ApiRunId::default();

    // Setup: create a key
    substrate.kv_put(&run, "dispatch_test", Value::Int(1)).unwrap();

    // KvGet should read from KV, not JSON or State
    let cmd = Command::KvGet { run: run.clone(), key: "dispatch_test".into() };
    let result = executor.execute(cmd).unwrap();

    match result {
        Output::MaybeVersioned(Some(v)) => {
            assert_eq!(v.value, Value::Int(1));
        }
        _ => panic!("KvGet should return MaybeVersioned with value"),
    }

    // Verify the same key doesn't exist in JSON namespace
    assert!(substrate.json_get(&run, "dispatch_test", "$").unwrap().is_none());
}
```

**Strategy**: For ambiguous commands (e.g., both KV and JSON have "get"), verify the command hits the right namespace by checking side effects.

**Test file**: `tests/executor_comprehensive/dispatch/`

### 5. Edge Case Tests (HIGH)

**Purpose**: Test boundary conditions that could break the executor.

```rust
/// Empty batch operations handled correctly
#[test]
fn test_edge_empty_batch_operations() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate);
    let run = ApiRunId::default();

    // Empty mget
    let cmd = Command::KvMget { run: run.clone(), keys: vec![] };
    let result = executor.execute(cmd).unwrap();
    match result {
        Output::Values(v) => assert!(v.is_empty()),
        _ => panic!("Expected empty Values"),
    }

    // Empty mput
    let cmd = Command::KvMput { run: run.clone(), entries: vec![] };
    let result = executor.execute(cmd);
    // Should either succeed with version or explicitly reject
    assert!(result.is_ok() || matches!(result.unwrap_err(), Error::InvalidInput { .. }));
}

/// Unicode in all string positions
#[test]
fn test_edge_unicode_everywhere() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate);
    let run = ApiRunId::default();

    let unicode_key = "键_キー_مفتاح_🔑";
    let unicode_value = Value::String("值_値_قيمة_💎".into());

    let cmd = Command::KvPut {
        run: run.clone(),
        key: unicode_key.into(),
        value: unicode_value.clone(),
    };

    executor.execute(cmd).unwrap();

    let cmd = Command::KvGet { run: run.clone(), key: unicode_key.into() };
    let result = executor.execute(cmd).unwrap();

    match result {
        Output::MaybeVersioned(Some(v)) => assert_eq!(v.value, unicode_value),
        _ => panic!("Unicode key/value should round-trip"),
    }
}

/// Maximum key length
#[test]
fn test_edge_max_key_length() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate);
    let run = ApiRunId::default();

    let max_key = "k".repeat(1024);
    let cmd = Command::KvPut {
        run: run.clone(),
        key: max_key.clone(),
        value: Value::Int(1),
    };

    let result = executor.execute(cmd);
    assert!(result.is_ok(), "Max length key should be accepted");

    let over_key = "k".repeat(1025);
    let cmd = Command::KvPut {
        run: run.clone(),
        key: over_key,
        value: Value::Int(1),
    };

    let result = executor.execute(cmd);
    assert!(result.is_err(), "Over-max length key should be rejected");
}
```

**Test file**: `tests/executor_comprehensive/edge_cases/`

### 6. execute_many Sequential Order Tests (MEDIUM)

**Purpose**: Batch execution preserves order and stops on error (or continues, depending on semantics).

```rust
/// execute_many processes commands in order
#[test]
fn test_execute_many_order_preserved() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate);
    let run = ApiRunId::default();

    let commands = vec![
        Command::KvPut { run: run.clone(), key: "seq".into(), value: Value::Int(1) },
        Command::KvPut { run: run.clone(), key: "seq".into(), value: Value::Int(2) },
        Command::KvPut { run: run.clone(), key: "seq".into(), value: Value::Int(3) },
    ];

    let results = executor.execute_many(commands);

    // All should succeed
    assert!(results.iter().all(|r| r.is_ok()));

    // Final value should be 3 (last write wins)
    let cmd = Command::KvGet { run: run.clone(), key: "seq".into() };
    let result = executor.execute(cmd).unwrap();
    match result {
        Output::MaybeVersioned(Some(v)) => assert_eq!(v.value, Value::Int(3)),
        _ => panic!("Expected final value 3"),
    }
}

/// execute_many with mid-sequence error
#[test]
fn test_execute_many_error_handling() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate);
    let run = ApiRunId::default();

    let commands = vec![
        Command::KvPut { run: run.clone(), key: "ok1".into(), value: Value::Int(1) },
        Command::KvPut { run: run.clone(), key: "".into(), value: Value::Int(2) }, // Invalid key
        Command::KvPut { run: run.clone(), key: "ok2".into(), value: Value::Int(3) },
    ];

    let results = executor.execute_many(commands);

    assert!(results[0].is_ok(), "First command should succeed");
    assert!(results[1].is_err(), "Second command should fail (empty key)");
    // Third command behavior depends on semantics - document it
    // If continue-on-error: results[2].is_ok()
    // If stop-on-error: results[2] may not exist or be Err
}
```

**Test file**: `tests/executor_comprehensive/batch/`

---

## Test Data Strategy

Following the pattern in `tests/substrate_api_comprehensive/`, use JSONL test data files:

```
tests/executor_comprehensive/
├── testdata/
│   ├── commands.jsonl          # Command variants with expected outputs
│   ├── error_cases.jsonl       # Inputs that should produce specific errors
│   └── serialization.jsonl     # Values that need special serialization handling
├── parity/
│   ├── kv_parity.rs
│   ├── json_parity.rs
│   ├── event_parity.rs
│   ├── state_parity.rs
│   ├── vector_parity.rs
│   ├── run_parity.rs
│   ├── transaction_parity.rs
│   └── retention_parity.rs
├── errors/
│   └── error_mapping.rs
├── serialization/
│   ├── value_types.rs
│   ├── special_floats.rs
│   └── bytes_encoding.rs
├── dispatch/
│   └── dispatch_coverage.rs
├── edge_cases/
│   └── boundaries.rs
├── batch/
│   └── execute_many.rs
└── main.rs
```

---

## Test Counts (Estimated)

| Category | Tests | Rationale |
|----------|-------|-----------|
| Parity | ~101 | One per command variant |
| Error mapping | ~25 | One per error variant |
| Serialization | ~20 | Edge cases for all 8 Value types |
| Dispatch | ~10 | Verify namespace separation |
| Edge cases | ~15 | Boundaries, unicode, empty inputs |
| execute_many | ~5 | Order, error handling |
| **Total** | **~176** | |

Each test catches a specific bug. No vanity tests.

---

## Anti-Pattern Checklist

Before adding a test, verify:

- [ ] **Does this test a specific bug or failure mode?** (If no, don't write it)
- [ ] **Would this test fail if the code was broken?** (If no, it's useless)
- [ ] **Is this testing behavior, not implementation?** (Match on behavior, not struct fields)
- [ ] **Does the assertion check actual values?** (No `is_ok()` without checking content)
- [ ] **Is this something the compiler doesn't verify?** (No Clone/Send/Sync tests)

---

## Integration with Existing Tests

M13 tests should NOT duplicate existing substrate tests. The relationship:

```
┌─────────────────────────────────────────────────────────────────┐
│  tests/substrate_api_comprehensive/                             │
│  Tests: Substrate API correctness, durability, concurrency      │
│  (Already exists - DO NOT DUPLICATE)                            │
└─────────────────────────────────────────────────────────────────┘
                              ▲
                              │ Calls
                              │
┌─────────────────────────────────────────────────────────────────┐
│  tests/executor_comprehensive/                                  │
│  Tests: Command→Substrate dispatch, Output conversion,          │
│         Error mapping, Serialization fidelity                   │
│  (NEW - Tests the translation layer only)                       │
└─────────────────────────────────────────────────────────────────┘
```

The executor tests assume the Substrate is correct (tested elsewhere). They only test the executor's translation fidelity.

---

## Success Criteria

M13 testing is complete when:

1. **All 101 commands have parity tests** - Executor produces same results as direct calls
2. **All error variants have trigger tests** - Each error can be provoked and verified
3. **All Value types round-trip through JSON** - No serialization loss
4. **No vanity tests exist** - Every test catches a real bug
5. **Tests run in < 30 seconds** - Fast feedback loop (in-memory mode)

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-25 | Initial M13 testing plan |
