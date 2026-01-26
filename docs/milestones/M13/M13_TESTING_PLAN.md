# M13 Testing Plan

This document defines the testing strategy for M13 (Command Execution Layer), following the [Testing Methodology](../../testing/TESTING_METHODOLOGY.md).

**Guiding Principle**: Tests exist to find bugs, not inflate test counts.

---

## Critical Context

### Two-Step Landing Strategy

**M13 is additive.** The executor exists alongside strata-api. Deletion comes later.

```
M13 (This Milestone):                M13.1/M14 (Future):
┌─────────────────────┐              ┌─────────────────────┐
│  Python/MCP         │              │  Python/MCP         │
└─────────┬───────────┘              └─────────┬───────────┘
          │                                    │
          ▼                                    ▼
┌─────────────────────┐              ┌─────────────────────┐
│  strata-api         │◄── EXISTS    │  strata-executor    │◄── SOLE API
│  (Substrate/Facade) │              │  (Commands)         │
└─────────┬───────────┘              └─────────┬───────────┘
          │                                    │
┌─────────┴───────────┐                        │
│  strata-executor    │◄── NEW                 │
│  (Commands)         │                        │
└─────────┬───────────┘                        │
          │                                    │
          ▼                                    ▼
┌─────────────────────┐              ┌─────────────────────┐
│  strata-engine      │              │  strata-engine      │
└─────────────────────┘              └─────────────────────┘
```

### M13 Testing Focus

In M13, we test **parity** between executor and substrate:

1. Every executor command produces the same result as the equivalent substrate call
2. Serialization round-trips work correctly
3. Error mapping preserves error types

We do NOT port all substrate tests yet. That happens in M13.1/M14 when substrate is deleted.

### M13.1/M14 Testing Focus (Future)

When strata-api is deleted:

1. Port all `tests/substrate_api_comprehensive/` to use Commands
2. Executor tests become the canonical API tests
3. Test scope expands to durability, concurrency, invariants

---

## What We're Testing in M13

M13 tests focus on the **executor layer itself**, not full API behavior (substrate tests cover that).

| Category | What's Tested | Priority |
|----------|---------------|----------|
| **Parity** | Executor produces same results as substrate | CRITICAL |
| **Serialization** | Commands/Outputs survive JSON round-trip | CRITICAL |
| **Determinism** | Same command + same state = same result | CRITICAL |
| **Error mapping** | Substrate errors map correctly to executor errors | HIGH |
| **Edge cases** | Serialization edge cases (NaN, bytes, i64 boundaries) | HIGH |
| **execute_many** | Batch execution order and semantics | MEDIUM |

**NOT tested in M13** (covered by existing substrate tests, ported in M13.1):
- Durability (crash/recovery)
- Concurrency (thread safety)
- Full behavioral correctness
- All edge cases

---

## M13 Parity Tests

Parity tests verify that executor commands produce the same results as direct substrate calls.

### Strategy

For each of the 101 command variants, write ONE parity test:

```rust
/// KV put through executor matches direct substrate call
#[test]
fn test_parity_kv_put() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate.db());
    let run = ApiRunId::default();

    // Direct substrate call
    let direct_version = substrate.kv_put(&run, "key", Value::Int(42)).unwrap();

    // Reset
    substrate.kv_delete(&run, "key").unwrap();

    // Executor call
    let exec_result = executor.execute(Command::KvPut {
        run: run.clone().into(),
        key: "key".into(),
        value: Value::Int(42),
    }).unwrap();

    // Both should return Version
    match (direct_version, exec_result) {
        (Version::Txn(_), Output::Version(Version::Txn(_))) => (),
        _ => panic!("Version types should match"),
    }
}
```

### Coverage

| Category | Parity Tests | Description |
|----------|--------------|-------------|
| KV | 15 | One per KV command variant |
| JSON | 17 | One per JSON command variant |
| Event | 11 | One per Event command variant |
| State | 8 | One per State command variant |
| Vector | 19 | One per Vector command variant |
| Run | 24 | One per Run command variant |
| Transaction | 5 | One per Transaction command variant |
| Retention | 3 | One per Retention command variant |
| Database | 4 | One per Database command variant |
| **Total** | **106** | |

