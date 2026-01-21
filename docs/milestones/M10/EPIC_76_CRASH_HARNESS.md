# Epic 76: Crash Harness

**Goal**: Implement a crash testing harness for validating storage correctness under failure conditions

**Dependencies**: Epic 72 (Recovery), Epic 75 (Database Lifecycle)

---

## Scope

- Crash harness framework
- Random process kill tests
- WAL tail corruption tests
- Reference model comparator
- Crash scenario matrix

---

## Rationale

> Storage bugs are catastrophic and often only manifest under specific failure conditions. A systematic crash harness is how serious storage engines are validated. This is not optional for a durable storage layer.

Examples of bugs that crash testing catches:
- Data loss on crash during WAL append
- Corruption when crash occurs mid-fsync
- Recovery failures with partial records
- State inconsistency after multiple consecutive crashes
- MANIFEST corruption scenarios

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #539 | Crash Harness Framework | CRITICAL |
| #540 | Random Process Kill Tests | CRITICAL |
| #541 | WAL Tail Corruption Tests | CRITICAL |
| #542 | Reference Model Comparator | HIGH |
| #543 | Crash Scenario Matrix | HIGH |

---

## Story #539: Crash Harness Framework

**File**: `crates/storage/src/testing/crash_harness.rs` (NEW)

**Deliverable**: Framework for systematic crash testing

### Design

The crash harness provides:
1. Controlled crash injection at configurable points
2. Process isolation for realistic crash simulation
3. State verification after recovery
4. Reproducible test scenarios

### Implementation

