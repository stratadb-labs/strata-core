# Epic 9: Recovery Support - Implementation Prompts

**Epic Goal**: Implement transaction-aware recovery that replays WAL to restore database state after crash.

**Status**: Ready to begin
**Dependencies**: Epic 8 (Durability & Commit) complete

---

## 🔴 AUTHORITATIVE SPECIFICATION - READ THIS FIRST

**`docs/architecture/M2_TRANSACTION_SEMANTICS.md` is the GOSPEL for ALL M2 implementation.**

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M2:

1. **Every story MUST implement behavior EXACTLY as specified in the semantics document**
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

### What the Spec Defines (Read Before Any M2 Work):

| Section | Content | You MUST Follow |
|---------|---------|-----------------|
| Section 1 | Isolation Level | **Snapshot Isolation, NOT Serializability** |
| Section 2 | Visibility Rules | What txns see/don't see/may see |
| Section 3 | Conflict Detection | When aborts happen, first-committer-wins |
| Section 4 | Implicit Transactions | How M1-style ops work in M2 |
| Section 5 | Replay Semantics | **No re-validation, single-threaded, version preservation** |
| Section 6 | Version Semantics | Version 0 = never existed, tombstones |

### Before Starting ANY Story:

```bash
# 1. Read the full spec
cat docs/architecture/M2_TRANSACTION_SEMANTICS.md

# 2. Identify which sections apply to your story
# 3. Understand the EXACT behavior required
# 4. Implement EXACTLY that behavior
# 5. Write tests that validate spec compliance
```

**WARNING**: Code review will verify spec compliance. Non-compliant code will be rejected.

---

## 🔴 BRANCHING STRATEGY - READ THIS

### Branch Hierarchy
```
main                          ← Protected: only accepts merges from develop
  └── develop                 ← Integration branch for completed epics
       └── epic-9-recovery    ← Epic branch (base for all story PRs)
            └── epic-9-story-93-*    ← Story branches
```

### Critical Rules

1. **Story PRs go to EPIC branch, NOT main**
   ```bash
   # CORRECT: PR base is epic branch
   /opt/homebrew/bin/gh pr create --base epic-9-recovery --head epic-9-story-93-infrastructure

   # WRONG: Never PR directly to main
   /opt/homebrew/bin/gh pr create --base main --head epic-9-story-93-infrastructure  # ❌ NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-9-recovery
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M2: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 93  # Creates PR to epic-9-recovery
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## 🔴 CRITICAL TESTING RULE

**NEVER adjust tests to make them pass**

- If a test fails, the CODE must be fixed, not the test
- Tests define correct behavior - failed tests reveal bugs in implementation
- Only adjust a test if the test itself is incorrect (wrong assertion logic)
- Tests MUST validate spec-compliant behavior

---

## 🔴 TDD METHODOLOGY

For each story:

1. **Write tests FIRST** that validate spec-compliant behavior
2. **Run tests** - they should FAIL (no implementation yet)
3. **Implement code** to make tests pass
4. **Refactor** if needed while keeping tests green
5. **Run full validation** before completing story

---

## Tool Paths

Use fully qualified paths:
- Cargo: `~/.cargo/bin/cargo`
- GitHub CLI: `/opt/homebrew/bin/gh`

---

## Epic 9 Overview

### Scope
- Transaction-aware WAL replay
- Recovery from crash scenarios
- Version preservation during replay
- Incomplete transaction handling
- Integration with TransactionManager

### Key Spec References (Section 5: Replay Semantics)

| Rule | Description |
|------|-------------|
| **No re-validation** | WAL contains only committed transactions. Replay applies writes directly. |
| **Single-threaded** | Replay processes entries in WAL order. No concurrency. |
| **Version preservation** | Versions from WAL are preserved exactly (use `put_with_version`). |
| **Incomplete = discard** | Transactions without CommitTxn are discarded. |
| **Deterministic** | Same WAL always produces identical state. |

### Recovery Algorithm (Spec Section 5.4)

```
RECOVERY PROCEDURE:

1. Load snapshot (if exists)
   - Provides base state at snapshot_version
   - Skip WAL entries <= snapshot_version

2. Open WAL, scan for entries after snapshot_version
   - Build map: txn_id → [entries]
   - Track which txn_ids have CommitTxn markers

3. Identify incomplete transactions
   - Incomplete = has BeginTxn but no CommitTxn
   - These represent crashed-during-commit

4. For each COMPLETE transaction (has CommitTxn):
   - Apply all Write entries: storage.put_with_version(key, value, version)
   - Apply all Delete entries: storage.delete_with_version(key, version)
   - Update global version counter to commit_version

5. DISCARD incomplete transactions
   - Do not apply their Write/Delete entries
   - They were never committed

