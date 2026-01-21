# M11 Public API & SDK Contract - Autonomous Execution Prompt

**Usage**: `claude --dangerously-skip-permissions -p "$(cat docs/prompts/M11/M11_AUTONOMOUS_EXECUTION.md)"`

---

## Task

Execute M11 Epics 80-87 with phased implementation and epic-end validation after each epic.

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "M11" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - "Strata" IS allowed and encouraged in the codebase (e.g., `StrataError`, `strata_value`)
>
> **CORRECT**: `pub enum StrataError { ... }`
> **WRONG**: `//! M11 Value type`

## M11 Philosophy

> M11 is a **contract milestone**, not a feature milestone. It freezes the public API surface so all downstream consumers (wire protocol, CLI, SDKs, server) have a stable foundation.
>
> After M11, breaking changes require a major version bump. The contract defines what users observe. Internal implementation details remain flexible.
>
> **M11 does NOT add new capabilities.** It stabilizes, documents, and validates the existing API surface.

## The Eight Architectural Rules

These rules are NON-NEGOTIABLE. Violating any is a blocking issue.

1. **Facade Desugars to Substrate**: Every facade op maps to deterministic substrate ops
2. **No Hidden Errors**: Facade surfaces all substrate errors unchanged
3. **No Type Coercion**: `Int(1)` != `Float(1.0)`, `String` != `Bytes`
4. **Explicit Run Scoping**: Substrate requires run_id, Facade targets default
5. **Wire Encoding Preserves Types**: Round-trip must be lossless
6. **Errors Are Explicit**: All invalid inputs produce explicit errors
7. **Contract Stability**: Frozen elements cannot change without major version
8. **Default Run Is Literal "default"**: Always exists, cannot be closed

## Core Invariants

### Value Model (VAL-1 to VAL-5)
- Eight types only: Null, Bool, Int, Float, String, Bytes, Array, Object
- No implicit type coercions
- IEEE-754 float equality (NaN != NaN, -0.0 == 0.0)

### Facade (FAC-1 to FAC-5)
- Every facade op maps to deterministic substrate ops
- Facade adds no semantic behavior beyond defaults
- Facade never swallows substrate errors

### Wire Encoding (WIRE-1 to WIRE-5)
- Bytes encode as `{"$bytes": "<base64>"}`
- Non-finite floats encode as `{"$f64": "NaN"|"+Inf"|"-Inf"|"-0.0"}`
- Absent values encode as `{"$absent": true}`
- Round-trip preserves exact type and value

### Error Model (ERR-1 to ERR-4)
- 12 error codes: NotFound, WrongType, InvalidKey, InvalidPath, ConstraintViolation, Conflict, RunNotFound, RunClosed, RunExists, HistoryTrimmed, Overflow, Internal
- All errors include code, message, details
- `Conflict` = temporal (CAS failure); `ConstraintViolation` = structural (limits)

## Execution Pattern

For each epic in the recommended order:

1. **Read specs first**:
   - `docs/milestones/M11/M11_CONTRACT.md` (AUTHORITATIVE)
   - `docs/milestones/M11/M11_IMPLEMENTATION_PLAN.md`
   - `docs/milestones/M11/EPIC_{N}_*.md`

2. **Start epic branch**: `./scripts/start-story.sh {epic} {first-story} {desc}`

3. **Implement all stories** per epic specification using TDD

4. **Run epic-end validation** (MANDATORY - see below)

5. **Merge to develop** (only after validation passes):
   ```bash
   git checkout develop
   git merge --no-ff epic-{N}-* -m "Epic {N}: {Name} complete"
   git push origin develop
   ```

6. **Proceed to next epic**

## Epic-End Validation (MANDATORY)

**After completing each epic, you MUST run the full epic-end validation before proceeding.**

Reference: `docs/prompts/EPIC_END_VALIDATION.md`

### Quick Validation Checklist

Run all 7 phases for each epic:

```bash
# Phase 1: Automated Checks
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
~/.cargo/bin/cargo doc --workspace --no-deps && \
echo "Phase 1: PASS"
```

| Phase | Description | Required |
|-------|-------------|----------|
| 1 | Automated Checks (build, test, clippy, fmt, docs) | ✓ |
| 2 | Story Completion Verification | ✓ |
| 3 | Spec Compliance Review (M11_CONTRACT.md) | ✓ |
| 4 | Code Review Checklist | ✓ |
| 5 | Best Practices Verification | ✓ |
| 6 | Epic-Specific Validation (M11 rules below) | ✓ |
| 7 | Final Sign-Off | ✓ |

### M11-Specific Phase 6 Validation

For M11 epics, Phase 6 must verify the Eight Architectural Rules:

```bash
# Rule 3: No type coercion
~/.cargo/bin/cargo test nc_ -- --nocapture

# Rule 5: Wire encoding round-trip
~/.cargo/bin/cargo test wire_round_trip_ -- --nocapture

# M11 comprehensive tests
~/.cargo/bin/cargo test --test m11_comprehensive
```

