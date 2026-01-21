# M11 Epic Prompt Header

**Copy this header to the top of every M11 epic prompt file (Epics 80-87).**

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "M11" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - "Strata" IS allowed and encouraged in the codebase (e.g., `StrataError`, `strata_value`)
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Strata value model types and equality semantics`
> **CORRECT**: `pub enum StrataError { ... }`
> **WRONG**: `//! M11 Value type for public API`
> **WRONG**: `m11_value_tests.rs`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M11 implementation:**

1. **`docs/milestones/M11/M11_CONTRACT.md`** - THE AUTHORITATIVE CONTRACT SPECIFICATION
2. **`docs/milestones/M11/M11_IMPLEMENTATION_PLAN.md`** - Epic/Story breakdown and phased approach
3. **`docs/milestones/M11/EPIC_*.md`** - Story-level specifications with TDD tests
4. **`docs/milestones/M11/M11_TESTING_PLAN.md`** - 500+ test specifications

**The contract spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the contract spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M11:

1. **Every story MUST implement behavior EXACTLY as specified in the Epic documents**
   - No "improvements" that deviate from the spec
   - No "simplifications" that change behavior
   - No "optimizations" that break guarantees

2. **If your code contradicts the spec, YOUR CODE IS WRONG**
   - The spec defines correct behavior
   - Fix the code, not the spec

3. **If your tests contradict the spec, YOUR TESTS ARE WRONG**
   - Tests must validate spec-compliant behavior
   - Never adjust tests to make broken code pass

4. **If the spec seems wrong or unclear:**
   - STOP implementation immediately
   - Raise the issue for discussion
   - Do NOT proceed with assumptions
   - Do NOT implement your own interpretation

5. **No breaking the spec for ANY reason:**
   - Not for "performance"
   - Not for "simplicity"
   - Not for "it's just an edge case"
   - Not for "we can fix it later"

---

## THE EIGHT ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in EVERY M11 story. Violating any of these is a blocking issue.**

### Rule 1: Facade Desugars to Substrate

> **Every facade operation MUST map to a deterministic sequence of substrate operations. No hidden semantics.**

```rust
// CORRECT: Facade is thin wrapper over substrate
pub fn set(&self, key: &str, value: Value) -> Result<()> {
    // Desugars to: begin(); kv_put(default, key, value); commit()
    self.substrate.kv_put(&self.default_run, key, value)?;
    Ok(())
}

// WRONG: Facade adds hidden behavior
pub fn set(&self, key: &str, value: Value) -> Result<()> {
    let value = self.coerce_to_string(value); // NEVER transform values
    self.substrate.kv_put(&self.default_run, key, value)?;
    Ok(())
}
```

### Rule 2: No Hidden Errors

> **The facade MUST surface all substrate errors unchanged. No swallowing, transforming, or hiding errors.**

```rust
// CORRECT: Error propagates unchanged
pub fn get(&self, key: &str) -> Result<Option<Value>> {
    self.substrate.kv_get(&self.default_run, key)
}

// WRONG: Error is swallowed
pub fn get(&self, key: &str) -> Option<Value> {
    self.substrate.kv_get(&self.default_run, key).ok().flatten() // NEVER swallow errors
}
```

### Rule 3: No Type Coercion

> **Values MUST NOT be implicitly converted between types. `Int(1)` does not equal `Float(1.0)`.**

```rust
// CORRECT: Types are distinct
assert_ne!(Value::Int(1), Value::Float(1.0));
assert_ne!(Value::String("abc".into()), Value::Bytes(b"abc".to_vec()));

// WRONG: Implicit coercion
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(i), Value::Float(f)) => *i as f64 == *f, // NEVER coerce
            // ...
        }
    }
}
```

### Rule 4: Explicit Run Scoping

> **Substrate operations MUST require explicit `run_id`. Facade operations MUST target the default run.**

```rust
// CORRECT: Substrate requires run_id
pub fn kv_put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version>;

// CORRECT: Facade targets default
pub fn set(&self, key: &str, value: Value) -> Result<()> {
    self.substrate.kv_put(&DEFAULT_RUN_ID, key, value)
}

// WRONG: Substrate with implicit run
pub fn kv_put(&self, key: &str, value: Value) -> Result<Version>; // Missing run_id!

// WRONG: Facade with explicit run
pub fn set(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>; // Facade shouldn't expose run
```