6. Database ready for new operations
```

### Success Criteria
- [ ] WAL replay restores exact pre-crash state
- [ ] Incomplete transactions are discarded
- [ ] Versions are preserved exactly from WAL
- [ ] Global version counter is restored correctly
- [ ] TransactionManager initialized with correct version
- [ ] All unit tests pass (>95% coverage)

### Component Breakdown
- **Story #93**: Recovery Infrastructure 🔴 BLOCKS ALL Epic 9
- **Story #94**: WAL Replay Enhancement
- **Story #95**: Transaction Recovery
- **Story #96**: Crash Recovery Testing
- **Story #97**: Recovery Validation

---

## Dependency Graph

```
Phase 1 (Sequential - CRITICAL):
  Story #93 (Recovery Infrastructure)
    └─> 🔴 BLOCKS #94, #95

Phase 2 (Parallel - 2 Claudes after #93):
  Story #94 (WAL Replay Enhancement)
  Story #95 (Transaction Recovery)
    └─> Both depend on #93
    └─> Independent of each other

Phase 3 (Sequential):
  Story #96 (Crash Recovery Testing)
    └─> Depends on #94, #95

Phase 4 (Sequential):
  Story #97 (Recovery Validation)
    └─> Depends on #96
```

---

## Parallelization Strategy

### Optimal Parallel Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 4 hours | #93 Infrastructure | - |
| 2 | 4 hours | #94 WAL Replay | #95 Transaction Recovery |
| 3 | 4 hours | #96 Crash Testing | - |
| 4 | 3 hours | #97 Validation | - |

**Total Wall Time**: ~15 hours (vs. ~18 hours sequential)

---

## Existing Infrastructure

Epic 9 builds on existing recovery code in `crates/durability/src/recovery.rs`:

### Already Implemented
- `replay_wal()` function - basic WAL replay
- `ReplayStats` struct - tracks replay statistics
- `validate_transactions()` - WAL validation before replay
- Transaction grouping by txn_id
- Incomplete transaction detection (missing CommitTxn)
- Aborted transaction detection (AbortTxn entry)
- `put_with_version()` / `delete_with_version()` usage

### What Epic 9 Adds
- Integration with TransactionManager
- Recovery coordinator for database startup
- Version counter restoration
- Crash scenario testing
- End-to-end recovery validation

---

## Story #93: Recovery Infrastructure

**GitHub Issue**: #93
**Estimated Time**: 4 hours
**Dependencies**: Epic 8 complete
**Blocks**: Stories #94, #95, #96, #97

### ⚠️ PREREQUISITE: Read the Semantics Spec

Before writing ANY code, read:
- Section 5: Replay Semantics (ENTIRE section)
- Section 5.4: Recovery Algorithm
- Section 5.5: Incomplete Transaction Handling
- Section 6.1: Global Version Counter

### Semantics This Story Must Implement

From the spec:

| Requirement | Description |
|-------------|-------------|
| **Deterministic replay** | Given the same WAL, replay always produces identical state |
| **Version preservation** | Replay must preserve exact version numbers from WAL |
| **Global version restoration** | After replay, global version = max version from WAL |
| **Single-threaded** | Replay processes entries in order, no concurrency |

### What to Implement

Create `RecoveryCoordinator` in `crates/concurrency/src/recovery.rs`:

```rust
/// Coordinates database recovery after crash or restart
///
/// Per spec Section 5.4:
/// 1. Loads checkpoint (if exists)
/// 2. Replays WAL from checkpoint
/// 3. Discards incomplete transactions
/// 4. Restores global version counter
/// 5. Initializes TransactionManager with correct version
pub struct RecoveryCoordinator {
    /// Path to WAL file
    wal_path: PathBuf,
    /// Path to snapshot directory (optional)
    snapshot_path: Option<PathBuf>,
}

impl RecoveryCoordinator {
    /// Create a new recovery coordinator
    pub fn new(wal_path: PathBuf) -> Self;

    /// Set snapshot path for checkpoint-based recovery
    pub fn with_snapshot_path(self, path: PathBuf) -> Self;

    /// Perform recovery and return initialized components
    ///
    /// Returns:
    /// - UnifiedStore with recovered state
    /// - TransactionManager initialized with correct version
    /// - RecoveryStats with details about recovery
    ///
    /// Per spec Section 5.4: Recovery Procedure
    pub fn recover(&self) -> Result<RecoveryResult>;
}

/// Result of recovery operation
pub struct RecoveryResult {
    /// Recovered storage
    pub storage: UnifiedStore,
    /// Transaction manager initialized with recovered version
    pub txn_manager: TransactionManager,
    /// Statistics about the recovery
    pub stats: RecoveryStats,
}

/// Statistics from recovery
pub struct RecoveryStats {
    /// Number of committed transactions replayed
    pub txns_replayed: usize,
    /// Number of incomplete transactions discarded
    pub incomplete_txns: usize,
    /// Number of aborted transactions discarded
    pub aborted_txns: usize,
    /// Number of writes applied
    pub writes_applied: usize,
    /// Number of deletes applied
    pub deletes_applied: usize,
    /// Final version after recovery
    pub final_version: u64,
    /// Whether recovery was from checkpoint
    pub from_checkpoint: bool,
}
```

### Implementation Steps

#### Step 1: Create the module structure

```bash
# Start the story
./scripts/start-story.sh 9 93 recovery-infrastructure
```

Create `crates/concurrency/src/recovery.rs`:

```rust
//! Recovery infrastructure for transaction-aware database recovery
//!
//! Per spec Section 5 (Replay Semantics):
//! - Replays do NOT re-run conflict detection
//! - Replays apply commit decisions, not re-execute logic
//! - Replays are single-threaded
//! - Versions are preserved exactly
//!
//! ## Recovery Procedure (Section 5.4)
//!
//! 1. Load snapshot (if exists)
//! 2. Open WAL, scan for entries after snapshot_version
//! 3. Build map: txn_id → [entries]
//! 4. Track which txn_ids have CommitTxn markers
//! 5. Apply COMPLETE transactions (has CommitTxn)
//! 6. DISCARD incomplete transactions
//! 7. Initialize TransactionManager with final version

use crate::TransactionManager;
use in_mem_core::error::Result;
use in_mem_durability::recovery::{replay_wal, ReplayStats as DurabilityReplayStats};
use in_mem_durability::wal::{DurabilityMode, WAL};
use in_mem_storage::UnifiedStore;
use std::path::PathBuf;
```

#### Step 2: Write tests FIRST (TDD)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use in_mem_core::types::{Key, Namespace, RunId};
    use in_mem_core::value::Value;
    use in_mem_durability::wal::WALEntry;
    use tempfile::TempDir;

    fn create_test_namespace(run_id: RunId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    fn now() -> i64 {
        Utc::now().timestamp()
    }

    #[test]
    fn test_recovery_empty_wal() {
        // Per spec: Empty WAL = empty database, version 0
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        // Create empty WAL
        let _wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        drop(_wal);

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.final_version, 0);
        assert_eq!(result.txn_manager.current_version(), 0);
    }

    #[test]
    fn test_recovery_committed_transaction() {
        // Per spec Section 5.4: COMPLETE transactions are applied
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("committed.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Write committed transaction to WAL
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::I64(42),
                version: 100,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id,
            }).unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Transaction should be replayed
        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.writes_applied, 1);
        assert_eq!(result.stats.final_version, 100);

        // TransactionManager should have correct version
        assert_eq!(result.txn_manager.current_version(), 100);

        // Storage should have the key with preserved version
        use in_mem_core::traits::Storage;
        let stored = result.storage.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
        assert_eq!(stored.version, 100); // Version preserved exactly
    }

    #[test]
    fn test_recovery_discards_incomplete_transaction() {
        // Per spec Section 5.5: Incomplete = has BeginTxn but no CommitTxn
        // These are DISCARDED, not applied
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("incomplete.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "crash_key");

        // Write incomplete transaction (crash scenario)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("should_not_exist".to_string()),
                version: 50,
            }).unwrap();
            // NO CommitTxn - simulates crash during commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Transaction should be discarded
        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);
        assert_eq!(result.stats.writes_applied, 0);

        // Storage should NOT have the key
        use in_mem_core::traits::Storage;
        assert!(result.storage.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_recovery_version_preservation() {
        // Per spec Section 5.3 Rule 4: Versions are preserved exactly
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("versions.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Write with non-sequential versions (like real usage)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Transaction 1: version 100
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::I64(1),
                version: 100,
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::I64(2),
                version: 100, // Same version in one txn
            }).unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id,
            }).unwrap();

            // Transaction 2: version 200
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::I64(3),
                version: 200,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 2,
                run_id,
            }).unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Final version should be max from WAL
        assert_eq!(result.stats.final_version, 200);
        assert_eq!(result.txn_manager.current_version(), 200);

        // Verify each key has correct version
        use in_mem_core::traits::Storage;

        let key1 = Key::new_kv(ns.clone(), "key1");
        assert_eq!(result.storage.get(&key1).unwrap().unwrap().version, 100);

        let key2 = Key::new_kv(ns.clone(), "key2");
        assert_eq!(result.storage.get(&key2).unwrap().unwrap().version, 100);

        let key3 = Key::new_kv(ns.clone(), "key3");
        assert_eq!(result.storage.get(&key3).unwrap().unwrap().version, 200);
    }

    #[test]
    fn test_recovery_determinism() {
        // Per spec Section 5.6: replay(W) at T1 == replay(W) at T2
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("determinism.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Create WAL with some transactions
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            for i in 1..=5u64 {
                wal.append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                }).unwrap();
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), &format!("key{}", i)),
                    value: Value::I64(i as i64 * 10),
                    version: i * 100,
                }).unwrap();
                wal.append(&WALEntry::CommitTxn {
                    txn_id: i,
                    run_id,
                }).unwrap();
            }
        }

        // Recover twice
        let coordinator = RecoveryCoordinator::new(wal_path.clone());
        let result1 = coordinator.recover().unwrap();

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result2 = coordinator.recover().unwrap();

        // Results must be identical
        assert_eq!(result1.stats.final_version, result2.stats.final_version);
        assert_eq!(result1.stats.txns_replayed, result2.stats.txns_replayed);
        assert_eq!(result1.stats.writes_applied, result2.stats.writes_applied);

        // Verify storage state is identical
        use in_mem_core::traits::Storage;
        for i in 1..=5u64 {
            let key = Key::new_kv(ns.clone(), &format!("key{}", i));
            let v1 = result1.storage.get(&key).unwrap().unwrap();
            let v2 = result2.storage.get(&key).unwrap().unwrap();
            assert_eq!(v1.value, v2.value);
            assert_eq!(v1.version, v2.version);
        }
    }

    #[test]
    fn test_recovery_mixed_transactions() {
        // Mix of committed, incomplete, and aborted transactions
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("mixed.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1: Committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed"),
                value: Value::String("yes".to_string()),
                version: 10,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id,
            }).unwrap();

            // Txn 2: Incomplete (crash)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete"),
                value: Value::String("no".to_string()),
                version: 20,
            }).unwrap();
            // NO CommitTxn

            // Txn 3: Aborted
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "aborted"),
                value: Value::String("no".to_string()),
                version: 30,
            }).unwrap();
            wal.append(&WALEntry::AbortTxn {
                txn_id: 3,
                run_id,
            }).unwrap();

            // Txn 4: Committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 4,
                run_id,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "also_committed"),
                value: Value::String("yes".to_string()),
                version: 40,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn {
                txn_id: 4,
                run_id,
            }).unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 2); // Txn 1 and 4
        assert_eq!(result.stats.incomplete_txns, 1); // Txn 2
        assert_eq!(result.stats.aborted_txns, 1); // Txn 3
        assert_eq!(result.stats.final_version, 40);

        // Only committed keys exist
        use in_mem_core::traits::Storage;
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "committed")).unwrap().is_some());
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "also_committed")).unwrap().is_some());
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "incomplete")).unwrap().is_none());
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "aborted")).unwrap().is_none());
    }
}
```

#### Step 3: Implement RecoveryCoordinator

```rust
/// Coordinates database recovery after crash or restart
///
/// Per spec Section 5.4:
/// 1. Loads checkpoint (if exists) - not implemented in M2
/// 2. Replays WAL from beginning
/// 3. Discards incomplete transactions
/// 4. Restores global version counter
/// 5. Initializes TransactionManager with final version
pub struct RecoveryCoordinator {
    wal_path: PathBuf,
    snapshot_path: Option<PathBuf>,
}

impl RecoveryCoordinator {
    /// Create a new recovery coordinator
    pub fn new(wal_path: PathBuf) -> Self {
        RecoveryCoordinator {
            wal_path,
            snapshot_path: None,
        }
    }

    /// Set snapshot path for checkpoint-based recovery (M3+ feature)
    pub fn with_snapshot_path(mut self, path: PathBuf) -> Self {
        self.snapshot_path = Some(path);
        self
    }

    /// Perform recovery and return initialized components
    ///
    /// Per spec Section 5.4: Recovery Procedure
    ///
    /// # Returns
    /// - RecoveryResult containing storage, transaction manager, and stats
    ///
    /// # Errors
    /// - If WAL cannot be opened or read
    /// - If replay fails
    pub fn recover(&self) -> Result<RecoveryResult> {
        // 1. Open WAL
        let wal = WAL::open(&self.wal_path, DurabilityMode::Strict)?;

        // 2. Create empty storage
        let storage = UnifiedStore::new();

        // 3. Replay WAL using existing durability layer function
        let durability_stats = replay_wal(&wal, &storage)?;

        // 4. Create TransactionManager with recovered version
        let txn_manager = TransactionManager::new(durability_stats.final_version);

        // 5. Convert stats
        let stats = RecoveryStats {
            txns_replayed: durability_stats.txns_applied,
            incomplete_txns: durability_stats.incomplete_txns,
            aborted_txns: durability_stats.aborted_txns,
            writes_applied: durability_stats.writes_applied,
            deletes_applied: durability_stats.deletes_applied,
            final_version: durability_stats.final_version,
            from_checkpoint: false, // Checkpoint not implemented in M2
        };

        Ok(RecoveryResult {
            storage,
            txn_manager,
            stats,
        })
    }
}

/// Result of recovery operation
pub struct RecoveryResult {
    /// Recovered storage
    pub storage: UnifiedStore,
    /// Transaction manager initialized with recovered version
    pub txn_manager: TransactionManager,
    /// Statistics about the recovery
    pub stats: RecoveryStats,
}

/// Statistics from recovery
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecoveryStats {
    /// Number of committed transactions replayed
    pub txns_replayed: usize,
    /// Number of incomplete transactions discarded
    pub incomplete_txns: usize,
    /// Number of aborted transactions discarded
    pub aborted_txns: usize,
    /// Number of writes applied
    pub writes_applied: usize,
    /// Number of deletes applied
    pub deletes_applied: usize,
    /// Final version after recovery
    pub final_version: u64,
    /// Whether recovery was from checkpoint
    pub from_checkpoint: bool,
}
```

#### Step 4: Update lib.rs exports

Add to `crates/concurrency/src/lib.rs`:

```rust
pub mod recovery;

pub use recovery::{RecoveryCoordinator, RecoveryResult, RecoveryStats};
```

#### Step 5: Run validation

```bash
~/.cargo/bin/cargo test -p in-mem-concurrency
~/.cargo/bin/cargo clippy -p in-mem-concurrency -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Acceptance Criteria

- [ ] RecoveryCoordinator struct implemented
- [ ] recover() method returns storage + TransactionManager + stats
- [ ] TransactionManager initialized with correct version from WAL
- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Formatted correctly

### Complete the Story

```bash
./scripts/complete-story.sh 93
```

---

## Story #94: WAL Replay Enhancement

**GitHub Issue**: #94
**Estimated Time**: 3 hours
**Dependencies**: Story #93
**Blocks**: Story #96

### ⚠️ PREREQUISITE: Read the Semantics Spec

Before writing ANY code, read:
- Section 5.2: Replay Rules (all 4 rules)
- Section 5.6: Replay Determinism

### Semantics This Story Must Implement

| Rule | Description |
|------|-------------|
| **Rule 1** | Replays do NOT re-run conflict detection |
| **Rule 2** | Replays apply commit decisions, not re-execute logic |
| **Rule 3** | Replays are single-threaded |
| **Rule 4** | Versions are preserved exactly |

### What to Implement

Enhance `crates/durability/src/recovery.rs` with additional capabilities:

1. **Filtered replay by run_id**
2. **Point-in-time recovery** (replay up to specific version)
3. **Progress callbacks** for monitoring

```rust
/// Options for WAL replay
#[derive(Debug, Default, Clone)]
pub struct ReplayOptions {
    /// Only replay transactions for this run_id (None = all)
    pub filter_run_id: Option<RunId>,
    /// Stop replay at this version (None = replay all)
    pub stop_at_version: Option<u64>,
    /// Callback for progress reporting (called after each transaction)
    pub progress_callback: Option<fn(ReplayProgress)>,
}

/// Progress information during replay
#[derive(Debug, Clone)]
pub struct ReplayProgress {
    /// Current transaction being applied
    pub current_txn: u64,
    /// Total transactions found so far
    pub total_txns: usize,
    /// Current version
    pub current_version: u64,
}

/// Replay WAL with options
///
/// Per spec Section 5.2:
/// - Rule 1: No re-validation (WAL contains committed transactions)
/// - Rule 2: Apply results directly, not logic
/// - Rule 3: Single-threaded, in WAL order
/// - Rule 4: Preserve version numbers exactly
///
/// # Arguments
/// * `wal` - WAL to replay from
/// * `storage` - Storage to apply to
/// * `options` - Replay options (filtering, stopping point)
///
/// # Returns
/// * `Ok(ReplayStats)` - Statistics about replay
/// * `Err` - If replay fails
pub fn replay_wal_with_options(
    wal: &WAL,
    storage: &UnifiedStore,
    options: &ReplayOptions,
) -> Result<ReplayStats>;
```

### Implementation Steps

#### Step 1: Start the story

```bash
./scripts/start-story.sh 9 94 wal-replay-enhancement
```

#### Step 2: Write tests FIRST

```rust
#[cfg(test)]
mod replay_options_tests {
    use super::*;

    #[test]
    fn test_replay_filter_by_run_id() {
        // Create WAL with transactions from multiple runs
        // Replay with filter - only one run's transactions should apply
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("filter.wal");

        let run_a = RunId::new();
        let run_b = RunId::new();

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Transaction for run_a
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_a,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_a,
                key: Key::new_kv(create_ns(run_a), "key_a"),
                value: Value::I64(1),
                version: 10,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id: run_a }).unwrap();

            // Transaction for run_b
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_b,
                timestamp: now(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_b,
                key: Key::new_kv(create_ns(run_b), "key_b"),
                value: Value::I64(2),
                version: 20,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id: run_b }).unwrap();
        }

        // Replay only run_a
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let storage = UnifiedStore::new();
        let options = ReplayOptions {
            filter_run_id: Some(run_a),
            ..Default::default()
        };

        let stats = replay_wal_with_options(&wal, &storage, &options).unwrap();

        assert_eq!(stats.txns_applied, 1);
        // key_a should exist, key_b should not
    }

    #[test]
    fn test_replay_stop_at_version() {
        // Create WAL with transactions at versions 10, 20, 30
        // Replay with stop_at_version=25 - should only apply v10, v20
        // ...
    }

    #[test]
    fn test_replay_with_progress_callback() {
        // Verify callback is called for each transaction
        // ...
    }
}
```

#### Step 3: Implement replay_wal_with_options

Extend the existing `replay_wal` function to support options.

#### Step 4: Run validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability
~/.cargo/bin/cargo clippy -p in-mem-durability -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Acceptance Criteria

- [ ] ReplayOptions struct implemented
- [ ] replay_wal_with_options() supports run_id filtering
- [ ] replay_wal_with_options() supports version-based stopping
- [ ] Progress callback works correctly
- [ ] All tests pass
- [ ] No clippy warnings

---

## Story #95: Transaction Recovery

**GitHub Issue**: #95
**Estimated Time**: 4 hours
**Dependencies**: Story #93
**Blocks**: Story #96

### ⚠️ PREREQUISITE: Read the Semantics Spec

Before writing ANY code, read:
- Section 5.4: Recovery Algorithm
- Section 5.5: Incomplete Transaction Handling
- Appendix A.3: Why No AbortTxn WAL Entry (M2)

### What to Implement

Enhance recovery to handle all transaction edge cases:

1. **Interleaved transactions** from concurrent runs
2. **Transaction spanning multiple WAL segments** (future-proofing)
3. **Recovery of delete operations** with version preservation

```rust
/// Handle interleaved transaction recovery
///
/// Per spec: WAL entries are in commit order, but concurrent transactions
/// may have interleaved writes. Recovery groups by txn_id correctly.
///
/// Example WAL sequence:
/// - BeginTxn(1), BeginTxn(2), Write(txn1), Write(txn2), Commit(1), Commit(2)
///
/// This should correctly group and apply both transactions.
pub fn recover_interleaved_transactions(wal: &WAL, storage: &UnifiedStore) -> Result<ReplayStats>;
```

### Implementation Steps

#### Step 1: Start the story

```bash
./scripts/start-story.sh 9 95 transaction-recovery
```

#### Step 2: Write tests FIRST

```rust
#[test]
fn test_interleaved_transaction_recovery() {
    // Two transactions that overlap in the WAL
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("interleaved.wal");

    let run_id = RunId::new();
    let ns = create_test_namespace(run_id);

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Interleaved sequence
        wal.append(&WALEntry::BeginTxn { txn_id: 1, run_id, timestamp: now() }).unwrap();
        wal.append(&WALEntry::BeginTxn { txn_id: 2, run_id, timestamp: now() }).unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "from_txn1"),
            value: Value::I64(1),
            version: 100,
        }).unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "from_txn2"),
            value: Value::I64(2),
            version: 200,
        }).unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id }).unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id }).unwrap();
    }

    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let storage = UnifiedStore::new();
    let stats = replay_wal(&wal, &storage).unwrap();

    assert_eq!(stats.txns_applied, 2);

    use in_mem_core::traits::Storage;
    assert!(storage.get(&Key::new_kv(ns.clone(), "from_txn1")).unwrap().is_some());
    assert!(storage.get(&Key::new_kv(ns.clone(), "from_txn2")).unwrap().is_some());
}

