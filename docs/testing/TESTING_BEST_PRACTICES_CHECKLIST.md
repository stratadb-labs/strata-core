# Testing Best Practices Checklist

This checklist evaluates whether unit tests are meaningful and thorough, not just shallow "does it compile" tests.

## 1. Test Coverage Depth

### 1.1 Core Logic Testing
- [ ] **Happy path tested**: Normal/expected inputs produce correct outputs
- [ ] **Edge cases tested**: Boundary conditions, empty inputs, max values
- [ ] **Error paths tested**: Invalid inputs return appropriate errors
- [ ] **State transitions tested**: For stateful code, all valid transitions covered

### 1.2 Assertion Quality
- [ ] **Specific assertions**: Tests assert on actual values, not just `is_ok()` or `is_some()`
- [ ] **Multiple properties checked**: Tests verify more than one aspect of the result
- [ ] **Error messages verified**: Error cases check the error type/message, not just `is_err()`
- [ ] **No trivial assertions**: Avoid `assert!(true)` or `assert_eq!(x, x)`

### 1.3 Test Independence
- [ ] **No order dependence**: Tests can run in any order
- [ ] **Isolated state**: Each test sets up its own state
- [ ] **No shared mutable state**: Tests don't rely on global/static state

## 2. Test Categories

### 2.1 Unit Tests (per function/method)
- [ ] **Pure functions**: Input/output relationships verified
- [ ] **Methods with side effects**: State changes verified
- [ ] **Constructors**: All construction paths tested
- [ ] **Trait implementations**: Each trait method tested

### 2.2 Property-Based Considerations
- [ ] **Invariants maintained**: Key invariants checked after operations
- [ ] **Idempotency**: Where applicable, repeated operations produce same result
- [ ] **Commutativity/Associativity**: Where applicable, order independence verified
- [ ] **Round-trip**: Serialize/deserialize, encode/decode produce original

### 2.3 Error Handling
- [ ] **All error variants tested**: Each error type in Result has a test
- [ ] **Error propagation**: Errors from dependencies are properly wrapped
- [ ] **Panic conditions**: `#[should_panic]` tests for intentional panics

## 3. Red Flags (Shallow Test Indicators)

### 3.1 Compilation-Only Tests
```rust
// BAD: Only tests that code compiles
#[test]
fn test_foo() {
    let _ = Foo::new();
}
```

### 3.2 Type-System Tests
```rust
// BAD: Only tests trait bounds
#[test]
fn test_is_send_sync() {
    fn assert_send<T: Send>() {}
    assert_send::<MyType>();
}
```
*Note: These have value but should NOT be the only tests*

### 3.3 Trivial Assertions
```rust
// BAD: Asserts on construction, not behavior
#[test]
fn test_default() {
    let x = MyType::default();
    assert!(x.is_empty()); // Only checks one trivial property
}
```

### 3.4 No Verification Tests
```rust
// BAD: Calls method but doesn't verify result
#[test]
fn test_process() {
    let mut x = MyType::new();
    x.process(data); // No assertion on result or state!
}
```

## 4. Good Test Patterns

### 4.1 Behavior Verification
```rust
// GOOD: Tests actual behavior
#[test]
fn test_insert_and_get() {
    let mut map = MyMap::new();
    map.insert("key", 42);

    assert_eq!(map.get("key"), Some(&42));
    assert_eq!(map.len(), 1);
    assert!(map.contains_key("key"));
}
```

### 4.2 Error Case Verification
```rust
// GOOD: Tests error type and message
#[test]
fn test_invalid_input_error() {
    let result = parse("");

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ParseError::EmptyInput));
    assert!(err.to_string().contains("empty"));
}
```

### 4.3 State Transition Verification
```rust
// GOOD: Tests state machine transitions
#[test]
fn test_state_transitions() {
    let mut fsm = StateMachine::new();
    assert_eq!(fsm.state(), State::Initial);

    fsm.start();
    assert_eq!(fsm.state(), State::Running);

    fsm.stop();
    assert_eq!(fsm.state(), State::Stopped);
}
```

### 4.4 Invariant Verification
```rust
// GOOD: Verifies invariants after operations
#[test]
fn test_sorted_insert_maintains_order() {
    let mut list = SortedList::new();
    list.insert(3);
    list.insert(1);
    list.insert(2);

    // Verify invariant: list is always sorted
    let items: Vec<_> = list.iter().collect();
    assert_eq!(items, vec![&1, &2, &3]);
}
```

## 5. Crate-Specific Considerations

### 5.1 Storage Crates
- [ ] Data persists correctly across operations
- [ ] Corruption is detected (CRC, checksums)
- [ ] Recovery produces correct state
- [ ] Concurrent access handled properly

### 5.2 Serialization Crates
- [ ] Round-trip (encode then decode = original)
- [ ] Malformed input rejected with proper error
- [ ] Edge cases: empty, max size, special characters
- [ ] Version compatibility if applicable