### DO NOT proceed to the next epic if:

- Any Phase 1 check fails
- Any story is incomplete
- Any M11 architectural rule is violated
- Any invariant (VAL, FAC, WIRE, ERR) is broken
- Tests are failing

## Recommended Execution Order

M11 is split into **M11a (Core Contract)** and **M11b (Consumer Surfaces)**.

### M11a: Core Contract (Must complete before M11b)

#### Phase 1: Data Model Foundation

**Epics**: 80 (Value Model) + 83 (Wire Encoding)

1. **Epic 80: Value Model Stabilization** - FOUNDATION
   - Stories #550-#555: Value enum, float edge cases, equality, no coercion, size limits, key validation
   - Start: `./scripts/start-story.sh 80 550 value-enum-finalization`

2. **Epic 83: Wire Encoding Contract**
   - Stories #574-#579: Envelope, value mapping, $bytes, $f64, $absent, Versioned encoding
   - Start: `./scripts/start-story.sh 83 574 request-response-envelope`

**Exit Criteria**: All 8 value types encode/decode correctly. Round-trip tests pass.

#### Phase 2: Error Model

**Epic**: 84 (Error Model Finalization)

1. **Epic 84: Error Model**
   - Stories #581-#585: Error codes, wire shape, constraint reasons, details
   - Start: `./scripts/start-story.sh 84 581 error-code-enumeration`

**Exit Criteria**: All error conditions produce correct structured errors.

#### Phase 3: API Layers

**Epics**: 81 (Facade API) + 82 (Substrate API)

1. **Epic 81: Facade API Implementation**
   - Stories #557-#564: KV, JSON, Event, Vector, State, History, Run, Capabilities
   - Start: `./scripts/start-story.sh 81 557 kv-operations`

2. **Epic 82: Substrate API Implementation**
   - Stories #566-#572: KVStore, JsonStore, EventLog, StateCell, VectorStore, TraceStore, RunIndex
   - Start: `./scripts/start-story.sh 82 566 kvstore-substrate`

**Exit Criteria**: Both API layers complete. Parity tests pass.

#### Phase 4: Core Validation (M11a Exit Gate)

**Epic**: 87a (Core Validation Suite)

1. **Epic 87a: Core Validation**
   - Stories #587-#590: Facade-Substrate parity, value round-trip, wire conformance, determinism
   - Start: `./scripts/start-story.sh 87 587 facade-substrate-parity`

**Exit Criteria**: All M11a contract guarantees validated. Zero defects in core contract.

---

### M11b: Consumer Surfaces (After M11a complete)

#### Phase 5: CLI + SDK

**Epics**: 85 (CLI) + 86 (SDK Foundation)

1. **Epic 85: CLI Implementation**
   - CLI argument parsing, all commands, output formatting
   - Start: `./scripts/start-story.sh 85 <first-story> cli-argument-parser`

2. **Epic 86: SDK Foundation**
   - Rust SDK, Python/JS mappings, conformance harness
   - Start: `./scripts/start-story.sh 86 <first-story> sdk-value-mapping`

**Exit Criteria**: CLI works for all facade operations. SDK conformance harness passes.

#### Phase 6: Surface Validation (M11b Exit Gate)

**Epic**: 87b (Surface Validation Suite)

1. **Epic 87b: Surface Validation**
   - CLI conformance, SDK conformance, regression suite

**Exit Criteria**: All M11b contract guarantees validated.

## GitHub Issue Mapping

### M11a (Core Contract)

| Epic | GitHub Issue | Story Issues | Phase |
|------|--------------|--------------|-------|
| Epic 80: Value Model | #549 | #550-#555 | 1 |
| Epic 83: Wire Encoding | #573 | #574-#579 | 1 |
| Epic 84: Error Model | #580 | #581-#585 | 2 |
| Epic 81: Facade API | #556 | #557-#564 | 3 |
| Epic 82: Substrate API | #565 | #566-#572 | 3 |
| Epic 87a: Core Validation | #586 | #587-#590 | 4 |

### M11b (Consumer Surfaces)

| Epic | GitHub Issue | Story Issues | Phase |
|------|--------------|--------------|-------|
| Epic 85: CLI | TBD | TBD | 5 |
| Epic 86: SDK Foundation | TBD | TBD | 5 |
| Epic 87b: Surface Validation | TBD | TBD | 6 |

## Stop Conditions

- Any architectural rule violation (8 rules)
- Any invariant violation (VAL, FAC, WIRE, ERR)
- Epic-end validation failure
- Test failures that can't be resolved
- Type coercion anywhere in the codebase
- Wire encoding that loses type information
- Facade operation that doesn't desugar to substrate

## Validation Between Phases

After each phase, run validation:

```bash
# Phase 1: Automated checks (must all pass)
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
echo "Phase 1: PASS"
```

### M11-Specific Validation