#[test]
fn test_delete_recovery_preserves_version() {
    // Per spec: Deletes also preserve versions
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("deletes.wal");

    let run_id = RunId::new();
    let ns = create_test_namespace(run_id);
    let key = Key::new_kv(ns.clone(), "deleted_key");

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Write then delete
        wal.append(&WALEntry::BeginTxn { txn_id: 1, run_id, timestamp: now() }).unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: key.clone(),
            value: Value::String("exists".to_string()),
            version: 100,
        }).unwrap();
        wal.append(&WALEntry::Delete {
            run_id,
            key: key.clone(),
            version: 101,
        }).unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id }).unwrap();
    }

    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let storage = UnifiedStore::new();
    let stats = replay_wal(&wal, &storage).unwrap();

    assert_eq!(stats.writes_applied, 1);
    assert_eq!(stats.deletes_applied, 1);

    // Key should be deleted (tombstoned with version 101)
    use in_mem_core::traits::Storage;
    assert!(storage.get(&key).unwrap().is_none());
}

#[test]
fn test_recovery_multiple_runs_independent() {
    // Transactions from different runs are independent
    // Even if one run's transaction is incomplete, other run's should apply
    // ...
}
```

#### Step 3: Verify existing implementation handles these cases

The existing `replay_wal` in `crates/durability/src/recovery.rs` already handles most of these cases. Verify with tests and add any missing functionality.

#### Step 4: Run validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability
~/.cargo/bin/cargo test -p in-mem-concurrency
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Acceptance Criteria

- [ ] Interleaved transaction recovery works correctly
- [ ] Delete operations preserve versions during recovery
- [ ] Multiple runs recover independently
- [ ] All tests pass
- [ ] No clippy warnings

---

## Story #96: Crash Recovery Testing

**GitHub Issue**: #96
**Estimated Time**: 4 hours
**Dependencies**: Stories #94, #95
**Blocks**: Story #97

### ⚠️ PREREQUISITE: Read the Semantics Spec

Before writing ANY code, read:
- Section 5.5: Incomplete Transaction Handling
- Core Invariants: All-or-nothing commit

### What to Implement

Comprehensive crash scenario tests:

1. **Crash before BeginTxn written** - No trace, nothing to recover
2. **Crash after BeginTxn, before writes** - Incomplete, discard
3. **Crash mid-writes** - Incomplete, discard all writes
4. **Crash after writes, before CommitTxn** - Incomplete, discard
5. **Crash after CommitTxn, before storage apply** - Durable, MUST recover
6. **Crash during storage apply** - Durable, MUST recover (idempotent replay)

```rust
/// Test module for crash scenarios
///
/// Per spec Section 5.5:
/// "If a crash occurs during commit:
///  - Sees BeginTxn for txn_id 42
///  - Sees Write entries for txn_id 42
///  - Does NOT see CommitTxn for txn_id 42
///  - Conclusion: Transaction 42 is INCOMPLETE
///  - Action: DISCARD all entries for txn_id 42
///  - Result: Keys are NOT modified"
#[cfg(test)]
mod crash_scenarios {
    // Crash scenario tests here
}
```

### Implementation Steps

#### Step 1: Start the story

```bash
./scripts/start-story.sh 9 96 crash-recovery-testing
```

#### Step 2: Create comprehensive crash tests

```rust
#[cfg(test)]
mod crash_scenarios {
    use super::*;

