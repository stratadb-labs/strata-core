# Testing Methodology

This document defines our testing philosophy and practices. The goal is to **find bugs**, not inflate test counts.

---

## Core Principles

### 1. Tests Exist to Find Bugs

A test that never fails is worthless. Every test should be written with a specific failure mode in mind:
- What bug would this catch?
- Has this bug happened before?
- Could this bug happen during refactoring?

If you can't answer these questions, don't write the test.

### 2. Test Behavior, Not Implementation

**Bad**: Testing that a struct has certain fields or that a method exists
**Good**: Testing that the system produces correct outputs for given inputs

```rust
// BAD - tests implementation details
#[test]
fn test_user_has_name_field() {
    let user = User::new("Alice");
    assert!(!user.name.is_empty());
}

// GOOD - tests behavior
#[test]
fn test_user_greeting_includes_name() {
    let user = User::new("Alice");
    assert_eq!(user.greeting(), "Hello, Alice!");
}
```

### 3. One Failure Mode Per Test

Each test should verify one specific thing that could go wrong. If a test fails, you should immediately know what's broken.

---

## What NOT to Test

### Compiler-Verified Properties

Don't test things the compiler already guarantees:

```rust
// DON'T - compiler enforces Clone
#[test]
fn test_foo_is_clone() {
    let a = Foo::new();
    let b = a.clone();
    assert_eq!(a, b);
}

// DON'T - compiler enforces Send + Sync
#[test]
fn test_foo_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    assert_send::<Foo>();
    assert_sync::<Foo>();
}

// DON'T - compiler enforces Debug
#[test]
fn test_foo_debug() {
    let f = Foo::new();
    let s = format!("{:?}", f);
    assert!(s.contains("Foo"));
}
```

### Trivial Constructors and Accessors

```rust
// DON'T - trivial round-trip
#[test]
fn test_point_new() {
    let p = Point::new(1, 2);
    assert_eq!(p.x(), 1);
    assert_eq!(p.y(), 2);
}

// DON'T - Default trait
#[test]
fn test_config_default() {
    let c = Config::default();
    assert!(c.timeout > 0);
}
```

### Shallow Assertions

```rust
// DON'T - is_ok() without checking value
#[test]
fn test_parse_works() {
    let result = parse("input");
    assert!(result.is_ok());
}

// DO - verify actual result
#[test]
fn test_parse_extracts_value() {
    let result = parse("key=value").unwrap();
    assert_eq!(result.key, "key");
    assert_eq!(result.value, "value");
}
```

---

## What TO Test

### 1. Business Logic and Algorithms

Test the actual computations your code performs:

```rust
#[test]
fn test_bm25_score_increases_with_term_frequency() {
    let scorer = BM25Scorer::new();
    let score_1 = scorer.score("rust", doc_with_term_count("rust", 1));
    let score_5 = scorer.score("rust", doc_with_term_count("rust", 5));
    assert!(score_5 > score_1, "More occurrences should increase score");
}
```

### 2. Edge Cases and Boundaries

Test the limits of your system:

```rust
#[test]
fn test_empty_input() {
    assert_eq!(process(""), Output::Empty);
}

#[test]
fn test_at_capacity_limit() {
    let mut buffer = Buffer::with_capacity(100);
    for i in 0..100 {
        buffer.push(i).unwrap();
    }
    assert!(buffer.push(100).is_err(), "Should reject at capacity");
}

#[test]
fn test_max_u64_value() {
    let v = Version::new(u64::MAX);
    assert_eq!(v.increment(), Version::new(u64::MAX)); // saturating
}
```

### 3. Error Conditions

Test that errors are produced correctly:

```rust
#[test]
fn test_invalid_input_returns_specific_error() {
    let err = parse("malformed{{").unwrap_err();
    assert!(matches!(err, ParseError::UnbalancedBraces { position: 9 }));
}

#[test]
fn test_not_found_error_includes_key() {
    let err = store.get("missing").unwrap_err();
    match err {
        StoreError::NotFound { key } => assert_eq!(key, "missing"),
        _ => panic!("Expected NotFound error"),
    }
}
```

### 4. State Transitions and Invariants

Test that invariants are maintained:

```rust
#[test]
fn test_transaction_isolation() {
    let db = Database::new();

    // Start transaction, write value
    let tx1 = db.begin();
    tx1.put("key", "value1");

    // Another transaction shouldn't see uncommitted write
    let tx2 = db.begin();
    assert_eq!(tx2.get("key"), None, "Uncommitted writes should be invisible");

    tx1.commit();

    // New transaction sees committed value
    let tx3 = db.begin();
    assert_eq!(tx3.get("key"), Some("value1"));
}
```