```bash
# Run M11 comprehensive tests
~/.cargo/bin/cargo test --test m11_comprehensive

# Run value model tests
~/.cargo/bin/cargo test value_model_

# Run wire encoding tests
~/.cargo/bin/cargo test wire_encoding_

# Run facade parity tests
~/.cargo/bin/cargo test facade_substrate_parity_

# Run error model tests
~/.cargo/bin/cargo test error_model_

# Run no-coercion tests
~/.cargo/bin/cargo test no_coercion_
```

### Eight Rules Quick Check

```bash
# Rule 3: No type coercion - CRITICAL
echo "Rule 3: No type coercion"
~/.cargo/bin/cargo test nc_ -- --nocapture

# Rule 5: Wire encoding preserves types
echo "Rule 5: Wire encoding round-trip"
~/.cargo/bin/cargo test wire_round_trip_ -- --nocapture

# Rule 8: Default run is literal "default"
echo "Rule 8: Default run"
grep -r 'DEFAULT.*=.*"default"' crates/ && echo "PASS" || echo "CHECK"
```

## Files to Create

### New Crate Structure

```
crates/api/src/
├── facade/                 # Redis-like API
│   ├── mod.rs
│   ├── kv.rs
│   ├── json.rs
│   ├── event.rs
│   ├── vector.rs
│   ├── state.rs
│   └── history.rs
└── substrate/             # Power-user API
    ├── mod.rs
    ├── kv.rs
    ├── json.rs
    ├── event.rs
    ├── state.rs
    ├── vector.rs
    └── run.rs

crates/wire/src/
├── json/
│   ├── mod.rs
│   ├── value.rs           # Value JSON encoding
│   ├── wrappers.rs        # $bytes, $f64, $absent
│   ├── envelope.rs        # Request/response
│   └── version.rs         # Version encoding
└── lib.rs

crates/cli/src/
├── main.rs
├── parser.rs
├── commands/
└── output.rs
```

## Common Patterns

### Value Equality (No Coercion)

```rust
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b, // IEEE-754
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => a == b,
            // Different types: NEVER equal
            _ => false,
        }
    }
}
```

### Wire Encoding Pattern

```rust
pub fn encode_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_nan() {
                r#"{"$f64":"NaN"}"#.to_string()
            } else if *f == f64::INFINITY {
                r#"{"$f64":"+Inf"}"#.to_string()
            } else if *f == f64::NEG_INFINITY {
                r#"{"$f64":"-Inf"}"#.to_string()
            } else if f.to_bits() == (-0.0_f64).to_bits() {
                r#"{"$f64":"-0.0"}"#.to_string()
            } else {
                f.to_string()
            }
        }
        Value::String(s) => format!("\"{}\"", escape_json_string(s)),
        Value::Bytes(b) => format!(r#"{{"$bytes":"{}"}}"#, base64_encode(b)),
        Value::Array(arr) => { ... }
        Value::Object(obj) => { ... }
    }
}
```

### Facade Desugaring Pattern

```rust
// Facade: set(key, value)
pub fn set(&self, key: &str, value: Value) -> Result<()> {
    // Desugars to: kv_put(default_run, key, value)
    self.substrate.kv_put(&DEFAULT_RUN_ID, key, value)?;
    Ok(())
}

// Facade: get(key)
pub fn get(&self, key: &str) -> Result<Option<Value>> {
    // Desugars to: kv_get(default_run, key).map(|v| v.value)
    Ok(self.substrate.kv_get(&DEFAULT_RUN_ID, key)?.map(|v| v.value))
}
```

## Troubleshooting

### "Int(1) equals Float(1.0)"

This violates Rule 3 (No Type Coercion). This is a BLOCKING BUG.

1. Check `PartialEq` implementation for Value
2. Ensure different enum variants return `false`
3. Run `cargo test nc_001_int_one_not_float_one`

### "Wire encoding loses type"

This violates Rule 5 (Wire Encoding Preserves Types).

1. Verify Bytes uses `{"$bytes": "..."}`
2. Verify special floats use `{"$f64": "..."}`
3. Run round-trip tests

### "Facade swallows error"

This violates Rule 2 (No Hidden Errors).

1. Never use `.ok()` or `.unwrap_or()` in facade
2. Propagate all errors with `?`
3. Run error propagation tests

## Start

Begin with Phase 1: Epic 80 (Value Model Stabilization).

Read the specs:
1. `docs/milestones/M11/M11_CONTRACT.md`
2. `docs/milestones/M11/M11_IMPLEMENTATION_PLAN.md`
3. `docs/milestones/M11/EPIC_80_VALUE_MODEL.md`

Then start with Story #550 (Value Enum Finalization):
```bash
./scripts/start-story.sh 80 550 value-enum-finalization
```

Remember:
- **Rule 3: No Type Coercion** - `Int(1) != Float(1.0)` is CRITICAL
- **TDD**: Write tests FIRST, then implementation
- **NEVER modify tests to make them pass** - fix the implementation
- All epic-end validation uses comprehensive test suite

---

*End of M11 Autonomous Execution Prompt*