    /// Scenario 1: Crash before any WAL activity
    /// Expected: Empty database
    #[test]
    fn test_crash_before_any_activity() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        // Create WAL file but write nothing
        let _wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        drop(_wal);

        // Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.final_version, 0);
    }

    /// Scenario 2: Crash after BeginTxn, before any writes
    /// Expected: Transaction discarded
    #[test]
    fn test_crash_after_begin_before_writes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("begin_only.wal");

        let run_id = RunId::new();

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();
            // CRASH - no writes, no commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);
    }

    /// Scenario 3: Crash mid-writes
    /// Expected: ALL writes from this transaction discarded
    #[test]
    fn test_crash_mid_writes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("mid_writes.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();

            // Some writes completed
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::I64(1),
                version: 10,
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::I64(2),
                version: 10,
            }).unwrap();
            // CRASH - more writes planned but not written, no commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // ALL writes discarded (all-or-nothing)
        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);
        assert_eq!(result.stats.writes_applied, 0);

        // Keys should NOT exist
        use in_mem_core::traits::Storage;
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "key1")).unwrap().is_none());
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "key2")).unwrap().is_none());
    }

    /// Scenario 4: Crash after all writes, before CommitTxn
    /// Expected: Transaction discarded
    #[test]
    fn test_crash_after_writes_before_commit() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("no_commit.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::I64(1),
                version: 10,
            }).unwrap();

            // CRASH - about to write CommitTxn but didn't
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);

        // Key should NOT exist
        use in_mem_core::traits::Storage;
        assert!(result.storage.get(&Key::new_kv(ns, "key1")).unwrap().is_none());
    }

    /// Scenario 5: Crash after CommitTxn written to WAL
    /// Expected: Transaction IS durable, MUST be recovered
    ///
    /// Per spec: "If crash occurs after step 8: Transaction is durable,
    /// replayed on recovery."
    #[test]
    fn test_crash_after_commit_written() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("committed.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            }).unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "durable_key"),
                value: Value::String("must_exist".to_string()),
                version: 100,
            }).unwrap();

            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id,
            }).unwrap();

            // CRASH - after commit marker written
            // (Storage may not have been updated yet)
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Transaction MUST be recovered
        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.incomplete_txns, 0);

        // Key MUST exist with correct value and version
        use in_mem_core::traits::Storage;
        let stored = result.storage.get(&Key::new_kv(ns, "durable_key")).unwrap().unwrap();
        assert_eq!(stored.value, Value::String("must_exist".to_string()));
        assert_eq!(stored.version, 100);
    }

    /// Scenario 6: One committed, one incomplete
    /// Expected: Committed applies, incomplete discarded
    #[test]
    fn test_one_committed_one_incomplete() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("mixed.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Committed transaction
            wal.append(&WALEntry::BeginTxn { txn_id: 1, run_id, timestamp: now() }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed"),
                value: Value::I64(1),
                version: 10,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id }).unwrap();

            // Incomplete transaction
            wal.append(&WALEntry::BeginTxn { txn_id: 2, run_id, timestamp: now() }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "uncommitted"),
                value: Value::I64(2),
                version: 20,
            }).unwrap();
            // CRASH - no commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.incomplete_txns, 1);

        use in_mem_core::traits::Storage;
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "committed")).unwrap().is_some());
        assert!(result.storage.get(&Key::new_kv(ns.clone(), "uncommitted")).unwrap().is_none());
    }

    /// Scenario 7: Multiple incomplete transactions
    /// Expected: All discarded
    #[test]
    fn test_multiple_incomplete_transactions() {
        // ...
    }

    /// Scenario 8: Recovery is idempotent
    /// Expected: Recovering twice gives same result
    #[test]
    fn test_recovery_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("idempotent.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn { txn_id: 1, run_id, timestamp: now() }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key"),
                value: Value::I64(42),
                version: 100,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id }).unwrap();
        }

        // Recover first time
        let coordinator = RecoveryCoordinator::new(wal_path.clone());
        let result1 = coordinator.recover().unwrap();

        // Recover second time (simulates restart)
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result2 = coordinator.recover().unwrap();

        // Results should be identical
        assert_eq!(result1.stats.txns_replayed, result2.stats.txns_replayed);
        assert_eq!(result1.stats.final_version, result2.stats.final_version);

        use in_mem_core::traits::Storage;
        let v1 = result1.storage.get(&Key::new_kv(ns.clone(), "key")).unwrap().unwrap();
        let v2 = result2.storage.get(&Key::new_kv(ns.clone(), "key")).unwrap().unwrap();
        assert_eq!(v1.value, v2.value);
        assert_eq!(v1.version, v2.version);
    }
}
```

#### Step 3: Run all tests

```bash
~/.cargo/bin/cargo test -p in-mem-concurrency -- crash_scenarios
~/.cargo/bin/cargo test --all
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Acceptance Criteria

