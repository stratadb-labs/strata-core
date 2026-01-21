# Epic 81: Facade API Implementation - Implementation Prompts

**Epic Goal**: Implement the Redis-like Facade API that desugars to Substrate operations

**GitHub Issue**: [#556](https://github.com/anibjoshi/in-mem/issues/556)
**Status**: Ready after Epic 80
**Dependencies**: Epic 80 (Value Model)
**Phase**: 3 (API Layers)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "Strata" IS allowed (e.g., `StrataFacade`, `strata_facade`)
>
> **CORRECT**: `//! Strata facade API for Redis-like operations`
> **WRONG**: `//! M11 facade implementation`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

Before starting ANY story in this epic, read:
1. **Contract Spec**: `docs/milestones/M11/M11_CONTRACT.md`
2. **Epic Spec**: `docs/milestones/M11/EPIC_81_FACADE_API.md`
3. **Prompt Header**: `docs/prompts/M11/M11_PROMPT_HEADER.md`

---

## Epic 81 Overview

### Scope
- KV operations (set, get, getv, mget, mset, delete, exists, incr)
- JSON operations (json_set, json_get, json_getv, json_del, json_merge)
- Event operations (xadd, xrange)
- Vector operations (vset, vget, vdel)
- State/CAS operations (cas_set, cas_get)
- History operations (history, get_at)
- Run operations (runs, use_run)
- Capability discovery

### Architectural Invariants (MUST FOLLOW)

1. **Every facade operation desugars to exactly one substrate operation sequence**
2. **Facade adds NO semantic behavior - only defaults**
3. **Facade NEVER swallows substrate errors**
4. **Facade targets the default run (named "default")**
5. **Facade auto-commits each operation**

### Success Criteria
- [ ] All KV operations implemented
- [ ] All JSON operations implemented
- [ ] All Event operations implemented
- [ ] All Vector operations implemented
- [ ] All CAS operations implemented
- [ ] All History operations implemented
- [ ] All operations target default run
- [ ] All errors propagate unchanged

### Component Breakdown
- **Story #557**: KV Operations (set, get, getv, mget, mset, delete, exists, exists_many, incr)
- **Story #558**: JSON Operations (json_set, json_get, json_getv, json_del, json_merge)
- **Story #559**: Event Operations (xadd, xrange)
- **Story #560**: Vector Operations (vset, vget, vdel)
- **Story #561**: State/CAS Operations (cas_set, cas_get)
- **Story #562**: History Operations (history, get_at, latest_version)
- **Story #563**: Run Operations (runs, use_run)
- **Story #564**: Capability Discovery (capabilities)

---

## Desugaring Reference

Every facade operation MUST desugar mechanically to substrate:

| Facade | Substrate |
|--------|-----------|
| `set(key, value)` | `kv_put(default, key, value)` |
| `get(key)` | `kv_get(default, key).map(\|v\| v.value)` |
| `getv(key)` | `kv_get(default, key)` |
| `mget(keys)` | `batch { kv_get(default, k) for k in keys }` |
| `mset(entries)` | `begin(); for (k,v): kv_put(default, k, v); commit()` |
| `delete(keys)` | `begin(); for k: kv_delete(default, k); commit(); count` |
| `exists(key)` | `kv_get(default, key).is_some()` |
| `incr(key, delta)` | `kv_incr(default, key, delta)` |
| `json_set(key, path, value)` | `json_set(default, key, path, value)` |
| `json_get(key, path)` | `json_get(default, key, path).map(\|v\| v.value)` |
| `xadd(stream, payload)` | `event_append(default, stream, payload)` |
| `vset(key, vector, meta)` | `vector_set(default, key, vector, meta)` |
| `vget(key)` | `vector_get(default, key)` |
| `cas_set(key, expected, new)` | `state_cas(default, key, expected, new)` |
| `history(key, limit, before)` | `kv_history(default, key, limit, before)` |

---

## Story #557: KV Operations

**GitHub Issue**: [#557](https://github.com/anibjoshi/in-mem/issues/557)
**Dependencies**: Epic 80
**Blocks**: None

### Start Story

```bash
./scripts/start-story.sh 81 557 kv-operations
```

### Key Implementation Points

```rust
pub struct Facade {
    substrate: Substrate,
    default_run: RunId,
}

impl Facade {
    pub fn set(&self, key: &str, value: Value) -> Result<()> {
        self.substrate.kv_put(&self.default_run, key, value)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Value>> {
        Ok(self.substrate.kv_get(&self.default_run, key)?.map(|v| v.value))
    }

    pub fn getv(&self, key: &str) -> Result<Option<Versioned<Value>>> {
        self.substrate.kv_get(&self.default_run, key)
    }

    pub fn mget(&self, keys: &[&str]) -> Result<Vec<Option<Value>>> {
        keys.iter()
            .map(|k| Ok(self.substrate.kv_get(&self.default_run, k)?.map(|v| v.value)))
            .collect()
    }

    pub fn delete(&self, keys: &[&str]) -> Result<u64> {
        let mut count = 0;
        for key in keys {
            if self.substrate.kv_delete(&self.default_run, key)? {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn incr(&self, key: &str, delta: i64) -> Result<i64> {
        self.substrate.kv_incr(&self.default_run, key, delta)
    }
}
```

### Acceptance Criteria

- [ ] `set(key, value) -> ()` works
- [ ] `get(key) -> Option<Value>` returns value without version
- [ ] `getv(key) -> Option<Versioned<Value>>` returns full versioned
- [ ] `mget` returns Vec in same order as input
- [ ] `mset` is atomic
- [ ] `delete` returns count of keys that existed
- [ ] `incr` is atomic and returns new value
- [ ] All operations target default run

### Complete Story

```bash
./scripts/complete-story.sh 557
```

---

## Story #558: JSON Operations

**GitHub Issue**: [#558](https://github.com/anibjoshi/in-mem/issues/558)

### Start Story

```bash
./scripts/start-story.sh 81 558 json-operations
```

### Key Implementation Points

```rust
impl Facade {
    pub fn json_set(&self, key: &str, path: &str, value: Value) -> Result<()> {
        self.substrate.json_set(&self.default_run, key, path, value)?;
        Ok(())
    }

    pub fn json_get(&self, key: &str, path: &str) -> Result<Option<Value>> {
        Ok(self.substrate.json_get(&self.default_run, key, path)?.map(|v| v.value))
    }

    pub fn json_getv(&self, key: &str, path: &str) -> Result<Option<Versioned<Value>>> {
        self.substrate.json_get(&self.default_run, key, path)
    }

    pub fn json_del(&self, key: &str, path: &str) -> Result<bool> {
        self.substrate.json_delete(&self.default_run, key, path)
    }

    pub fn json_merge(&self, key: &str, path: &str, value: Value) -> Result<()> {
        self.substrate.json_merge(&self.default_run, key, path, value)?;
        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] `json_set` creates/updates path in document
- [ ] `json_get` returns value at path
- [ ] `json_getv` returns document-level version
- [ ] `json_del` removes path from document
- [ ] `json_merge` deep merges objects
- [ ] Invalid paths return `InvalidPath` error

---

## Story #559: Event Operations

**GitHub Issue**: [#559](https://github.com/anibjoshi/in-mem/issues/559)

### Key Implementation Points

```rust
impl Facade {
    pub fn xadd(&self, stream: &str, payload: Value) -> Result<Version> {
        self.substrate.event_append(&self.default_run, stream, payload)
    }

    pub fn xrange(
        &self,
        stream: &str,
        start: Option<Version>,
        end: Option<Version>,
        limit: Option<usize>,
    ) -> Result<Vec<Versioned<Value>>> {
        self.substrate.event_range(&self.default_run, stream, start, end, limit)
    }
}
```

### Acceptance Criteria

- [ ] `xadd` returns sequence version
- [ ] `xrange` returns events in order
- [ ] Pagination with limit works

---

## Story #560: Vector Operations

**GitHub Issue**: [#560](https://github.com/anibjoshi/in-mem/issues/560)

### Key Implementation Points

```rust
impl Facade {
    pub fn vset(&self, key: &str, vector: Vec<f32>, metadata: Value) -> Result<()> {
        self.substrate.vector_set(&self.default_run, key, vector, metadata)?;
        Ok(())
    }

    pub fn vget(&self, key: &str) -> Result<Option<Versioned<VectorEntry>>> {
        self.substrate.vector_get(&self.default_run, key)
    }

    pub fn vdel(&self, key: &str) -> Result<bool> {
        self.substrate.vector_delete(&self.default_run, key)
    }
}

pub struct VectorEntry {
    pub vector: Vec<f32>,
    pub metadata: Value,
}
```

### Acceptance Criteria

- [ ] `vset` stores vector with metadata
- [ ] `vget` returns versioned vector+metadata
- [ ] `vdel` returns true if existed

---

## Story #561: State/CAS Operations

**GitHub Issue**: [#561](https://github.com/anibjoshi/in-mem/issues/561)

### Key Implementation Points

```rust
impl Facade {
    /// Compare-and-swap: set new value only if current matches expected
    /// Use Value::Absent for "expected not to exist"
    pub fn cas_set(&self, key: &str, expected: Value, new: Value) -> Result<bool> {
        self.substrate.state_cas(&self.default_run, key, expected, new)
    }

    pub fn cas_get(&self, key: &str) -> Result<Option<Value>> {
        Ok(self.substrate.state_get(&self.default_run, key)?.map(|v| v.value))
    }
}
```

### Acceptance Criteria

- [ ] `cas_set` with matching expected succeeds
- [ ] `cas_set` with mismatched expected returns false (or Conflict)
- [ ] `cas_set` with `Absent` for create-if-missing
- [ ] Uses structural equality (no type coercion!)

---

## Story #562: History Operations

**GitHub Issue**: [#562](https://github.com/anibjoshi/in-mem/issues/562)

### Key Implementation Points

```rust
impl Facade {
    pub fn history(
        &self,
        key: &str,
        limit: Option<usize>,
        before: Option<Version>,
    ) -> Result<Vec<Versioned<Value>>> {
        self.substrate.kv_history(&self.default_run, key, limit, before)
    }

    pub fn get_at(&self, key: &str, version: Version) -> Result<Option<Versioned<Value>>> {
        self.substrate.kv_get_at(&self.default_run, key, version)
    }
}
```

### Acceptance Criteria

- [ ] `history` returns versions newest-first
- [ ] `limit` restricts count
- [ ] `before` enables pagination
- [ ] `get_at` returns specific version
- [ ] Trimmed versions return `HistoryTrimmed` error

---

## Epic 81 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test facade_ -- --nocapture
~/.cargo/bin/cargo test --test m11_comprehensive facade_api
```

### 2. Verify Desugaring Parity

```bash
# Every facade op should produce same result as substrate equivalent
~/.cargo/bin/cargo test facade_substrate_parity_
```

### 3. Verify Error Propagation

```bash
# Errors should propagate unchanged
~/.cargo/bin/cargo test facade_error_propagation_
```

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-81-facade-api -m "Epic 81: Facade API Implementation complete

Delivered:
- KV operations (set, get, mget, mset, delete, exists, incr)
- JSON operations (json_set, json_get, json_del, json_merge)
- Event operations (xadd, xrange)
- Vector operations (vset, vget, vdel)
- CAS operations (cas_set, cas_get)
- History operations (history, get_at)
- All operations target default run
- All operations auto-commit

Stories: #557, #558, #559, #560, #561, #562, #563, #564
"
git push origin develop
gh issue close 556 --comment "Epic 81: Facade API Implementation - COMPLETE"
```

---

## Summary

Epic 81 establishes the FACADE API:

- **Redis-like interface**: Familiar patterns
- **Default run targeting**: No explicit run needed
- **Auto-commit**: Each operation is atomic
- **Mechanical desugaring**: No hidden behavior
- **Error transparency**: All errors propagate