```rust
use std::path::{Path, PathBuf};
use std::process::{Command, Child, Stdio};
use std::time::Duration;

/// Crash harness for testing storage durability
pub struct CrashHarness {
    /// Test database directory
    db_dir: PathBuf,

    /// Reference model for expected state
    reference: ReferenceModel,

    /// Crash injection configuration
    config: CrashConfig,
}

/// Configuration for crash injection
#[derive(Debug, Clone)]
pub struct CrashConfig {
    /// Probability of crash at each injection point (0.0 - 1.0)
    pub crash_probability: f64,

    /// Types of crashes to simulate
    pub crash_types: Vec<CrashType>,

    /// Maximum operations before forced crash
    pub max_operations: usize,

    /// Timeout for child process
    pub timeout: Duration,
}

impl Default for CrashConfig {
    fn default() -> Self {
        CrashConfig {
            crash_probability: 0.1,
            crash_types: vec![
                CrashType::ProcessKill,
                CrashType::ProcessAbort,
            ],
            max_operations: 1000,
            timeout: Duration::from_secs(30),
        }
    }
}

/// Types of crash simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashType {
    /// SIGKILL - immediate process termination
    ProcessKill,

    /// SIGABRT - abort signal
    ProcessAbort,

    /// SIGSEGV - segmentation fault simulation
    SegFault,

    /// Power loss simulation (kill without cleanup)
    PowerLoss,
}

/// Crash injection points
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashPoint {
    /// Before writing WAL record
    BeforeWalWrite,

    /// After writing WAL record, before fsync
    AfterWalWriteBeforeFsync,

    /// After fsync, before returning
    AfterFsync,

    /// During segment rotation
    DuringSegmentRotation,

    /// During snapshot creation (before rename)
    DuringSnapshotBeforeRename,

    /// During snapshot creation (after rename)
    DuringSnapshotAfterRename,

    /// During MANIFEST update
    DuringManifestUpdate,

    /// During compaction
    DuringCompaction,
}

impl CrashHarness {
    /// Create a new crash harness
    pub fn new(db_dir: impl AsRef<Path>, config: CrashConfig) -> Self {
        CrashHarness {
            db_dir: db_dir.as_ref().to_path_buf(),
            reference: ReferenceModel::new(),
            config,
        }
    }

    /// Run a crash test scenario
    pub fn run_scenario<F>(&mut self, scenario: F) -> Result<CrashTestResult, CrashTestError>
    where
        F: FnOnce(&mut ScenarioRunner) -> Result<(), CrashTestError>,
    {
        let start_time = std::time::Instant::now();

        // Create fresh database
        self.setup_database()?;

        // Run scenario with crash injection
        let mut runner = ScenarioRunner::new(
            self.db_dir.clone(),
            &mut self.reference,
            self.config.clone(),
        );

        let scenario_result = scenario(&mut runner);

        // If scenario crashed (expected), verify recovery
        let recovery_result = self.verify_recovery()?;

        Ok(CrashTestResult {
            scenario_succeeded: scenario_result.is_ok(),
            recovery_succeeded: recovery_result.is_valid,
            crash_point: runner.last_crash_point,
            operations_completed: runner.operations_completed,
            duration: start_time.elapsed(),
            recovery_result,
        })
    }

    /// Setup fresh database for testing
    fn setup_database(&self) -> Result<(), CrashTestError> {
        // Remove existing database
        if self.db_dir.exists() {
            std::fs::remove_dir_all(&self.db_dir)?;
        }

        // Create new database
        let db = Database::create(&self.db_dir, DatabaseConfig::strict())?;
        db.close()?;

        Ok(())
    }

    /// Verify database state after recovery
    fn verify_recovery(&self) -> Result<RecoveryVerification, CrashTestError> {
        // Open database (triggers recovery)
        let db = match Database::open(&self.db_dir, DatabaseConfig::strict()) {
            Ok(db) => db,
            Err(e) => {
                return Ok(RecoveryVerification {
                    is_valid: false,
                    error: Some(format!("Recovery failed: {}", e)),
                    mismatches: vec![],
                });
            }
        };

        // Compare against reference model
        let mismatches = self.reference.compare(&db)?;

        Ok(RecoveryVerification {
            is_valid: mismatches.is_empty(),
            error: None,
            mismatches,
        })
    }
}

/// Runner for executing test scenarios
pub struct ScenarioRunner {
    db_dir: PathBuf,
    reference: *mut ReferenceModel, // Mutable reference
    config: CrashConfig,
    operations_completed: usize,
    last_crash_point: Option<CrashPoint>,
    rng: rand::rngs::StdRng,
}

impl ScenarioRunner {
    fn new(
        db_dir: PathBuf,
        reference: &mut ReferenceModel,
        config: CrashConfig,
    ) -> Self {
        ScenarioRunner {
            db_dir,
            reference: reference as *mut _,
            config,
            operations_completed: 0,
            last_crash_point: None,
            rng: rand::SeedableRng::seed_from_u64(42),
        }
    }

    /// Execute an operation with possible crash injection
    pub fn execute<F, R>(&mut self, crash_point: CrashPoint, op: F) -> Result<R, CrashTestError>
    where
        F: FnOnce(&Database) -> Result<R, StorageError>,
    {
        use rand::Rng;

        // Check if we should crash
        if self.rng.gen::<f64>() < self.config.crash_probability {
            self.last_crash_point = Some(crash_point);
            return Err(CrashTestError::SimulatedCrash(crash_point));
        }

        // Open database
        let db = Database::open(&self.db_dir, DatabaseConfig::strict())?;

        // Execute operation
        let result = op(&db)?;

        // Update reference model (safe because we have exclusive access)
        // unsafe { (*self.reference).record_success(...) }

        self.operations_completed += 1;
        db.close()?;

        Ok(result)
    }

    /// Execute a KV put with tracking
    pub fn kv_put(
        &mut self,
        run_name: &str,
        key: &str,
        value: &[u8],
    ) -> Result<(), CrashTestError> {
        self.execute(CrashPoint::AfterFsync, |db| {
            let run_id = db.get_or_create_run(run_name)?;
            db.kv_put(run_id, key, value)?;
            Ok(())
        })?;

        // Update reference on success
        unsafe {
            (*self.reference).kv_put(run_name, key, value.to_vec());
        }

        Ok(())
    }

    /// Execute a checkpoint with tracking
    pub fn checkpoint(&mut self) -> Result<(), CrashTestError> {
        self.execute(CrashPoint::DuringSnapshotAfterRename, |db| {
            db.checkpoint()?;
            Ok(())
        })?;

        // Update reference
        unsafe {
            (*self.reference).checkpoint();
        }

        Ok(())
    }
}

/// Result of a crash test
#[derive(Debug)]
pub struct CrashTestResult {
    /// Whether the scenario completed without crash
    pub scenario_succeeded: bool,

    /// Whether recovery was successful
    pub recovery_succeeded: bool,

    /// Crash injection point (if crashed)
    pub crash_point: Option<CrashPoint>,

    /// Number of operations completed before crash
    pub operations_completed: usize,

    /// Test duration
    pub duration: Duration,

    /// Detailed recovery verification
    pub recovery_result: RecoveryVerification,
}

/// Recovery verification result
#[derive(Debug)]
pub struct RecoveryVerification {
    /// Whether recovered state matches expected
    pub is_valid: bool,

    /// Error message if recovery failed
    pub error: Option<String>,

    /// List of state mismatches
    pub mismatches: Vec<StateMismatch>,
}

/// State mismatch found during verification
#[derive(Debug)]
pub struct StateMismatch {
    pub entity: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CrashTestError {
    #[error("Simulated crash at {0:?}")]
    SimulatedCrash(CrashPoint),

    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Acceptance Criteria

- [ ] `CrashHarness` with configurable crash injection
- [ ] `CrashPoint` enum for all injection points
- [ ] `CrashType` enum for different crash simulations
- [ ] `ScenarioRunner` for executing test operations
- [ ] `CrashTestResult` with detailed results
- [ ] Recovery verification after crash

---

## Story #540: Random Process Kill Tests

**File**: `crates/storage/src/testing/kill_tests.rs` (NEW)

**Deliverable**: Tests using actual process kills

### Design

More realistic than in-process crash simulation. Spawns child process, kills at random point, verifies recovery in parent.

### Implementation

```rust
use std::process::{Command, Child};
use std::time::Duration;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

