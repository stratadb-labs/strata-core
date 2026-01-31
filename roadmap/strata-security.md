# strata-security: Access Control

**Theme**: A dedicated crate for access control, starting simple and growing with the architecture.

## Why a separate crate

Security policy should live in one place. Today the scope is small (read-only vs read-write), but server mode (v0.6) will need per-connection access modes, API key validation, and per-branch permissions. Building `strata-security` as a standalone crate now means:

- The command classification and policy enforcement APIs stabilize early
- Server mode imports and extends the crate rather than yanking control flow out of the executor
- Security logic is testable in isolation, without standing up a database

The crate sits between the public API (`strata-executor`) and the engine — it inspects commands and either allows or rejects them before they reach dispatch.

```
  Strata API (strata-executor)
        │
        ▼
  ┌─────────────┐
  │  strata-     │  ← policy check: is this command allowed?
  │  security    │
  └──────┬──────┘
         │
         ▼
  Executor dispatch → Engine → Storage
```

## Phase 1: Access Mode (embedded, pre-server)

### AccessMode enum

```rust
// crates/security/src/lib.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    ReadOnly,
    ReadWrite,
}
```

Set once at open time. Immutable for the lifetime of the handle.

### Command classification

The crate owns the classification of every `Command` variant as read or write.

```rust
// crates/security/src/classify.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAccess {
    Read,
    Write,
}

pub fn classify(cmd: &Command) -> CommandAccess { ... }
```

**Read commands** (allowed in both modes):

| Primitive | Commands |
|-----------|----------|
| KV | `KvGet`, `KvList`, `KvGetv` |
| JSON | `JsonGet`, `JsonGetv`, `JsonList` |
| Event | `EventRead`, `EventReadByType`, `EventLen` |
| State | `StateRead`, `StateReadv` |
| Vector | `VectorGet`, `VectorSearch`, `VectorListCollections` |
| Branch | `BranchGet`, `BranchList`, `BranchExists` |
| Transaction | `TxnInfo`, `TxnIsActive` |
| Retention | `RetentionStats`, `RetentionPreview` |
| Database | `Ping`, `Info` |
| Intelligence | `Search` |
| Bundle | `BranchBundleValidate` |

**Write commands** (rejected in read-only mode):

| Primitive | Commands |
|-----------|----------|
| KV | `KvPut`, `KvDelete` |
| JSON | `JsonSet`, `JsonDelete` |
| Event | `EventAppend` |
| State | `StateSet`, `StateCas`, `StateInit` |
| Vector | `VectorUpsert`, `VectorDelete`, `VectorCreateCollection`, `VectorDeleteCollection` |
| Branch | `BranchCreate`, `BranchDelete` |
| Transaction | `TxnBegin`, `TxnCommit`, `TxnRollback` |
| Retention | `RetentionApply` |
| Database | `Flush`, `Compact` |
| Bundle | `BranchExport`, `BranchImport` |

### Policy guard

```rust
// crates/security/src/guard.rs

pub struct PolicyGuard {
    mode: AccessMode,
}

impl PolicyGuard {
    pub fn new(mode: AccessMode) -> Self { ... }

    /// Returns Ok(()) if the command is allowed, Err if rejected.
    pub fn check(&self, cmd: &Command) -> Result<(), AccessDenied> {
        if self.mode == AccessMode::ReadOnly && classify(cmd) == CommandAccess::Write {
            return Err(AccessDenied {
                command: cmd.name(),
                reason: "database opened in read-only mode",
            });
        }
        Ok(())
    }
}
```

### Integration with Strata

The executor's `Strata` struct holds a `PolicyGuard` and calls `check()` before dispatch:

```rust
// In crates/executor/src/api/mod.rs

pub struct Strata {
    executor: Executor,
    current_branch: BranchId,
    guard: PolicyGuard,  // from strata-security
}

// All command dispatch goes through this internal method
fn execute_checked(&self, cmd: Command) -> Result<Output> {
    self.guard.check(&cmd)?;
    self.executor.execute(cmd)
}
```

### Public API

```rust
// Read-write (default, same as today)
let db = Strata::open("/path/to/data")?;

// Explicit read-only
let db = Strata::open_read_only("/path/to/data")?;

// Via builder
let db = Strata::builder()
    .path("/path/to/data")
    .access_mode(AccessMode::ReadOnly)
    .open()?;
```

### Error type

```rust
// crates/security/src/error.rs

#[derive(Debug, thiserror::Error)]
#[error("access denied: {command} rejected — {reason}")]
pub struct AccessDenied {
    pub command: String,
    pub reason: &'static str,
}
```

The executor's `Error` enum gets a new variant that wraps this:

```rust
pub enum Error {
    // ... existing variants ...
    AccessDenied(strata_security::AccessDenied),
}
```

## Phase 2: Server mode extensions (v0.6+)

When the TCP server ships, `strata-security` grows to handle multi-connection concerns. These are sketched here to show the crate's trajectory — not committed to implementation.

### Per-connection access mode

Each network connection gets its own `PolicyGuard`. The server assigns the mode based on authentication:

```rust
// Future: per-connection policy
let guard = PolicyGuard::new(connection.access_mode());
guard.check(&cmd)?;
```

### API key authentication

```rust
// crates/security/src/auth.rs (future)

pub struct ApiKey {
    pub key_id: String,
    pub access_mode: AccessMode,
    pub allowed_branches: Option<Vec<String>>,  // None = all branches
}

pub struct Authenticator {
    keys: HashMap<String, ApiKey>,
}

impl Authenticator {
    pub fn validate(&self, token: &str) -> Result<&ApiKey, AuthError> { ... }
}
```

### Per-branch permissions

Extend `PolicyGuard` to also check branch scope:

```rust
// Future: branch-scoped access
pub struct PolicyGuard {
    mode: AccessMode,
    allowed_branches: Option<HashSet<String>>,  // None = unrestricted
}
```

### Audit logging

Optional command-level audit trail:

```rust
// Future: audit log
pub trait AuditSink {
    fn record(&self, event: AuditEvent);
}

pub struct AuditEvent {
    pub timestamp: u64,
    pub command: String,
    pub branch: Option<String>,
    pub result: AuditResult,  // Allowed | Denied
}
```

## What this is NOT

- **Not RBAC/RCAC**: No roles, row-level, or column-level access control. The permission model is flat: a handle is either read-only or read-write.
- **Not encryption**: No data-at-rest or data-in-transit encryption.
- **Not a firewall**: In embedded mode, the caller IS the process. The crate prevents programming errors, not adversaries with process-level access.

## Crate structure

```
crates/security/
├── Cargo.toml          # depends on strata-executor (for Command type)
├── src/
│   ├── lib.rs          # AccessMode, re-exports
│   ├── classify.rs     # Command → Read/Write classification
│   ├── guard.rs        # PolicyGuard
│   └── error.rs        # AccessDenied
└── tests/
    ├── classify_tests.rs   # every command variant is classified
    └── guard_tests.rs      # read-only mode rejects writes, allows reads
```

## Dependencies

- `strata-executor` (for `Command` type) — or `strata-core` if `Command` moves to core
- `thiserror` (for error derive)
- No other dependencies. The crate is deliberately lightweight.

## Ordering

- Phase 1 can ship independently of any milestone
- Phase 2 ships with or after v0.6 (server mode)