- [ ] All 8 crash scenarios tested
- [ ] Incomplete transactions always discarded
- [ ] Committed transactions always recovered
- [ ] Recovery is idempotent
- [ ] All tests pass
- [ ] No clippy warnings

---

## Story #97: Recovery Validation

**GitHub Issue**: #97
**Estimated Time**: 3 hours
**Dependencies**: Story #96
**Blocks**: None (Epic 9 complete)

### What to Implement

End-to-end validation of recovery:

1. **Integration test**: Full database lifecycle with crash and recovery
2. **Property-based testing**: Random transaction sequences
3. **Performance benchmarks**: Recovery time for various WAL sizes

### Implementation Steps

#### Step 1: Start the story

```bash
./scripts/start-story.sh 9 97 recovery-validation
```

#### Step 2: Create integration tests

```rust
/// Integration test: Complete database lifecycle
#[test]
fn test_full_database_lifecycle_with_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("lifecycle.wal");

    // Phase 1: Normal operation
    let run_id = RunId::new();
    let ns = create_test_namespace(run_id);

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let storage = UnifiedStore::new();
        let txn_manager = TransactionManager::new(0);

        // Perform some transactions
        for i in 1..=10u64 {
            let txn_id = txn_manager.next_txn_id();
            let commit_version = txn_manager.current_version() + 1;

            // Write to WAL
            wal.append(&WALEntry::BeginTxn {
                txn_id,
                run_id,
                timestamp: Utc::now().timestamp(),
            }).unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), &format!("key{}", i)),
                value: Value::I64(i as i64),
                version: commit_version,
            }).unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id, run_id }).unwrap();

            // Apply to storage
            storage.put_with_version(
                Key::new_kv(ns.clone(), &format!("key{}", i)),
                Value::I64(i as i64),
                commit_version,
                None,
            ).unwrap();
        }

        // Verify state
        assert_eq!(storage.current_version(), 10);
    }

    // Phase 2: Simulate crash (just drop everything)

    // Phase 3: Recovery
    let coordinator = RecoveryCoordinator::new(wal_path);
    let result = coordinator.recover().unwrap();

    // Verify recovered state matches original
    assert_eq!(result.stats.txns_replayed, 10);
    assert_eq!(result.stats.final_version, 10);
    assert_eq!(result.txn_manager.current_version(), 10);

    use in_mem_core::traits::Storage;
    for i in 1..=10u64 {
        let key = Key::new_kv(ns.clone(), &format!("key{}", i));
        let stored = result.storage.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(i as i64));
        assert_eq!(stored.version, i);
    }
}
```