---

## Tests to Port (M13.1/M14 - FUTURE)

**This section is for planning only.** Test porting happens AFTER M13 when strata-api is deleted.

```
tests/substrate_api_comprehensive/  →  tests/executor_comprehensive/
├── kv/                             →  ├── kv/
├── jsonstore/                      →  ├── json/
├── eventlog/                       →  ├── event/
├── statecell/                      →  ├── state/
├── vectorstore/                    →  ├── vector/
└── runindex/                       →  └── run/
```

**Porting transform**:
```rust
// BEFORE: substrate.method()
substrate.kv_put(&run, "key", Value::Int(42)).unwrap();

// AFTER: executor.execute(Command::...)
executor.execute(Command::KvPut { run, key: "key".into(), value: Value::Int(42) }).unwrap();
```

---

## New Tests (M13-Specific)

### 1. Serialization Round-Trip Tests

Commands must survive JSON serialization for Python/MCP clients.

```rust
/// All command variants serialize and deserialize correctly
#[test]
fn test_all_commands_json_roundtrip() {
    let commands = generate_all_command_variants();

    for (name, cmd) in commands {
        let json = serde_json::to_string(&cmd)
            .expect(&format!("Failed to serialize {}", name));
        let restored: Command = serde_json::from_str(&json)
            .expect(&format!("Failed to deserialize {}", name));

        assert_eq!(cmd, restored, "Command {} failed round-trip", name);
    }
}

/// Special float values in commands survive round-trip
#[test]
fn test_special_floats_in_commands() {
    let test_cases = vec![
        ("infinity", f64::INFINITY),
        ("neg_infinity", f64::NEG_INFINITY),
        ("neg_zero", -0.0_f64),
    ];

    for (name, float) in test_cases {
        let cmd = Command::KvPut {
            run: RunId::default(),
            key: name.into(),
            value: Value::Float(float),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let restored: Command = serde_json::from_str(&json).unwrap();

        if let Command::KvPut { value: Value::Float(f), .. } = restored {
            if float.is_infinite() {
                assert!(f.is_infinite() && f.signum() == float.signum());
            } else if float == -0.0 {
                assert!(f == 0.0 && f.is_sign_negative());
            }
        } else {
            panic!("Wrong command type after round-trip");
        }
    }
}

/// Binary data in commands survives round-trip
#[test]
fn test_bytes_in_commands_roundtrip() {
    let test_bytes = vec![
        vec![],
        vec![0x00, 0xFF],
        (0..=255).collect::<Vec<u8>>(),
    ];

    for bytes in test_bytes {
        let cmd = Command::KvPut {
            run: RunId::default(),
            key: "bytes".into(),
            value: Value::Bytes(bytes.clone()),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        let restored: Command = serde_json::from_str(&json).unwrap();

        if let Command::KvPut { value: Value::Bytes(b), .. } = restored {
            assert_eq!(b, bytes);
        } else {
            panic!("Bytes lost in round-trip");
        }
    }
}
```

### 2. Output Serialization Tests

Outputs must also survive JSON for client responses.

```rust
/// All output variants serialize correctly
#[test]
fn test_all_outputs_json_roundtrip() {
    let outputs = generate_all_output_variants();

    for (name, output) in outputs {
        let json = serde_json::to_string(&output)
            .expect(&format!("Failed to serialize output {}", name));
        let restored: Output = serde_json::from_str(&json)
            .expect(&format!("Failed to deserialize output {}", name));

        assert_eq!(output, restored, "Output {} failed round-trip", name);
    }
}
```

### 3. Error Serialization Tests

Errors must serialize with structured details preserved.