/// Process-based crash tester
pub struct ProcessCrashTester {
    /// Path to test binary
    test_binary: PathBuf,

    /// Database directory
    db_dir: PathBuf,

    /// Test configuration
    config: ProcessCrashConfig,
}

#[derive(Debug, Clone)]
pub struct ProcessCrashConfig {
    /// Minimum time before kill (ms)
    pub min_runtime_ms: u64,

    /// Maximum time before kill (ms)
    pub max_runtime_ms: u64,

    /// Number of iterations
    pub iterations: usize,

    /// Signal to use for kill
    pub signal: Signal,
}

impl Default for ProcessCrashConfig {
    fn default() -> Self {
        ProcessCrashConfig {
            min_runtime_ms: 10,
            max_runtime_ms: 1000,
            iterations: 100,
            signal: Signal::SIGKILL,
        }
    }
}

impl ProcessCrashTester {
    pub fn new(
        test_binary: impl AsRef<Path>,
        db_dir: impl AsRef<Path>,
        config: ProcessCrashConfig,
    ) -> Self {
        ProcessCrashTester {
            test_binary: test_binary.as_ref().to_path_buf(),
            db_dir: db_dir.as_ref().to_path_buf(),
            config,
        }
    }

    /// Run random kill test iterations
    pub fn run(&self) -> Result<ProcessCrashResults, CrashTestError> {
        let mut results = ProcessCrashResults::new();

        for i in 0..self.config.iterations {
            let result = self.run_single_iteration(i)?;
            results.add(result);
        }

        Ok(results)
    }