### Rule 5: Wire Encoding Preserves Types

> **Wire encoding MUST preserve the distinction between Value types. Round-trip must be lossless.**

```rust
// CORRECT: Bytes uses wrapper
let v = Value::Bytes(vec![1, 2, 3]);
let json = encode_json(&v); // {"$bytes":"AQID"}
let decoded = decode_json(&json)?;
assert_eq!(v, decoded);

// CORRECT: Special floats use wrapper
let v = Value::Float(f64::NAN);
let json = encode_json(&v); // {"$f64":"NaN"}

// WRONG: Bytes as plain array
let json = "[1, 2, 3]"; // Ambiguous - is this Bytes or Array of Ints?
```

### Rule 6: Errors Are Explicit

> **All invalid inputs MUST produce explicit errors. No silent failures or best-effort handling.**

```rust
// CORRECT: Explicit error
if key.is_empty() {
    return Err(StrataError::InvalidKey {
        key: key.to_string(),
        reason: "key cannot be empty".to_string(),
    });
}

// WRONG: Silent truncation
let key = &key[..MAX_KEY_LEN.min(key.len())]; // NEVER silently truncate
```

### Rule 7: Contract Stability

> **Frozen elements MUST NOT change without major version bump.**

Frozen elements include:
- Operation names (e.g., `kv.set`, `json.get`)
- Parameter shapes
- Return shapes
- Error codes (all 12 codes)
- Wire encoding wrappers ($bytes, $f64, $absent)
- Value type names

### Rule 8: Default Run Is Literal "default"

> **The default run has the canonical name `"default"` (literal string, not UUID). It always exists.**

```rust
// CORRECT: Default run is literal "default"
pub const DEFAULT_RUN_ID: &str = "default";

// WRONG: UUID for default run
pub const DEFAULT_RUN_ID: Uuid = Uuid::nil(); // Never use UUID for default
```

---

## CORE INVARIANTS

### Value Model Invariants (VAL-1 to VAL-5)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| VAL-1 | Eight types only: Null, Bool, Int, Float, String, Bytes, Array, Object | Type exhaustiveness tests |
| VAL-2 | No implicit type coercions | Cross-type comparison tests |
| VAL-3 | `Int(1)` != `Float(1.0)` | Explicit inequality tests |
| VAL-4 | `Bytes` are not `String` | Type distinction tests |
| VAL-5 | Float uses IEEE-754 equality | NaN, -0.0 equality tests |

### Facade Invariants (FAC-1 to FAC-5)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| FAC-1 | Every facade op maps to deterministic substrate ops | Desugaring unit tests |
| FAC-2 | Facade adds no semantic behavior beyond defaults | Parity tests facade vs substrate |
| FAC-3 | Facade never swallows substrate errors | Error propagation tests |
| FAC-4 | Facade does not reorder operations | Ordering verification tests |
| FAC-5 | All behavior traces to explicit substrate operation | Audit all code paths |

### Wire Encoding Invariants (WIRE-1 to WIRE-5)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| WIRE-1 | JSON encoding is mandatory | Encoding availability tests |
| WIRE-2 | Bytes encode as `{"$bytes": "<base64>"}` | Bytes round-trip tests |
| WIRE-3 | Non-finite floats encode as `{"$f64": "..."}` | Float special value tests |
| WIRE-4 | Absent values encode as `{"$absent": true}` | CAS absent value tests |
| WIRE-5 | Round-trip preserves exact type and value | Full round-trip suite |

### Error Invariants (ERR-1 to ERR-4)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| ERR-1 | All errors surface through structured error model | Error shape validation |
| ERR-2 | All errors include code, message, details | Error completeness tests |
| ERR-3 | No operation has undefined behavior | Exhaustive edge case tests |
| ERR-4 | `Conflict` = temporal; `ConstraintViolation` = structural | Error categorization tests |

---

## PHASED IMPLEMENTATION STRATEGY (NON-NEGOTIABLE)

> **Stabilize the data model first. Wire encoding must work before APIs. APIs must work before CLI/SDK.**

M11 uses a phased approach where each phase produces a testable, validated increment:

### M11a Phases (Core Contract)

#### Phase 1: Data Model Foundation (Epics 80, 83)
- Value enum finalization
- Float edge case handling
- Wire encoding with special wrappers