```rust
/// Error details survive serialization
#[test]
fn test_error_details_preserved() {
    let errors = vec![
        Error::KeyNotFound { key: "missing_key".into() },
        Error::RunNotFound { run: "missing_run".into() },
        Error::DimensionMismatch { expected: 128, actual: 256 },
        Error::VersionConflict { expected: 5, actual: 7 },
    ];

    for err in errors {
        let json = serde_json::to_string(&err).unwrap();
        let restored: Error = serde_json::from_str(&json).unwrap();

        // Verify structured fields preserved
        match (&err, &restored) {
            (Error::KeyNotFound { key: k1 }, Error::KeyNotFound { key: k2 }) => {
                assert_eq!(k1, k2);
            }
            (Error::DimensionMismatch { expected: e1, actual: a1 },
             Error::DimensionMismatch { expected: e2, actual: a2 }) => {
                assert_eq!(e1, e2);
                assert_eq!(a1, a2);
            }
            _ => assert_eq!(err, restored),
        }
    }
}
```

### 4. execute_many Tests

Batch execution semantics.

```rust
/// execute_many preserves order
#[test]
fn test_execute_many_order() {
    let executor = quick_executor();
    let run = RunId::default();

    let commands = vec![
        Command::KvPut { run: run.clone(), key: "k".into(), value: Value::Int(1) },
        Command::KvPut { run: run.clone(), key: "k".into(), value: Value::Int(2) },
        Command::KvPut { run: run.clone(), key: "k".into(), value: Value::Int(3) },
    ];

    executor.execute_many(commands);

    // Final value should be 3
    let result = executor.execute(Command::KvGet {
        run: run.clone(),
        key: "k".into(),
    }).unwrap();

    match result {
        Output::MaybeVersioned(Some(v)) => assert_eq!(v.value, Value::Int(3)),
        _ => panic!("Expected value 3"),
    }
}

/// execute_many returns results in same order as commands
#[test]
fn test_execute_many_results_order() {
    let executor = quick_executor();
    let run = RunId::default();

    // Setup
    executor.execute(Command::KvPut {
        run: run.clone(),
        key: "exists".into(),
        value: Value::Int(1),
    }).unwrap();

    let commands = vec![
        Command::KvGet { run: run.clone(), key: "exists".into() },    // Should succeed
        Command::KvGet { run: run.clone(), key: "missing".into() },   // Should return None
        Command::KvExists { run: run.clone(), key: "exists".into() }, // Should return true
    ];

    let results = executor.execute_many(commands);

    assert_eq!(results.len(), 3);
    assert!(matches!(results[0], Ok(Output::MaybeVersioned(Some(_)))));
    assert!(matches!(results[1], Ok(Output::MaybeVersioned(None))));
    assert!(matches!(results[2], Ok(Output::Bool(true))));
}
```

---

## Test File Structure

```
tests/executor_comprehensive/
├── main.rs                      # Test harness, shared utilities
├── test_utils.rs                # quick_executor(), helpers
├── testdata/
│   ├── kv_test_data.jsonl       # Copied from substrate tests
│   ├── edge_cases.jsonl
│   └── serialization_cases.jsonl
│
├── kv/
│   ├── mod.rs
│   ├── basic_ops.rs             # Ported from substrate
│   ├── atomic_ops.rs            # Ported
│   ├── batch_ops.rs             # Ported
│   ├── scan_ops.rs              # Ported
│   ├── edge_cases.rs            # Ported
│   ├── value_types.rs           # Ported
│   ├── durability.rs            # Ported
│   ├── concurrency.rs           # Ported
│   └── transactions.rs          # Ported
│
├── json/                        # Ported from substrate jsonstore/
├── event/                       # Ported from substrate eventlog/
├── state/                       # Ported from substrate statecell/
├── vector/                      # Ported from substrate vectorstore/
├── run/                         # Ported from substrate runindex/
│
├── transaction/                 # NEW - TransactionControl commands
│   ├── basic_ops.rs
│   └── savepoints.rs            # If implemented
│
├── retention/                   # NEW - RetentionSubstrate commands
│   └── basic_ops.rs
│
├── serialization/               # NEW - M13 specific
│   ├── command_roundtrip.rs
│   ├── output_roundtrip.rs
│   ├── error_roundtrip.rs
│   ├── special_values.rs
│   └── bytes_encoding.rs
│
└── batch/                       # NEW - execute_many
    ├── order.rs
    └── error_handling.rs
```

