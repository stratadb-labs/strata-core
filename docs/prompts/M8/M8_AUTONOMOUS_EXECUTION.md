# M8 Autonomous Execution Guide

This document provides a streamlined workflow for autonomous execution of M8 (Vector Primitive) implementation.

---

## Quick Reference

### GitHub Issues

| Epic | Issue | Stories |
|------|-------|---------|
| Epic 50: Core Types | #388 | #394-#398 |
| Epic 51: Vector Heap | #389 | #399-#404 |
| Epic 52: Index Backend | #390 | #405-#409 |
| Epic 53: Collection Management | #391 | #410-#414 |
| Epic 54: Search Integration | #392 | #415-#420 |
| Epic 55: Transaction & Durability | #393 | #421-#425 |

### Critical Rules (Memorize These)

1. **BTreeMap, not HashMap** - Deterministic iteration
2. **VectorId never reused** - Storage slots can be reused
3. **next_id in snapshots** - CRITICAL for T4 invariant
4. **Higher is better** - All metrics normalized
5. **Search is read-only** - No WAL, no counters, no side effects
6. **Upsert semantics** - Insert overwrites existing

---

## Phase 1: Foundation (Epic 50)

### Start Epic 50

```bash
# Create epic branch
git checkout develop
git pull origin develop
git checkout -b epic-50-core-types
git push -u origin epic-50-core-types
```

### Story Execution Order

```bash
# Story #398: VectorError (no deps, others depend on it)
./scripts/start-story.sh 50 398 vector-error
# Implement error types
~/.cargo/bin/cargo test vector::error
./scripts/complete-story.sh 398

# Story #394: VectorConfig
./scripts/start-story.sh 50 394 vector-config
# Implement VectorConfig
~/.cargo/bin/cargo test vector::types
./scripts/complete-story.sh 394

# Story #395: DistanceMetric
./scripts/start-story.sh 50 395 distance-metric
# Implement DistanceMetric
~/.cargo/bin/cargo test vector::types
./scripts/complete-story.sh 395

# Story #396: VectorEntry/Match
./scripts/start-story.sh 50 396 vector-entry-match
# Implement VectorEntry, VectorMatch, VectorId
~/.cargo/bin/cargo test vector::types
./scripts/complete-story.sh 396

# Story #397: MetadataFilter
./scripts/start-story.sh 50 397 metadata-filter
# Implement MetadataFilter, JsonScalar
~/.cargo/bin/cargo test vector::filter
./scripts/complete-story.sh 397
```

### Complete Epic 50

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
git checkout develop
git merge --no-ff epic-50-core-types -m "Epic 50: Core Types complete"
git push origin develop
gh issue close 388 --comment "Epic 50 complete"
```

---

## Phase 2: Storage (Epics 51 & 52 - Parallel)

### Claude 1: Epic 51

```bash
git checkout develop && git pull
git checkout -b epic-51-vector-heap
git push -u origin epic-51-vector-heap

# Stories in order: #404 -> #399 -> #400 -> #401 -> #402 -> #403
```

### Claude 2: Epic 52

```bash
git checkout develop && git pull
git checkout -b epic-52-index-backend
git push -u origin epic-52-index-backend

# Stories in order: #405 -> #407 -> #406 -> #408 -> #409
```

---

## Phase 3: Management & Search (Epics 53 & 54 - Parallel)

### Claude 1: Epic 53 (after Epic 51)

```bash
git checkout develop && git pull
git checkout -b epic-53-collection-management
git push -u origin epic-53-collection-management

# Stories: #410 -> #411 -> #412 -> #413 -> #414
```

### Claude 2: Epic 54 (after Epic 52)

```bash
git checkout develop && git pull
git checkout -b epic-54-search-integration
git push -u origin epic-54-search-integration

# Stories: #418 -> #415 -> #416 -> #417 -> #419 -> #420
```

---

## Phase 4: Durability (Epic 55)

```bash
git checkout develop && git pull
git checkout -b epic-55-transaction-durability
git push -u origin epic-55-transaction-durability

# Stories: #421 -> #422 -> #423 -> #424 -> #425
```

---

## Standard Story Workflow

```bash
# 1. Start
./scripts/start-story.sh <epic> <story> <description>

# 2. Read specs
cat docs/milestones/M8/EPIC_<N>_<NAME>.md
cat docs/prompts/M8/epic-<N>-claude-prompts.md

# 3. Write tests first
# 4. Implement to pass tests

# 5. Validate
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check