**Exit Criteria**: All 8 value types encode/decode correctly. Round-trip tests pass.

#### Phase 2: Error Model (Epic 84)
- Error code enumeration
- Wire error shape
- ConstraintViolation reasons

**Exit Criteria**: All error conditions produce correct structured errors.

#### Phase 3: API Layers (Epics 81, 82)
- Facade API with all operations
- Substrate API with explicit parameters
- Facade→Substrate desugaring verified

**Exit Criteria**: Both API layers complete. Parity tests pass.

#### Phase 4: Core Validation (Epic 87a) - M11a Exit Gate
- Value model tests (100% coverage)
- Wire encoding round-trip tests
- Facade-Substrate parity tests

**Exit Criteria**: All M11a contract guarantees validated. Zero defects in core contract.

### M11b Phases (Consumer Surfaces)

#### Phase 5: CLI + SDK (Epics 85, 86)
- CLI with Redis-like ergonomics
- Rust SDK implementation

**Exit Criteria**: CLI works for all facade operations. SDK conformance harness passes.

#### Phase 6: Surface Validation (Epic 87b) - M11b Exit Gate
- CLI argument parsing tests
- CLI output formatting tests
- SDK conformance tests

**Exit Criteria**: All M11b contract guarantees validated.

---

## TDD METHODOLOGY

**CRITICAL TESTING RULE** (applies to EVERY story):

- **NEVER adjust tests to make them pass**
- If a test fails, the CODE must be fixed, not the test
- Tests define correct behavior - failed tests reveal bugs in implementation
- Only adjust a test if the test itself is incorrect (wrong assertion logic)
- Tests MUST validate spec-compliant behavior

### The TDD Cycle

1. **Write the test** - Define expected behavior before writing any implementation
2. **Run the test** - Verify it fails (red)
3. **Write minimal implementation** - Just enough to pass the test
4. **Run the test** - Verify it passes (green)
5. **Refactor** - Clean up while keeping tests green

---

## Tool Paths

**ALWAYS use fully qualified paths:**
- Cargo: `~/.cargo/bin/cargo`
- GitHub CLI: `gh` (should be in PATH)

---

## Story Workflow

1. **Start story**: `./scripts/start-story.sh <epic> <story> <description>`
2. **Read specs**:
   ```bash
   cat docs/milestones/M11/M11_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M11/EPIC_<N>_*.md
   ```
3. **Write tests first** (TDD)
4. **Implement code** to pass tests
5. **Run validation**:
   ```bash
   ~/.cargo/bin/cargo test --workspace
   ~/.cargo/bin/cargo clippy --workspace -- -D warnings
   ~/.cargo/bin/cargo fmt --check
   ```
6. **Complete story**: `./scripts/complete-story.sh <story>`

---

## GitHub Issue References

M11 uses the following GitHub issue numbers:

### M11a Epics (Core Contract)