---

## What NOT to Test (Anti-Patterns)

Following [TESTING_METHODOLOGY.md](../../testing/TESTING_METHODOLOGY.md):

### Compiler-Verified Properties
```rust
// DON'T
#[test]
fn test_command_is_clone() { ... }

#[test]
fn test_executor_is_send_sync() { ... }
```

### Shallow Assertions
```rust
// DON'T
#[test]
fn test_kv_put_succeeds() {
    let result = executor.execute(Command::KvPut { ... });
    assert!(result.is_ok());  // Doesn't verify the version
}

// DO
#[test]
fn test_kv_put_returns_version() {
    let result = executor.execute(Command::KvPut { ... }).unwrap();
    match result {
        Output::Version(v) => assert!(v > 0),
        _ => panic!("Expected Version output"),
    }
}
```

### Implementation Details
```rust
// DON'T - tests internal dispatch mechanism
#[test]
fn test_kv_handler_called() {
    // Spy on internal handler...
}

// DO - tests observable behavior
#[test]
fn test_kv_put_stores_value() {
    executor.execute(Command::KvPut { key: "k", value: 42 });
    let result = executor.execute(Command::KvGet { key: "k" });
    // Verify value is stored
}
```

---

## Test Counts (M13 Only)

| Category | Tests | Description |
|----------|-------|-------------|
| Parity | ~106 | One per command variant |
| Serialization (Commands) | ~20 | All command variants round-trip |
| Serialization (Outputs) | ~15 | All output variants round-trip |
| Serialization (Errors) | ~10 | All error variants round-trip |
| Special values | ~15 | NaN, Infinity, bytes, i64 boundaries |
| execute_many | ~10 | Order, error handling |
| **M13 Total** | **~176** | |

### Future (M13.1/M14)

| Category | Tests | Source |
|----------|-------|--------|
| Full KV suite | ~80 | Ported from substrate |
| Full JSON suite | ~60 | Ported from substrate |
| Full Event suite | ~50 | Ported from substrate |
| Full State suite | ~40 | Ported from substrate |
| Full Vector suite | ~50 | Ported from substrate |
| Full Run suite | ~60 | Ported from substrate |
| **M13.1 Total** | **~340** | |

---

## Porting Checklist (M13.1/M14 - FUTURE)

This checklist applies when strata-api is deleted and tests are ported.

For each test file in `substrate_api_comprehensive/`:

- [ ] Create corresponding file in `executor_comprehensive/`
- [ ] Transform `substrate.method()` → `executor.execute(Command::...)`
- [ ] Transform return type assertions → `Output` enum matching
- [ ] Verify test still catches the same bug
- [ ] Remove any tests that become compiler-verified (unlikely)
- [ ] Keep test data files (`.jsonl`)

---

## Success Criteria

### M13 Go/No-Go Gates

M13 testing is complete when these three gates pass:

1. **Parity** - Every executor command produces same result as direct substrate call
2. **Lossless Serialization** - All Commands/Outputs/Errors survive JSON round-trip
3. **Determinism** - Same command + same state = same result

Everything else is optimization.

### Specific Criteria

| Gate | Criteria | How to Test |
|------|----------|-------------|
| Parity | 106 parity tests pass | `cargo test parity` |
| Serialization | All types round-trip | `cargo test serialization` |
| Determinism | Replay produces same state | Replay test with fresh DB |
| Tests run fast | < 30 seconds | In-memory mode |

### M13.1/M14 Criteria (Future)

- All substrate tests ported to executor
- Durability tests pass through executor
- Concurrency tests pass through executor
- strata-api deleted

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-25 | Initial M13 testing plan |
| 1.1 | 2026-01-25 | Revised: executor replaces strata-api, port all tests |
| 1.2 | 2026-01-25 | Corrected: Two-step landing (M13 additive, M13.1 deletes strata-api) |