# 6. Complete
./scripts/complete-story.sh <story>
```

---

## Validation Commands

### Quick Check

```bash
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check
```

### M8-Specific Tests

```bash
# Vector storage
~/.cargo/bin/cargo test vector::heap
~/.cargo/bin/cargo test vector::backend
~/.cargo/bin/cargo test vector::brute_force

# Vector search
~/.cargo/bin/cargo test vector::store
~/.cargo/bin/cargo test search::fusion

# Durability
~/.cargo/bin/cargo test vector_wal
~/.cargo/bin/cargo test vector_snapshot
~/.cargo/bin/cargo test vector_recovery
```

### Invariant Tests

```bash
# S4: VectorId never reused
~/.cargo/bin/cargo test vector_id_never_reused

# T4: VectorId monotonicity across crashes
~/.cargo/bin/cargo test vector_id_monotonicity

# R10: Search is read-only
~/.cargo/bin/cargo test search_is_read_only

# S8: Snapshot-WAL equivalence
~/.cargo/bin/cargo test snapshot_wal_equivalence
```

---

## Common Patterns

### VectorHeap with BTreeMap

```rust
// CORRECT
pub struct VectorHeap {
    id_to_offset: BTreeMap<VectorId, usize>,  // Deterministic!
}

// WRONG
pub struct VectorHeap {
    id_to_offset: HashMap<VectorId, usize>,  // Nondeterministic!
}
```

### VectorId Never Reused

```rust
// CORRECT: Monotonic IDs, reusable slots
fn insert(&mut self, embedding: &[f32]) -> VectorId {
    let id = VectorId(self.next_id.fetch_add(1, Ordering::SeqCst));
    let offset = self.free_slots.pop().unwrap_or_else(|| self.allocate());
    self.id_to_offset.insert(id, offset);
    id
}

fn delete(&mut self, id: VectorId) {
    if let Some(offset) = self.id_to_offset.remove(&id) {
        self.free_slots.push(offset);  // Recycle SLOT, not ID
    }
}

// WRONG: Reusing IDs
fn delete(&mut self, id: VectorId) {
    self.free_ids.push(id);  // NEVER DO THIS
}
```

### Score Normalization

```rust
// ALL metrics: higher = more similar
match metric {
    Cosine => dot(a,b) / (||a|| * ||b||),       // [-1, 1]
    Euclidean => 1.0 / (1.0 + distance(a,b)),  // (0, 1]
    DotProduct => dot(a,b),                     // unbounded
}
```

### Deterministic Ordering

```rust
// Backend: (score desc, VectorId asc)
results.sort_by(|a, b| {
    b.score.partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.id.cmp(&b.id))  // VectorId tie-break
});

// Facade: (score desc, key asc)
matches.sort_by(|a, b| {
    b.score.partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.key.cmp(&b.key))  // Key tie-break
});
```

### Snapshot Critical Fields

```rust
// CRITICAL: These MUST be in snapshots
pub struct VectorCollectionSnapshot {
    pub next_id: u64,           // T4 invariant!
    pub free_slots: Vec<usize>, // Correct slot reuse!
    // ...
}
```

---

## Troubleshooting

### "Dimension mismatch" Error

Check that embedding dimension matches collection config.

### Nondeterministic Search Results

Verify using BTreeMap, not HashMap.

### VectorId Reuse After Recovery

Verify next_id is being persisted and restored from snapshots.

### Search Modifying State

Search must be read-only. Check for:
- WAL writes
- Counter increments
- Cache mutations

---

## Timeline Estimate

| Phase | Epics | Estimated Days |
|-------|-------|----------------|
| Foundation | Epic 50 | 1-2 |
| Storage | Epic 51, 52 (parallel) | 2-3 |
| Management & Search | Epic 53, 54 (parallel) | 2-3 |
| Durability | Epic 55 | 2-3 |
| Validation | All | 2-3 |

**Total: ~10-14 days** with 2 parallel Claudes

---

## Final Checklist

Before declaring M8 complete:

- [ ] All 32 stories merged to develop
- [ ] All tests passing
- [ ] Clippy clean (no warnings)
- [ ] Format check passing
- [ ] Invariant tests: S4, S7, S8, S9, R2, R4, R5, R10, T4
- [ ] Performance baselines documented
- [ ] No regressions in M7 tests
- [ ] develop merged to main with tag

```bash
git checkout main
git merge --no-ff develop -m "M8: Vector Primitive complete"
git tag -a v0.8.0 -m "Milestone 8: Vector Primitive"
git push origin main --tags
```

---

*End of M8 Autonomous Execution Guide*