### 5. Concurrent Behavior

Test thread safety with actual concurrency:

```rust
#[test]
fn test_concurrent_increments_are_serialized() {
    let counter = Arc::new(AtomicCounter::new(0));
    let threads: Vec<_> = (0..10)
        .map(|_| {
            let c = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..1000 {
                    c.increment();
                }
            })
        })
        .collect();

    for t in threads {
        t.join().unwrap();
    }

    assert_eq!(counter.get(), 10_000, "All increments should be counted");
}
```

### 6. Recovery and Crash Scenarios

Test system resilience:

```rust
#[test]
fn test_recovery_after_crash_mid_transaction() {
    let dir = tempdir();

    // Write and commit
    {
        let db = Database::open(&dir);
        db.put("committed", "value");
        db.begin();
        db.put("uncommitted", "value"); // Don't commit
        // Simulate crash - drop without commit
    }

    // Reopen and verify
    let db = Database::open(&dir);
    assert_eq!(db.get("committed"), Some("value"));
    assert_eq!(db.get("uncommitted"), None, "Uncommitted should be rolled back");
}
```

---

## Test Organization

### Naming Convention

Test names should describe the scenario and expected outcome:

```
test_<action>_<condition>_<expected_result>
```

Examples:
- `test_parse_empty_string_returns_none`
- `test_commit_with_conflict_returns_error`
- `test_recovery_replays_committed_transactions`

### Test Structure

Use Arrange-Act-Assert (AAA) pattern:

```rust
#[test]
fn test_transfer_between_accounts() {
    // Arrange
    let mut bank = Bank::new();
    bank.create_account("alice", 100);
    bank.create_account("bob", 50);

    // Act
    let result = bank.transfer("alice", "bob", 30);

    // Assert
    assert!(result.is_ok());
    assert_eq!(bank.balance("alice"), 70);
    assert_eq!(bank.balance("bob"), 80);
}
```

### When to Use Integration vs Unit Tests

**Unit tests**: Test a single function or module in isolation
- Fast (< 10ms each)
- No I/O, no threads
- Mock dependencies

**Integration tests**: Test multiple components working together
- May be slower
- Use real I/O (tempdir, actual files)
- Test the public API

---

## Property-Based Testing

For complex logic, consider property-based testing:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn roundtrip_serialization(value: JsonValue) {
        let serialized = value.to_string();
        let deserialized: JsonValue = serialized.parse().unwrap();
        assert_eq!(value, deserialized);
    }

    #[test]
    fn sort_is_idempotent(mut vec: Vec<i32>) {
        vec.sort();
        let sorted = vec.clone();
        vec.sort();
        assert_eq!(vec, sorted);
    }
}
```

---

## Test Quality Checklist

Before adding a test, ask:

- [ ] Does this test a specific bug or failure mode?
- [ ] Would this test fail if the code was broken?
- [ ] Is this testing behavior, not implementation?
- [ ] Does the test name describe what's being verified?
- [ ] Is the assertion checking actual values, not just `is_ok()`?
- [ ] Is this something the compiler doesn't already verify?

---

## Anti-Patterns to Avoid

| Anti-Pattern | Example | Problem |
|--------------|---------|---------|
| Tautology tests | `assert!(x.is_ok() \|\| x.is_err())` | Always passes |
| Compiler tests | `fn assert_send<T: Send>() {}` | Compiler already checks |
| Shallow assertions | `assert!(result.is_some())` | Doesn't verify value |
| Implementation coupling | Testing private fields | Breaks on refactor |
| Test duplication | Same logic in 5 tests | Maintenance burden |
| Flaky tests | Tests that sometimes fail | Erode trust |
| Slow tests | Tests that take > 1s | Slow feedback loop |

---

## Metrics That Matter

**Good metrics:**
- Bugs found by tests (in CI, during development)
- Mutation testing score (do tests catch code changes?)
- Time to identify root cause when tests fail

**Vanity metrics (avoid optimizing for these):**
- Raw test count
- Line coverage percentage
- Test/code ratio

---

## Summary

Write tests that:
1. **Find bugs** - not confirm the obvious
2. **Test behavior** - not implementation
3. **Verify values** - not just success/failure
4. **Cover edge cases** - empty, max, concurrent
5. **Document intent** - test name explains the scenario

Delete tests that:
1. Test compiler-enforced properties
2. Only check `is_ok()` or `is_some()`
3. Test trivial constructors/accessors
4. Duplicate other tests
5. Never fail