    /// Run single kill iteration
    fn run_single_iteration(&self, iteration: usize) -> Result<KillIterationResult, CrashTestError> {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Calculate random kill time
        let kill_time_ms = rng.gen_range(
            self.config.min_runtime_ms..=self.config.max_runtime_ms
        );

        // Start child process
        let mut child = Command::new(&self.test_binary)
            .arg("--db-dir")
            .arg(&self.db_dir)
            .arg("--iteration")
            .arg(iteration.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = Pid::from_raw(child.id() as i32);

        // Wait for kill time
        std::thread::sleep(Duration::from_millis(kill_time_ms));

        // Kill the process
        let kill_result = kill(pid, self.config.signal);

        // Wait for process to actually die
        let _ = child.wait();

        // Verify recovery
        let recovery_ok = self.verify_recovery()?;

        Ok(KillIterationResult {
            iteration,
            kill_time_ms,
            kill_succeeded: kill_result.is_ok(),
            recovery_succeeded: recovery_ok,
        })
    }

    /// Verify database recovers correctly
    fn verify_recovery(&self) -> Result<bool, CrashTestError> {
        // Try to open database
        match Database::open(&self.db_dir, DatabaseConfig::strict()) {
            Ok(db) => {
                // Basic sanity checks
                // - Can list runs
                // - Can read data
                // - No panics
                let _ = db.list_runs();
                db.close()?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }
}

/// Results from process crash testing
#[derive(Debug)]
pub struct ProcessCrashResults {
    pub iterations: usize,
    pub successful_recoveries: usize,
    pub failed_recoveries: usize,
    pub results: Vec<KillIterationResult>,
}

impl ProcessCrashResults {
    fn new() -> Self {
        ProcessCrashResults {
            iterations: 0,
            successful_recoveries: 0,
            failed_recoveries: 0,
            results: Vec::new(),
        }
    }

    fn add(&mut self, result: KillIterationResult) {
        self.iterations += 1;
        if result.recovery_succeeded {
            self.successful_recoveries += 1;
        } else {
            self.failed_recoveries += 1;
        }
        self.results.push(result);
    }

    /// Check if all recoveries succeeded
    pub fn all_passed(&self) -> bool {
        self.failed_recoveries == 0
    }

    /// Get failure rate
    pub fn failure_rate(&self) -> f64 {
        self.failed_recoveries as f64 / self.iterations as f64
    }
}

#[derive(Debug)]
pub struct KillIterationResult {
    pub iteration: usize,
    pub kill_time_ms: u64,
    pub kill_succeeded: bool,
    pub recovery_succeeded: bool,
}
```

### Child Process Test Binary

```rust
// bin/crash_test_worker.rs
fn main() {
    let args: Vec<String> = std::env::args().collect();

    let db_dir = &args[2]; // --db-dir value
    let iteration: usize = args[4].parse().unwrap(); // --iteration value

    // Open or create database
    let db = Database::open_or_create(db_dir, DatabaseConfig::strict()).unwrap();

    // Run workload
    let run_id = db.get_or_create_run("crash-test-run").unwrap();

    // Write data continuously until killed
    let mut counter = 0u64;
    loop {
        let key = format!("key-{}-{}", iteration, counter);
        let value = format!("value-{}", counter);

        db.kv_put(run_id, &key, value.as_bytes()).unwrap();

        counter += 1;

        // Occasional checkpoint
        if counter % 100 == 0 {
            db.checkpoint().unwrap();
        }
    }
}
```

### Acceptance Criteria

- [ ] Spawn child process for realistic crash simulation
- [ ] Random kill timing within configurable range
- [ ] SIGKILL for immediate termination
- [ ] Recovery verification after kill
- [ ] Aggregated results over multiple iterations
- [ ] All iterations should recover successfully

---

## Story #541: WAL Tail Corruption Tests

**File**: `crates/storage/src/testing/corruption_tests.rs` (NEW)

**Deliverable**: Tests for WAL corruption scenarios

### Implementation

```rust
/// WAL corruption test utilities
pub struct WalCorruptionTester {
    db_dir: PathBuf,
}

impl WalCorruptionTester {
    pub fn new(db_dir: impl AsRef<Path>) -> Self {
        WalCorruptionTester {
            db_dir: db_dir.as_ref().to_path_buf(),
        }
    }

    /// Corrupt WAL tail by truncation
    pub fn truncate_wal_tail(&self, bytes_to_remove: usize) -> Result<(), CrashTestError> {
        let wal_dir = self.db_dir.join("WAL");

        // Find latest segment
        let mut segments: Vec<_> = std::fs::read_dir(&wal_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension() == Some(std::ffi::OsStr::new("seg"))
            })
            .collect();

        segments.sort_by_key(|e| e.path());

        if let Some(latest) = segments.last() {
            let path = latest.path();
            let metadata = std::fs::metadata(&path)?;
            let current_size = metadata.len();

            if current_size > bytes_to_remove as u64 {
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?;
                file.set_len(current_size - bytes_to_remove as u64)?;
            }
        }

        Ok(())
    }

    /// Append garbage bytes to WAL tail
    pub fn append_garbage(&self, garbage: &[u8]) -> Result<(), CrashTestError> {
        let wal_dir = self.db_dir.join("WAL");

        // Find latest segment
        let mut segments: Vec<_> = std::fs::read_dir(&wal_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension() == Some(std::ffi::OsStr::new("seg"))
            })
            .collect();

        segments.sort_by_key(|e| e.path());

        if let Some(latest) = segments.last() {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(latest.path())?;

            std::io::Write::write_all(&mut file, garbage)?;
        }

        Ok(())
    }

    /// Corrupt bytes at random positions in WAL
    pub fn corrupt_random_bytes(&self, count: usize) -> Result<(), CrashTestError> {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let wal_dir = self.db_dir.join("WAL");

        for entry in std::fs::read_dir(&wal_dir)? {
            let entry = entry?;
            if entry.path().extension() == Some(std::ffi::OsStr::new("seg")) {
                let path = entry.path();
                let mut data = std::fs::read(&path)?;

                // Skip header
                let header_size = 32;
                if data.len() > header_size {
                    for _ in 0..count {
                        let pos = rng.gen_range(header_size..data.len());
                        data[pos] ^= rng.gen::<u8>();
                    }
                    std::fs::write(&path, data)?;
                }
            }
        }

        Ok(())
    }

    /// Create partial record at WAL tail
    pub fn create_partial_record(&self) -> Result<(), CrashTestError> {
        // Write the beginning of a record (length prefix) but not the full record
        let partial = vec![
            0x10, 0x00, 0x00, 0x00, // Length = 16 bytes
            0x01,                    // Format version
            // ... rest is missing
        ];

        self.append_garbage(&partial)
    }

    /// Verify recovery handles corruption gracefully
    pub fn verify_recovery_after_corruption(&self) -> Result<CorruptionRecoveryResult, CrashTestError> {
        let result = Database::open(&self.db_dir, DatabaseConfig::strict());

        match result {
            Ok(db) => {
                // Recovered successfully
                let runs = db.list_runs()?;
                db.close()?;

                Ok(CorruptionRecoveryResult {
                    recovered: true,
                    error: None,
                    runs_found: runs.len(),
                })
            }
            Err(e) => {
                Ok(CorruptionRecoveryResult {
                    recovered: false,
                    error: Some(e.to_string()),
                    runs_found: 0,
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct CorruptionRecoveryResult {
    pub recovered: bool,
    pub error: Option<String>,
    pub runs_found: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_truncation_recovery() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with data
        {
            let db = Database::create(&db_path, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test").unwrap();

            for i in 0..100 {
                db.kv_put(run_id, &format!("key-{}", i), b"value").unwrap();
            }

            db.close().unwrap();
        }

        // Truncate WAL tail
        let tester = WalCorruptionTester::new(&db_path);
        tester.truncate_wal_tail(50).unwrap();

        // Should still recover
        let result = tester.verify_recovery_after_corruption().unwrap();
        assert!(result.recovered, "Should recover from truncation");
    }

    #[test]
    fn test_garbage_at_wal_tail() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with data
        {
            let db = Database::create(&db_path, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test").unwrap();
            db.kv_put(run_id, "key", b"value").unwrap();
            db.close().unwrap();
        }

        // Append garbage
        let tester = WalCorruptionTester::new(&db_path);
        tester.append_garbage(b"GARBAGE_DATA_HERE").unwrap();

        // Should still recover (truncates garbage)
        let result = tester.verify_recovery_after_corruption().unwrap();
        assert!(result.recovered, "Should recover with garbage truncated");
    }

    #[test]
    fn test_partial_record_recovery() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database
        {
            let db = Database::create(&db_path, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test").unwrap();
            db.kv_put(run_id, "key", b"value").unwrap();
            db.close().unwrap();
        }

        // Create partial record
        let tester = WalCorruptionTester::new(&db_path);
        tester.create_partial_record().unwrap();

        // Should recover
        let result = tester.verify_recovery_after_corruption().unwrap();
        assert!(result.recovered, "Should recover from partial record");
    }
}
```

### Acceptance Criteria

- [ ] `truncate_wal_tail()` simulates crash during write
- [ ] `append_garbage()` simulates partial write
- [ ] `corrupt_random_bytes()` simulates bit rot
- [ ] `create_partial_record()` simulates mid-record crash
- [ ] Recovery handles all corruption types gracefully
- [ ] Corrupted data is truncated, not propagated

---

## Story #542: Reference Model Comparator

**File**: `crates/storage/src/testing/reference_model.rs` (NEW)

**Deliverable**: Reference model for expected state tracking

### Implementation

```rust
use std::collections::HashMap;

/// Reference model tracking expected database state
///
/// Used by crash harness to verify recovery correctness.
pub struct ReferenceModel {
    /// KV state per run
    kv_state: HashMap<String, HashMap<String, Vec<u8>>>,

    /// Committed operations (successful before crash)
    committed_ops: Vec<Operation>,

    /// Pending operations (may or may not be committed)
    pending_ops: Vec<Operation>,

    /// Last checkpoint
    last_checkpoint: Option<u64>,
}

/// Operation recorded in reference model
#[derive(Debug, Clone)]
pub enum Operation {
    KvPut {
        run: String,
        key: String,
        value: Vec<u8>,
    },
    KvDelete {
        run: String,
        key: String,
    },
    EventAppend {
        run: String,
        payload: Vec<u8>,
    },
    Checkpoint,
}

impl ReferenceModel {
    pub fn new() -> Self {
        ReferenceModel {
            kv_state: HashMap::new(),
            committed_ops: Vec::new(),
            pending_ops: Vec::new(),
            last_checkpoint: None,
        }
    }

    /// Record a KV put (committed)
    pub fn kv_put(&mut self, run: &str, key: &str, value: Vec<u8>) {
        self.kv_state
            .entry(run.to_string())
            .or_insert_with(HashMap::new)
            .insert(key.to_string(), value.clone());

        self.committed_ops.push(Operation::KvPut {
            run: run.to_string(),
            key: key.to_string(),
            value,
        });
    }

    /// Record a KV delete (committed)
    pub fn kv_delete(&mut self, run: &str, key: &str) {
        if let Some(run_state) = self.kv_state.get_mut(run) {
            run_state.remove(key);
        }

        self.committed_ops.push(Operation::KvDelete {
            run: run.to_string(),
            key: key.to_string(),
        });
    }

    /// Record a checkpoint
    pub fn checkpoint(&mut self) {
        self.last_checkpoint = Some(self.committed_ops.len() as u64);
        self.committed_ops.push(Operation::Checkpoint);
    }

    /// Compare reference state against actual database
    pub fn compare(&self, db: &Database) -> Result<Vec<StateMismatch>, StorageError> {
        let mut mismatches = Vec::new();

        for (run_name, expected_kv) in &self.kv_state {
            let run_id = match db.resolve_run(run_name) {
                Ok(id) => id,
                Err(_) => {
                    mismatches.push(StateMismatch {
                        entity: format!("run:{}", run_name),
                        expected: "exists".to_string(),
                        actual: "not found".to_string(),
                    });
                    continue;
                }
            };

            for (key, expected_value) in expected_kv {
                let actual = db.kv_get(run_id, key)?;

                match actual {
                    Some(versioned) => {
                        if &versioned.value != expected_value {
                            mismatches.push(StateMismatch {
                                entity: format!("kv:{}:{}", run_name, key),
                                expected: format!("{:?}", expected_value),
                                actual: format!("{:?}", versioned.value),
                            });
                        }
                    }
                    None => {
                        mismatches.push(StateMismatch {
                            entity: format!("kv:{}:{}", run_name, key),
                            expected: format!("{:?}", expected_value),
                            actual: "not found".to_string(),
                        });
                    }
                }
            }
        }

        Ok(mismatches)
    }

    /// Get expected state for a key
    pub fn get_expected(&self, run: &str, key: &str) -> Option<&Vec<u8>> {
        self.kv_state.get(run)?.get(key)
    }

    /// Check if state matches
    pub fn matches(&self, db: &Database) -> Result<bool, StorageError> {
        Ok(self.compare(db)?.is_empty())
    }
}

impl Default for ReferenceModel {
    fn default() -> Self {
        Self::new()
    }
}
```

### Acceptance Criteria

- [ ] Track expected KV state per run
- [ ] Record committed operations
- [ ] `compare()` finds all mismatches with actual database
- [ ] `matches()` for quick check
- [ ] Supports KV, Event, State primitives

---

## Story #543: Crash Scenario Matrix

**File**: `crates/storage/tests/crash_scenarios.rs` (NEW)

**Deliverable**: Comprehensive crash scenario test suite

### Implementation

```rust
/// Crash scenario matrix covering all critical paths
#[cfg(test)]
mod crash_scenarios {
    use super::*;

    // === WAL Crash Scenarios ===

    #[test]
    fn crash_during_wal_append_before_write() {
        run_crash_scenario(CrashPoint::BeforeWalWrite, |runner| {
            runner.kv_put("test", "key", b"value")
        });
        // Expected: key not present (write didn't happen)
    }

    #[test]
    fn crash_during_wal_append_after_write_before_fsync() {
        run_crash_scenario(CrashPoint::AfterWalWriteBeforeFsync, |runner| {
            runner.kv_put("test", "key", b"value")
        });
        // Expected: key may or may not be present (buffered)
    }

    #[test]
    fn crash_during_wal_append_after_fsync() {
        run_crash_scenario(CrashPoint::AfterFsync, |runner| {
            runner.kv_put("test", "key", b"value")
        });
        // Expected: key present (committed)
    }

    // === Segment Rotation Scenarios ===

    #[test]
    fn crash_during_segment_rotation() {
        run_crash_scenario(CrashPoint::DuringSegmentRotation, |runner| {
            // Write enough data to trigger rotation
            for i in 0..10000 {
                runner.kv_put("test", &format!("key-{}", i), b"value")?;
            }
            Ok(())
        });
        // Expected: all committed data present
    }

    // === Snapshot Scenarios ===

    #[test]
    fn crash_during_snapshot_before_rename() {
        run_crash_scenario(CrashPoint::DuringSnapshotBeforeRename, |runner| {
            runner.kv_put("test", "key", b"value")?;
            runner.checkpoint()
        });
        // Expected: data present, snapshot may not exist
    }

    #[test]
    fn crash_during_snapshot_after_rename() {
        run_crash_scenario(CrashPoint::DuringSnapshotAfterRename, |runner| {
            runner.kv_put("test", "key", b"value")?;
            runner.checkpoint()
        });
        // Expected: data present, snapshot exists
    }

    // === MANIFEST Scenarios ===

    #[test]
    fn crash_during_manifest_update() {
        run_crash_scenario(CrashPoint::DuringManifestUpdate, |runner| {
            runner.kv_put("test", "key", b"value")?;
            runner.checkpoint()
        });
        // Expected: old MANIFEST or new MANIFEST, never corrupt
    }

    // === Compaction Scenarios ===

    #[test]
    fn crash_during_compaction() {
        run_crash_scenario(CrashPoint::DuringCompaction, |runner| {
            for i in 0..100 {
                runner.kv_put("test", &format!("key-{}", i), b"value")?;
            }
            runner.checkpoint()?;
            runner.compact(CompactMode::WALOnly)
        });
        // Expected: all data present, compaction may not have completed
    }

    // === Multiple Crash Scenarios ===

    #[test]
    fn multiple_consecutive_crashes() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Crash 1: During write
        {
            let mut harness = CrashHarness::new(&db_path, CrashConfig::default());
            let _ = harness.run_scenario(|runner| {
                runner.kv_put("test", "key1", b"value1")
            });
        }

        // Crash 2: During checkpoint
        {
            let mut harness = CrashHarness::new(&db_path, CrashConfig::default());
            let _ = harness.run_scenario(|runner| {
                runner.kv_put("test", "key2", b"value2")?;
                runner.checkpoint()
            });
        }

        // Crash 3: During another write
        {
            let mut harness = CrashHarness::new(&db_path, CrashConfig::default());
            let _ = harness.run_scenario(|runner| {
                runner.kv_put("test", "key3", b"value3")
            });
        }

        // Final recovery should succeed
        let db = Database::open(&db_path, DatabaseConfig::strict()).unwrap();
        // Some data may be present depending on crash timing
        db.close().unwrap();
    }

    // === Property-Based Testing ===

    #[test]
    fn random_operation_sequence_with_crashes() {
        use proptest::prelude::*;

        proptest!(|(
            operations in prop::collection::vec(
                prop_oneof![
                    Just(Op::Put),
                    Just(Op::Delete),
                    Just(Op::Checkpoint),
                ],
                1..100
            ),
            crash_point in 0usize..100
        )| {
            let dir = tempdir().unwrap();
            let mut harness = CrashHarness::new(
                dir.path().join("test.db"),
                CrashConfig {
                    crash_probability: 0.0, // Deterministic crash point
                    ..Default::default()
                },
            );

            let result = harness.run_scenario(|runner| {
                for (i, op) in operations.iter().enumerate() {
                    if i == crash_point {
                        return Err(CrashTestError::SimulatedCrash(CrashPoint::AfterFsync));
                    }

                    match op {
                        Op::Put => {
                            runner.kv_put("test", &format!("key-{}", i), b"value")?;
                        }
                        Op::Delete => {
                            runner.kv_delete("test", &format!("key-{}", i))?;
                        }
                        Op::Checkpoint => {
                            runner.checkpoint()?;
                        }
                    }
                }
                Ok(())
            });

            // Recovery should always succeed
            prop_assert!(result.unwrap().recovery_succeeded);
        });
    }

    // Helper
    fn run_crash_scenario<F>(crash_point: CrashPoint, scenario: F)
    where
        F: FnOnce(&mut ScenarioRunner) -> Result<(), CrashTestError>,
    {
        let dir = tempdir().unwrap();
        let mut harness = CrashHarness::new(
            dir.path().join("test.db"),
            CrashConfig {
                crash_probability: 1.0, // Always crash at specified point
                ..Default::default()
            },
        );

        harness.set_crash_point(crash_point);
        let result = harness.run_scenario(scenario).unwrap();

        assert!(
            result.recovery_succeeded,
            "Recovery failed after crash at {:?}",
            crash_point
        );
    }

    #[derive(Clone)]
    enum Op {
        Put,
        Delete,
        Checkpoint,
    }
}
```

### Crash Scenario Matrix Summary

| Scenario | Crash Point | Expected Result |
|----------|-------------|-----------------|
| WAL write before | BeforeWalWrite | Data not present |
| WAL write after, no fsync | AfterWalWriteBeforeFsync | Data may be present |
| WAL write after fsync | AfterFsync | Data present |
| Segment rotation | DuringSegmentRotation | All committed data present |
| Snapshot before rename | DuringSnapshotBeforeRename | Data present, no new snapshot |
| Snapshot after rename | DuringSnapshotAfterRename | Data present, snapshot exists |
| MANIFEST update | DuringManifestUpdate | Valid MANIFEST (old or new) |
| Compaction | DuringCompaction | All data present |
| Multiple crashes | Various | Recovery succeeds |

### Acceptance Criteria

- [ ] Tests for all CrashPoint variants
- [ ] Tests for WAL, snapshot, MANIFEST, compaction scenarios
- [ ] Multiple consecutive crash test
- [ ] Property-based random operation testing
- [ ] All scenarios should recover successfully
- [ ] Clear documentation of expected behavior per scenario

---

## CI Integration

```yaml
# .github/workflows/crash-tests.yml
name: Crash Tests

on:
  push:
    branches: [main, develop]
  pull_request:

jobs:
  crash-tests:
    runs-on: ubuntu-latest
    timeout-minutes: 30

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build crash test binaries
        run: cargo build --release -p strata-storage --features testing

      - name: Run crash harness tests
        run: cargo test --release -p strata-storage crash -- --test-threads=1

      - name: Run process kill tests
        run: cargo test --release -p strata-storage process_kill -- --test-threads=1 --ignored

      - name: Run corruption tests
        run: cargo test --release -p strata-storage corruption -- --test-threads=1

      - name: Run crash scenario matrix
        run: cargo test --release -p strata-storage crash_scenarios -- --test-threads=1
```

### Acceptance Criteria

- [ ] CI workflow for crash tests
- [ ] Runs on every push to main/develop
- [ ] Single-threaded execution (isolation)
- [ ] 30-minute timeout
- [ ] Separate jobs for different test types

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/testing/mod.rs` | CREATE - Testing module |
| `crates/storage/src/testing/crash_harness.rs` | CREATE - Crash harness |
| `crates/storage/src/testing/kill_tests.rs` | CREATE - Process kill tests |
| `crates/storage/src/testing/corruption_tests.rs` | CREATE - Corruption tests |
| `crates/storage/src/testing/reference_model.rs` | CREATE - Reference model |
| `crates/storage/tests/crash_scenarios.rs` | CREATE - Scenario matrix |
| `.github/workflows/crash-tests.yml` | CREATE - CI workflow |