| Epic | GitHub Issue | Stories (GitHub Issues) |
|------|--------------|-------------------------|
| Epic 80: Value Model | [#549](https://github.com/anibjoshi/in-mem/issues/549) | #550-#555 |
| Epic 81: Facade API | [#556](https://github.com/anibjoshi/in-mem/issues/556) | #557-#564 |
| Epic 82: Substrate API | [#565](https://github.com/anibjoshi/in-mem/issues/565) | #566-#572 |
| Epic 83: Wire Encoding | [#573](https://github.com/anibjoshi/in-mem/issues/573) | #574-#579 |
| Epic 84: Error Model | [#580](https://github.com/anibjoshi/in-mem/issues/580) | #581-#585 |
| Epic 87a: Core Validation | [#586](https://github.com/anibjoshi/in-mem/issues/586) | #587-#590 |

### M11b Epics (Consumer Surfaces)

| Epic | GitHub Issue | Stories (GitHub Issues) |
|------|--------------|-------------------------|
| Epic 85: CLI | TBD | TBD |
| Epic 86: SDK Foundation | TBD | TBD |
| Epic 87b: Surface Validation | TBD | TBD |

---

## EPIC END VALIDATION

**At the end of every epic, run the full validation process.**

### Quick Validation Commands

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
```

### Eight Rules Quick Check

```bash
# Rule 1: Facade desugars to substrate
echo "Rule 1: Facade desugars to substrate"
grep -r "fn.*default.*run" crates/api/src/facade/ && echo "CHECK" || echo "MISSING"

# Rule 2: No hidden errors
echo "Rule 2: No hidden errors"
grep -r "\.ok()\." crates/api/src/facade/ && echo "FAIL: Errors swallowed" || echo "PASS"

# Rule 3: No type coercion
echo "Rule 3: No type coercion"
grep -r "as f64\|as i64" crates/core/src/value.rs && echo "CHECK: Verify not in PartialEq" || echo "PASS"

# Rule 4: Explicit run scoping
echo "Rule 4: Explicit run scoping"
grep -r "run_id.*:.*&RunId" crates/api/src/substrate/ && echo "PASS" || echo "CHECK"

# Rule 5: Wire encoding preserves types
echo "Rule 5: Wire encoding preserves types"
grep -r "\$bytes\|\$f64\|\$absent" crates/wire/src/ && echo "PASS: Wrappers exist" || echo "CHECK"

# Rule 6: Errors are explicit
echo "Rule 6: Errors are explicit"
grep -r "return Err\|StrataError::" crates/api/src/ && echo "PASS" || echo "CHECK"

# Rule 8: Default run is literal
echo "Rule 8: Default run is literal 'default'"
grep -r 'DEFAULT_RUN.*=.*"default"' crates/ && echo "PASS" || echo "FAIL"
```

---

## M11 CORE CONCEPTS

### What M11 Is About

M11 is a **contract milestone**. It freezes the public API surface:

| Aspect | M11 Commits To |
|--------|----------------|
| **Value Model** | 8 types, no coercion, IEEE-754 floats |
| **Wire Encoding** | JSON with $bytes, $f64, $absent wrappers |
| **Facade API** | Redis-like surface targeting default run |
| **Substrate API** | Power-user surface with explicit run/version |
| **Error Model** | 12 error codes, structured payloads |
| **CLI** | All facade operations with frozen parsing |
| **SDK** | Rust SDK, Python/JS mappings |

### What M11 Is NOT

M11 is **not** a feature milestone. It stabilizes existing functionality.

| Deferred Item | Target |
|---------------|--------|
| Python SDK implementation | M14 |
| JavaScript SDK implementation | Post-MVP |
| MessagePack encoding | Post-MVP |
| JSONPath advanced features | Post-MVP |
| TTL/EXPIRE semantics | Post-MVP |
| Consumer groups for events | Post-MVP |

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Two-layer API | Facade = convenience, Substrate = power |
| No type coercion | Explicit > implicit, prevent subtle bugs |
| Wire wrappers | JSON lacks Bytes, non-finite floats |
| 12 error codes | Frozen, exhaustive, actionable |
| Default run = "default" | Simple, predictable, always exists |

---

## Directory Structure

```
crates/api/src/
├── facade/                 # Redis-like API (targets default run)
│   ├── mod.rs
│   ├── kv.rs              # set, get, mget, mset, delete, exists, incr
│   ├── json.rs            # json_set, json_get, json_del, json_merge
│   ├── event.rs           # xadd, xrange
│   ├── vector.rs          # vset, vget, vdel
│   ├── state.rs           # cas_set, cas_get
│   └── history.rs         # history, get_at
└── substrate/             # Power-user API (explicit run/version)
    ├── mod.rs
    ├── kv.rs              # kv_put, kv_get, kv_get_at, kv_history
    ├── json.rs            # json_set, json_get, json_history
    ├── event.rs           # event_append, event_range
    ├── state.rs           # state_get, state_set, state_cas
    ├── vector.rs          # vector_set, vector_get, vector_history
    └── run.rs             # run_create, run_get, run_list, run_close

crates/wire/src/
├── json/                  # JSON wire encoding
│   ├── mod.rs
│   ├── value.rs           # Value JSON encoding
│   ├── wrappers.rs        # $bytes, $f64, $absent
│   ├── envelope.rs        # Request/response envelopes
│   └── version.rs         # Version encoding
└── lib.rs

crates/cli/src/
├── main.rs               # CLI entry point
├── parser.rs             # Argument parsing
├── commands/             # Command implementations
└── output.rs             # Output formatting
```

---

*End of M11 Prompt Header - Epic-specific content follows below*
