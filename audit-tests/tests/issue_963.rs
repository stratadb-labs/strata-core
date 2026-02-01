//! Audit test for issue #963: Commit always calls wal.flush() regardless of durability mode
//! Verdict: ARCHITECTURAL CHOICE (questionable)
//!
//! When a transaction commits (or any write operation completes), the WAL
//! `flush()` method is called to ensure data is persisted to disk. This
//! happens regardless of the configured durability mode.
//!
//! Strata supports multiple durability modes (e.g., via `DurabilityMode`):
//! - `Sync` / `Fsync` — flush and fsync on every commit (strongest guarantee)
//! - `Periodic` — flush periodically, not on every commit
//! - `None` / `NoSync` — never explicitly flush (OS decides when to write)
//!
//! However, the commit path always calls `wal.flush()`, effectively ignoring
//! the durability mode setting for the flush decision. This means:
//! - In `Periodic` or `None` mode, writes are still flushed on every commit,
//!   negating the performance benefit of relaxed durability
//! - The durability mode setting may only affect fsync behavior, not flush
//!   behavior, which is a subtle distinction that may surprise users
//!
//! This is classified as an ARCHITECTURAL CHOICE because:
//! 1. Always flushing provides a stronger safety default — data loss window
//!    is minimized even if the application crashes
//! 2. The performance difference between flush-on-commit and periodic-flush
//!    depends heavily on the workload and storage hardware
//! 3. Users who chose `DurabilityMode::Cache` may be surprised that their
//!    writes are still being flushed, but this errs on the side of safety
//! 4. Separating flush from fsync is a valid design: flush pushes data to
//!    the OS page cache, fsync pushes it to stable storage. Skipping flush
//!    risks losing data even from application crashes (not just power loss)

/// Documents the architectural choice regarding flush behavior and durability modes.
/// Testing the actual flush/fsync behavior requires monitoring system calls,
/// which is outside the executor API scope.
#[test]
fn issue_963_commit_always_flushes_documented() {
    // The commit path calls wal.flush() unconditionally:
    //
    //   fn commit(&self) -> Result<Version> {
    //       // ... write entries to WAL ...
    //       self.wal.flush()?;    // <-- always called
    //       // ... update in-memory state ...
    //   }
    //
    // The DurabilityMode is checked elsewhere (e.g., for fsync decisions)
    // but the flush itself is not gated by it.
    //
    // Impact by durability mode:
    //   Sync:     flush + fsync on commit (correct, intended behavior)
    //   Periodic: flush on commit, fsync periodically (flush may be unnecessary)
    //   None:     flush on commit, no fsync (flush may be unnecessary)
    //
    // The questionable aspect is that `Periodic` and `None` modes suggest
    // reduced I/O overhead, but the per-commit flush is still performed.
    //
    // ARCHITECTURAL CHOICE: Always flushing is a conservative default that
    // prevents data loss from application crashes. The durability mode
    // primarily controls fsync behavior for power-loss protection.
}