#### Step 3: Create validation report

Create `docs/milestones/EPIC_9_REVIEW.md` with:
- Test results summary
- Spec compliance verification
- Performance metrics (if applicable)

#### Step 4: Run full validation

```bash
~/.cargo/bin/cargo test --all
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Acceptance Criteria

- [ ] Integration test for full lifecycle
- [ ] All edge cases covered
- [ ] EPIC_9_REVIEW.md created
- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Epic 9 complete

### Complete the Epic

After Story #97 is merged to epic-9-recovery:

```bash
# Merge epic to develop
git checkout develop
git merge --no-ff epic-9-recovery
git push origin develop

# Update M2_PROJECT_STATUS.md
# Create EPIC_9_REVIEW.md
```

---

## Quick Reference: Story Commands

```bash
# Start a story
./scripts/start-story.sh 9 <story_number> <description>

# Run tests
~/.cargo/bin/cargo test -p in-mem-concurrency
~/.cargo/bin/cargo test -p in-mem-durability
~/.cargo/bin/cargo test --all

# Check code quality
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check

# Complete a story
./scripts/complete-story.sh <story_number>
```

---

## Spec Compliance Checklist

Before completing any story, verify:

| Requirement | Verified |
|-------------|----------|
| Replays do NOT re-run conflict detection (Rule 1) | [ ] |
| Replays apply results, not logic (Rule 2) | [ ] |
| Replays are single-threaded (Rule 3) | [ ] |
| Versions preserved exactly (Rule 4) | [ ] |
| Incomplete transactions discarded (Section 5.5) | [ ] |
| Deterministic replay (Section 5.6) | [ ] |
| Global version restored correctly (Section 6.1) | [ ] |

---

*Generated for Epic 9: Recovery Support*
*Spec Reference: docs/architecture/M2_TRANSACTION_SEMANTICS.md Section 5*