### 5.3 Concurrency Crates
- [ ] Thread safety verified (not just Send+Sync bounds)
- [ ] Race conditions tested (multiple threads exercising same code)
- [ ] Deadlock scenarios considered
- [ ] Lock ordering verified

### 5.4 API Crates
- [ ] All public methods tested
- [ ] Error responses match documentation
- [ ] Input validation tested
- [ ] Stateful operations tested in sequence

## 6. Integration Test Best Practices

### 6.1 Scope and Purpose
- [ ] **Cross-module interaction**: Tests verify modules work together correctly
- [ ] **Real dependencies**: Uses actual implementations, not mocks (where appropriate)
- [ ] **End-to-end flows**: Complete user workflows tested
- [ ] **System boundaries**: Tests at API/interface boundaries

### 6.2 Integration Test Quality
- [ ] **Realistic scenarios**: Tests mirror actual usage patterns
- [ ] **Data flow verification**: Data flows correctly through system layers
- [ ] **Error propagation**: Errors propagate correctly across boundaries
- [ ] **Resource cleanup**: Tests clean up resources (files, connections, etc.)

### 6.3 Recovery and Durability Tests
- [ ] **Crash recovery**: System recovers correctly after simulated crash
- [ ] **Data persistence**: Data survives restart/reopen
- [ ] **Corruption handling**: Corrupted data detected and handled
- [ ] **Partial failure**: System handles partial writes/operations

### 6.4 Concurrency Integration Tests
- [ ] **Multi-threaded workloads**: Concurrent operations tested
- [ ] **Race condition scenarios**: Known race-prone patterns exercised
- [ ] **Deadlock detection**: Long-running concurrent tests don't hang
- [ ] **Resource contention**: System handles contention gracefully

### 6.5 Integration Test Red Flags

```rust
// BAD: Integration test that only tests one component
#[test]
fn test_database_open() {
    let db = Database::open(path).unwrap();
    // No actual operations or verification!
}
```

```rust
// BAD: No verification of cross-component interaction
#[test]
fn test_write_and_read() {
    db.write(key, value);
    db.read(key);
    // Doesn't verify the read returns what was written!
}
```

### 6.6 Good Integration Test Patterns

```rust
// GOOD: Tests complete flow with verification
#[test]
fn test_write_persist_recover() {
    let path = temp_dir();

    // Write data
    {
        let db = Database::open(&path).unwrap();
        db.put("key", "value").unwrap();
    } // db dropped, simulating close

    // Reopen and verify
    {
        let db = Database::open(&path).unwrap();
        assert_eq!(db.get("key").unwrap(), Some("value"));
    }
}
```

```rust
// GOOD: Tests error propagation across layers
#[test]
fn test_storage_error_surfaces_to_api() {
    let db = Database::open_readonly(&path).unwrap();

    let result = db.put("key", "value");

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ApiError::ReadOnly));
}
```

## 7. Evaluation Rubric

For each crate, rate the following (1-5):

### Unit Tests

| Criterion | Score | Notes |
|-----------|-------|-------|
| **Core Logic Coverage** | | Are the main algorithms/logic tested? |
| **Edge Case Coverage** | | Are boundary conditions tested? |
| **Error Path Coverage** | | Are error cases tested with specific assertions? |
| **Assertion Quality** | | Do assertions verify actual behavior? |
| **Test Independence** | | Can tests run in isolation? |
| **Shallow Test Ratio** | | What % of tests are compile-only or trivial? |

### Integration Tests

| Criterion | Score | Notes |
|-----------|-------|-------|
| **Cross-Module Coverage** | | Are component interactions tested? |
| **Recovery/Durability** | | Are crash/restart scenarios tested? |
| **Concurrent Workloads** | | Are multi-threaded scenarios tested? |
| **Realistic Scenarios** | | Do tests mirror actual usage? |
| **Error Propagation** | | Are errors tested across boundaries? |

**Scoring Guide:**
- 5: Excellent - Thorough coverage, meaningful assertions, realistic scenarios
- 4: Good - Most important paths covered, some gaps
- 3: Adequate - Core functionality tested, missing edge cases
- 2: Weak - Many shallow tests, key paths untested
- 1: Poor - Mostly compile-only or trivial tests

## 8. Specific Questions to Ask

When evaluating tests, ask:

1. **"If I broke the core logic, would this test catch it?"**
   - If no, the test is too shallow

2. **"Does this test verify behavior or just structure?"**
   - Testing that a struct has fields is shallow
   - Testing that operations produce correct results is meaningful

3. **"Could this test pass with a broken implementation?"**
   - Tests that only check `is_ok()` could pass with wrong data
   - Tests should verify the actual values/state

4. **"Is there a test for each error path?"**
   - Every `Result::Err` variant should have a test
   - Every panic condition should have a `#[should_panic]` test

5. **"Are the tests testing the contract or the implementation?"**
   - Good tests verify the public contract/behavior
   - Brittle tests verify internal implementation details
