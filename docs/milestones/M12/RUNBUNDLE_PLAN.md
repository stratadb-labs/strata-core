# RunBundle Implementation Plan

> **Status**: ✅ MVP COMPLETE (All 7 Phases)
> **Scope**: Export, verify, and import runs into empty databases
> **API Location**: RunIndex level (`run_index.export_run`, `run_index.import_run`, `run_index.verify_bundle`)
> **Format**: `.runbundle.tar.zst` - compressed tar archive

---

## MVP Scope

| Feature | MVP | Post-MVP |
|---------|-----|----------|
| Export terminal runs | ✅ | |
| Verify bundle integrity | ✅ | |
| Import into empty database | ✅ | |
| Import conflict handling (Reject/NewRunId) | | ✅ |
| Import into non-empty database | | ✅ |
| Run ID remapping | | ✅ |
| Snapshot acceleration | | ✅ |
| Writeset index | | ✅ |

**MVP contract**: Export a run, verify the bundle, import into a fresh database, get identical state.

---

## 1. Design Decisions Summary

Based on architectural discussion and clarifying questions:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Format** | `tar.zst` archive | Single artifact, streamable, inspectable with standard tools |
| **Contents** | WAL (required) + Snapshot (optional) | WAL is semantic truth, snapshot is acceleration |
| **Import ID** | Preserve ID, reject on conflict | Identity preservation is default truth |
| **Conflict Option** | `on_conflict: Reject \| NewRunId` | User choice for handling duplicates |
| **Exportable States** | Terminal only | Completed, Failed, Cancelled, Archived |
| **Rule** | Exportable ⇔ terminal | Paused/Active are resumable, not exportable |

### Why tar.zst?

- **Single artifact**: Easy to move, upload, attach to bug reports
- **Streamable**: No "must fit in memory" constraint for large runs
- **Inspectable**: `tar -tf` shows contents, extract for debugging
- **Compression**: zstd provides excellent compression with fast decompression
- **Standard tooling**: No custom tools needed for basic inspection

---

## 2. RunBundle Archive Format Specification

### 2.1 Archive Structure

File extension: `.runbundle.tar.zst`

**MVP structure:**
```
<run_id>.runbundle.tar.zst
└── runbundle/
    ├── MANIFEST.json        # Bundle format metadata (required)
    ├── RUN.json             # Run metadata and state (required)
    └── WAL.runlog           # Run-scoped WAL entries (required)
```

**Post-MVP additions:**
```
    ├── INDEX/               # Optional acceleration indices
    │   └── writeset.bin     # Key-version index for fast import
    └── SNAPSHOT/            # Optional materialized state
        ├── kv.bin           # KV primitive section
        ├── json.bin         # JSON primitive section
        ├── event.bin        # Event primitive section
        ├── state.bin        # State primitive section
        └── vector.bin       # Vector primitive section
```

### 2.2 MANIFEST.json

Bundle format metadata for versioning and validation.

**MVP:**
```json
{
  "format_version": 1,
  "strata_version": "0.12.0",
  "created_at": "2025-01-24T12:00:00Z",
  "checksum_algorithm": "xxh3",
  "checksums": {
    "RUN.json": "abc123...",
    "WAL.runlog": "def456..."
  },
  "contents": {
    "wal_entry_count": 1234,
    "wal_size_bytes": 56789
  }
}
```

**Post-MVP additions to `checksums`:** `INDEX/*`, `SNAPSHOT/*`
**Post-MVP additions to `contents`:** `has_snapshot`, `has_index`

### 2.3 RUN.json

Run identity and metadata (human-readable for debugging).

```json
{
  "run_id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "my-agent-run",
  "state": "completed",
  "created_at": "2025-01-24T10:00:00Z",
  "closed_at": "2025-01-24T11:30:00Z",
  "parent_run_id": null,
  "tags": ["production", "v2.1"],
  "metadata": {
    "user_id": "user_123",
    "model": "gpt-4"
  },
  "error": null
}
```

### 2.4 WAL.runlog

Binary file containing run-scoped WAL entries.

```
┌─────────────────────────────────────────────────────────────────┐
│ Header (16 bytes)                                               │
├─────────────────────────────────────────────────────────────────┤
│ Magic: "STRATA_WAL" (10 bytes)                                  │
│ Version: u16 (2 bytes, LE)                                      │
│ Entry Count: u32 (4 bytes, LE)                                  │
├─────────────────────────────────────────────────────────────────┤
│ Entries (variable)                                              │
├─────────────────────────────────────────────────────────────────┤
│ For each entry:                                                 │
│   Length: u32 (4 bytes, LE)                                     │
│   Data: [u8; length] (bincode-serialized WALEntry)              │
│   CRC32: u32 (4 bytes, LE)                                      │
└─────────────────────────────────────────────────────────────────┘
```

### 2.5 INDEX/writeset.bin (Optional)

Acceleration index for fast import - maps keys to final versions.

```
┌─────────────────────────────────────────────────────────────────┐
│ Header (8 bytes)                                                │
├─────────────────────────────────────────────────────────────────┤
│ Magic: "WSET" (4 bytes)                                         │
│ Entry Count: u32 (4 bytes, LE)                                  │
├─────────────────────────────────────────────────────────────────┤
│ Entries (variable)                                              │
├─────────────────────────────────────────────────────────────────┤
│ For each entry:                                                 │
│   Key Length: u16 (2 bytes, LE)                                 │
│   Key: [u8; key_length]                                         │
│   Final Version: u64 (8 bytes, LE)                              │
│   Is Delete: u8 (1 byte, 0=write, 1=delete)                     │
└─────────────────────────────────────────────────────────────────┘
```

### 2.6 SNAPSHOT/*.bin (Optional)

Materialized state per primitive, using existing `PrimitiveSection` format.

Each file contains:
```
┌─────────────────────────────────────────────────────────────────┐
│ Magic: "PRIM" (4 bytes)                                         │
│ Primitive Type: u8 (1 byte, same as snapshot primitive_ids)     │
│ Data Length: u64 (8 bytes, LE)                                  │
│ Data: [u8; data_length] (primitive-specific serialization)      │
│ CRC32: u32 (4 bytes, LE)                                        │
└─────────────────────────────────────────────────────────────────┘
```

### 2.7 Run States

Only terminal states can be exported:

| State | Value | Exportable |
|-------|-------|------------|
| Active | `"active"` | No |
| Paused | `"paused"` | No |
| Completed | `"completed"` | Yes |
| Failed | `"failed"` | Yes |
| Cancelled | `"cancelled"` | Yes |
| Archived | `"archived"` | Yes |

---

## 3. Core Types

### 3.1 New Types (in `crates/durability/src/run_bundle/`)

```rust
/// RunBundle format version
pub const RUNBUNDLE_FORMAT_VERSION: u32 = 1;

/// Archive paths
pub mod paths {
    pub const MANIFEST: &str = "runbundle/MANIFEST.json";
    pub const RUN: &str = "runbundle/RUN.json";
    pub const WAL: &str = "runbundle/WAL.runlog";
    pub const INDEX_DIR: &str = "runbundle/INDEX";
    pub const WRITESET_INDEX: &str = "runbundle/INDEX/writeset.bin";
    pub const SNAPSHOT_DIR: &str = "runbundle/SNAPSHOT";
}

/// Magic bytes for internal binary files
pub const WAL_MAGIC: &[u8; 10] = b"STRATA_WAL";
pub const WRITESET_MAGIC: &[u8; 4] = b"WSET";
pub const PRIMITIVE_MAGIC: &[u8; 4] = b"PRIM";

/// MANIFEST.json structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format_version: u32,
    pub strata_version: String,
    pub created_at: String,  // ISO 8601
    pub checksum_algorithm: String,  // "xxh3"
    pub checksums: HashMap<String, String>,
    pub contents: BundleContents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleContents {
    pub wal_entry_count: u64,
    pub wal_size_bytes: u64,
    // Post-MVP:
    // pub has_snapshot: bool,
    // pub has_index: bool,
}

/// RUN.json structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleRunInfo {
    pub run_id: String,
    pub name: String,
    pub state: String,  // "completed", "failed", "cancelled", "archived"
    pub created_at: String,  // ISO 8601
    pub closed_at: String,   // ISO 8601
    pub parent_run_id: Option<String>,
    pub tags: Vec<String>,
    pub metadata: Value,
    pub error: Option<String>,
}

/// Information returned from export
#[derive(Debug, Clone)]
pub struct RunExportInfo {
    pub run_id: RunId,
    pub path: PathBuf,
    pub wal_entry_count: u64,
    pub bundle_size_bytes: u64,
    pub checksum: String,  // xxh3 of entire .tar.zst
}

/// Information returned from import
#[derive(Debug, Clone)]
pub struct ImportedRunInfo {
    pub run_id: RunId,
    pub wal_entries_replayed: u64,
}

/// Information returned from verify
#[derive(Debug, Clone)]
pub struct BundleVerifyInfo {
    pub run_id: RunId,
    pub format_version: u32,
    pub wal_entry_count: u64,
    pub checksums_valid: bool,
}

/// Export options
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub compression_level: i32,  // zstd level, default: 3
    // Post-MVP:
    // pub include_snapshot: bool,
    // pub include_index: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            compression_level: 3,
        }
    }
}

// Post-MVP: Conflict resolution strategy for import
// pub enum ImportConflictStrategy { Reject, NewRunId }
// pub struct ImportOptions { pub on_conflict: ..., pub use_snapshot: ... }
```

### 3.2 Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum RunBundleError {
    #[error("Run not found: {0}")]
    RunNotFound(RunId),

    #[error("Run is not in terminal state (current: {0})")]
    NotTerminal(String),

    #[error("Invalid bundle: {0}")]
    InvalidBundle(String),

    #[error("Missing required file in bundle: {0}")]
    MissingFile(String),

    #[error("Checksum mismatch for {file}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        file: String,
        expected: String,
        actual: String,
    },

    #[error("Run already exists: {0}")]
    RunAlreadyExists(RunId),

    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u32),

    #[error("Archive error: {0}")]
    Archive(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("WAL replay error: {0}")]
    WalReplay(String),
}
```

---

## 4. Implementation Phases

### Phase 1: Core Types and Dependencies (Low Effort) ✅ COMPLETE

**Files to create:**
- `crates/durability/src/run_bundle/mod.rs` - Module root
- `crates/durability/src/run_bundle/types.rs` - Core types
- `crates/durability/src/run_bundle/error.rs` - Error types

**Dependencies to add (Cargo.toml):**
```toml
[dependencies]
tar = "0.4"
zstd = "0.13"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
```

**Tasks:**
1. Define `BundleManifest`, `BundleRunInfo`, `BundleContents`
2. Define `ExportOptions`, `ImportOptions`, `ImportConflictStrategy`
3. Define `RunExportInfo`, `ImportedRunInfo`
4. Add `RunBundleError` enum
5. Add module to `crates/durability/src/lib.rs`

**Tests:**
- Manifest JSON round-trip
- RunInfo JSON round-trip
- Options default values

### Phase 2: WAL.runlog Writer/Reader (Medium Effort) ✅ COMPLETE

**Files to create:**
- `crates/durability/src/run_bundle/wal_log.rs` - WAL log file handling

**Tasks:**
1. Implement WAL.runlog writer:
   ```rust
   pub struct WalLogWriter;

   impl WalLogWriter {
       /// Write WAL entries to a .runlog file
       pub fn write<W: Write>(
           entries: &[WALEntry],
           writer: W,
       ) -> Result<WalLogInfo, RunBundleError>;
   }

   pub struct WalLogInfo {
       pub entry_count: u64,
       pub bytes_written: u64,
       pub checksum: String,
   }
   ```

2. Implement WAL.runlog reader:
   ```rust
   pub struct WalLogReader;

   impl WalLogReader {
       /// Read WAL entries from a .runlog file
       pub fn read<R: Read>(reader: R) -> Result<Vec<WALEntry>, RunBundleError>;

       /// Stream WAL entries (for large files)
       pub fn stream<R: Read>(reader: R) -> impl Iterator<Item = Result<WALEntry, RunBundleError>>;
   }
   ```

3. Implement WAL entry filtering:
   ```rust
   pub fn filter_wal_for_run(entries: &[WALEntry], run_id: &RunId) -> Vec<WALEntry> {
       entries.iter()
           .filter(|e| e.run_id() == Some(*run_id))
           .cloned()
           .collect()
   }
   ```

**Tests:**
- Write/read empty WAL
- Write/read with entries
- CRC32 validation per entry
- Streaming read for large WAL

### Phase 3: Archive Writer (Medium Effort) ✅ COMPLETE

**Files to create:**
- `crates/durability/src/run_bundle/writer.rs` - Archive creation

**Tasks:**
1. Implement `RunBundleWriter`:
   ```rust
   pub struct RunBundleWriter {
       compression_level: i32,
   }

   impl RunBundleWriter {
       pub fn new(options: &ExportOptions) -> Self;

       /// Write a complete bundle to path (MVP: no snapshot)
       pub fn write(
           &self,
           manifest: &BundleManifest,
           run_info: &BundleRunInfo,
           wal_entries: &[WALEntry],
           path: &Path,
       ) -> Result<RunExportInfo, RunBundleError>;
   }
   ```

2. Implement atomic write pattern:
   - Write to `<path>.tmp`
   - Rename to `<path>` on success
   - Delete temp on failure

3. Implement checksum calculation (xxh3) for each file

**Tests:**
- Write bundle (MANIFEST + RUN + WAL)
- Atomic write (partial file not left on failure)
- Verify tar structure with `tar -tf`
- Verify checksums in manifest match actual files

### Phase 4: Archive Reader (Medium Effort) ✅ COMPLETE

**Files to create:**
- `crates/durability/src/run_bundle/reader.rs` - Archive reading

**Tasks:**
1. Implement `RunBundleReader`:
   ```rust
   pub struct RunBundleReader;

   impl RunBundleReader {
       /// Validate bundle integrity (checksums, required files)
       pub fn validate(path: &Path) -> Result<BundleManifest, RunBundleError>;

       /// Read manifest only
       pub fn read_manifest(path: &Path) -> Result<BundleManifest, RunBundleError>;

       /// Read run info only
       pub fn read_run_info(path: &Path) -> Result<BundleRunInfo, RunBundleError>;

       /// Read WAL entries
       pub fn read_wal_entries(path: &Path) -> Result<Vec<WALEntry>, RunBundleError>;
   }
   ```

2. Implement checksum validation against manifest
3. Implement streaming decompression

**Tests:**
- Read back written bundle
- Validate checksums match manifest
- Detect corrupted archive (bad checksum)
- Detect truncated archive
- Detect missing required files (MANIFEST, RUN, WAL)

### Phase 5: Database Export API (Medium Effort) ✅ COMPLETE

**Files modified:**
- `crates/primitives/src/run_index.rs` - Add `export_run` and `export_run_with_options` methods

> **Implementation Note:** The export_run method is on RunIndex rather than Database
> to avoid circular dependency (primitives depends on engine). This is the natural
> location since RunIndex already has access to both run metadata and the Database.

**Tasks:**
1. ✅ Add `export_run` method to `RunIndex`:
   ```rust
   impl RunIndex {
       /// Export a terminal run as a portable bundle
       pub fn export_run(&self, run_id: &str, path: &Path) -> RunBundleResult<RunExportInfo>;

       /// Export with custom options (compression level)
       pub fn export_run_with_options(
           &self,
           run_id: &str,
           path: &Path,
           options: &ExportOptions,
       ) -> RunBundleResult<RunExportInfo>;
   }
   ```

2. ✅ Add helper functions:
   - `is_exportable_status()` - Check if run can be exported
   - `metadata_to_bundle_run_info()` - Convert RunMetadata to BundleRunInfo
   - `format_timestamp_iso8601()` - Format timestamps for JSON
   - `value_to_json()` - Convert core::Value to serde_json::Value

**Tests (9 total):**
- ✅ Export completed run
- ✅ Export failed run (includes error message)
- ✅ Export cancelled run
- ✅ Export archived run
- ✅ Reject export of active run
- ✅ Reject export of paused run
- ✅ Reject export of non-existent run
- ✅ Export with tags preserved
- ✅ Verify bundle is verifiable (checksums valid)

### Phase 6: Database Import API (Medium Effort) ✅ COMPLETE

**Files modified:**
- `crates/primitives/src/run_index.rs` - Add `import_run` method

> **Implementation Note:** Like export_run, import_run is on RunIndex rather than Database
> to avoid circular dependency. The original run_id from the bundle is preserved to
> ensure data written to storage matches the run_id in the imported entries.

**MVP Constraint:** Import into empty database only. No conflict handling.

**Tasks:**
1. ✅ Add `import_run` method to `RunIndex`:
   ```rust
   impl RunIndex {
       /// Import a run from a bundle into this database
       pub fn import_run(&self, path: &Path) -> RunBundleResult<ImportedRunInfo>;
   }
   ```

2. ✅ Add helper methods:
   - `replay_imported_entries()` - Replay WAL entries to storage and WAL
   - `create_imported_run()` - Create run metadata with preserved run_id

3. ✅ Handles all WAL entry types:
   - Write/Delete (KV operations)
   - JsonCreate/JsonSet/JsonDelete/JsonDestroy (JSON operations)
   - Vector operations (counted but not fully replayed in MVP)

**Tests (7 total):**
- ✅ Import into empty database
- ✅ Import fails if run already exists
- ✅ Import preserves tags
- ✅ Import preserves failed run error message
- ✅ Import cancelled run
- ✅ Import archived run
- ✅ Round-trip: export → import → verify data matches

### Phase 7: Bundle Verification (Low Effort) ✅ COMPLETE

**Files modified:**
- `crates/primitives/src/run_index.rs` - Added `verify_bundle` method (convenience wrapper)

**Implementation Notes:**
- `verify_bundle` is a thin wrapper around `RunBundleReader::validate()`
- Located on `RunIndex` (consistent with export_run/import_run)
- Returns `BundleVerifyInfo` with run_id, format_version, wal_entry_count, checksums_valid

**Tasks Completed:**
1. ✅ Added `verify_bundle` method to `RunIndex`:
   ```rust
   impl RunIndex {
       /// Verify a bundle's integrity without importing
       pub fn verify_bundle(&self, path: &Path) -> RunBundleResult<BundleVerifyInfo> {
           RunBundleReader::validate(path)
       }
   }
   ```

**Tests (12 total):**
- ✅ test_verify_valid_bundle - Valid bundle passes verification
- ✅ test_verify_returns_entry_count - Entry count matches export
- ✅ test_verify_nonexistent_file - Missing file returns error
- ✅ test_verify_truncated_archive - Truncated archive fails
- ✅ test_verify_corrupted_archive - Corrupted data detected
- ✅ test_verify_empty_file - Empty file fails
- ✅ test_verify_random_data - Random bytes fail
- ✅ test_verify_does_not_import - Verify doesn't modify database
- ✅ test_verify_then_import_workflow - Full verify→import workflow
- ✅ test_verify_large_bundle - Works with 100+ entries
- ✅ test_verify_multiple_bundles - Multiple bundles independent
- ✅ test_verify_bundle_with_failed_run - Failed runs work

**Post-MVP:**
- `diff_bundles(a, b)` - compare two bundles
- `verify_bundle_replay(path)` - import to temp DB, re-export, compare

---

## 5. File Changes Summary

### New Files (MVP)

| File | Purpose |
|------|---------|
| `crates/durability/src/run_bundle/mod.rs` | Module root, re-exports |
| `crates/durability/src/run_bundle/types.rs` | Core types: Manifest, RunInfo |
| `crates/durability/src/run_bundle/error.rs` | RunBundleError enum |
| `crates/durability/src/run_bundle/wal_log.rs` | WAL.runlog reader/writer |
| `crates/durability/src/run_bundle/writer.rs` | Archive writer (tar.zst) |
| `crates/durability/src/run_bundle/reader.rs` | Archive reader (tar.zst) |

### Modified Files (MVP)

| File | Changes |
|------|---------|
| `crates/durability/Cargo.toml` | Add `tar`, `zstd`, `xxhash-rust` deps |
| `crates/durability/src/lib.rs` | Export `run_bundle` module |
| `crates/engine/src/database.rs` | Add `export_run`, `verify_bundle`, `import_run` |
| `crates/engine/src/lib.rs` | Export run bundle types |

### Post-MVP Files

| File | Purpose |
|------|---------|
| `crates/durability/src/wal.rs` | Add `with_remapped_run_id` (for NewRunId strategy) |
| `crates/core/src/types.rs` | Add `Key::with_remapped_run_id` |
| `crates/durability/src/run_bundle/snapshot.rs` | Snapshot section writer/reader |
| `crates/durability/src/run_bundle/index.rs` | Writeset index writer/reader |

---

## 6. Test Strategy

### Unit Tests (MVP)

- Manifest JSON round-trip serialization
- RunInfo JSON round-trip serialization
- WAL.runlog binary format read/write
- WAL filtering by run_id
- xxh3 checksum calculation
- Zstd compression/decompression
- Tar archive structure

### Integration Tests (`tests/run_bundle/`)

| Test File | MVP | Coverage |
|-----------|-----|----------|
| `export.rs` | ✅ | Export terminal runs, reject active/paused |
| `verify.rs` | ✅ | Validate bundle integrity, detect corruption |
| `import.rs` | ✅ | Import into empty database |
| `round_trip.rs` | ✅ | Export → Import → Verify data matches |
| `corruption.rs` | ✅ | Detect corrupted/truncated archives |
| `conflict.rs` | | Import conflict handling (post-MVP) |
| `large_runs.rs` | | Performance with many WAL entries |

### MVP Test Scenarios

```
1. Export completed run → verify bundle → import into fresh DB → data matches
2. Export failed run → error message preserved in RUN.json
3. Export active run → rejected with NotTerminal error
4. Verify corrupted bundle → ChecksumMismatch error
5. Import into DB with existing run → RunAlreadyExists error
```

### CLI Verification

Bundles should be inspectable with standard tools:
```bash
# List contents
tar -tf run.runbundle.tar.zst

# Extract for debugging
tar -xf run.runbundle.tar.zst
cat runbundle/RUN.json | jq .

# Check compression ratio
zstd -l run.runbundle.tar.zst
```

### Post-MVP Tests

- Import with NewRunId conflict strategy
- Snapshot acceleration vs WAL-only replay
- Large runs (streaming, no OOM)
- Property tests for arbitrary WAL sequences

---

## 7. API Surface

### MVP Public API (Database level)

```rust
impl Database {
    /// Export a terminal run as a portable bundle
    pub fn export_run(
        &self,
        run_id: &RunId,
        path: &Path,
    ) -> Result<RunExportInfo, RunBundleError>;

    /// Verify a bundle's integrity without importing
    pub fn verify_bundle(
        path: &Path,
    ) -> Result<BundleVerifyInfo, RunBundleError>;

    /// Import a run from a bundle into an empty database
    /// Fails if run_id already exists.
    pub fn import_run(
        &self,
        path: &Path,
    ) -> Result<ImportedRunInfo, RunBundleError>;
}
```

### MVP Types (exported from engine)

```rust
pub use durability::run_bundle::{
    RunExportInfo,
    BundleVerifyInfo,
    ImportedRunInfo,
    RunBundleError,
};
```

### Post-MVP API Additions

```rust
impl Database {
    /// Export with options (compression level, snapshot inclusion)
    pub fn export_run_with_options(..., options: ExportOptions) -> ...;

    /// Import with conflict handling options
    pub fn import_run_with_options(..., options: ImportOptions) -> ...;
}

pub enum ImportConflictStrategy { Reject, NewRunId }
pub struct ImportOptions { pub on_conflict: ImportConflictStrategy }
```

---

## 8. Compression Strategy

### Approach: Stream Compression

```
[tar archive] → [zstd compress] → file.runbundle.tar.zst
```

- **Algorithm**: zstd (level 3 default, configurable)
- **Scope**: Entire tar as single stream
- **Rationale**: Best compression ratio from cross-file redundancy

### Why Stream Compression?

| Factor | Stream (.tar.zst) | Per-file | Uncompressed |
|--------|-------------------|----------|--------------|
| Compression ratio | Best | Good | None |
| Random access | No* | Yes | Yes |
| Implementation | Simple | Medium | Simplest |
| Cloud upload bandwidth | Optimal | Good | Poor |

*Modern `tar` handles `.tar.zst` transparently - users can still list/extract:
```bash
tar -tf run.runbundle.tar.zst                      # list contents
tar -xf run.runbundle.tar.zst runbundle/RUN.json   # extract single file
```

### Cloud Readiness (Strata Cloud)

The bundle format is designed to support a future "GitHub for Strata runs" cloud service:

**Cloud workflow:**
1. User uploads `.runbundle.tar.zst`
2. Cloud decompresses once on ingest
3. Parses `MANIFEST.json` + `RUN.json` for indexing
4. Stores metadata in database (search, browse, filter)
5. Stores bundle/components in object storage
6. Re-streams compressed on download

**Format advantages for cloud:**
- Metadata front-loaded (`MANIFEST.json`, `RUN.json`) - parseable without full extraction
- Searchable fields: `run_id`, `name`, `state`, `tags`, `created_at`, `error`
- Checksums in manifest enable integrity verification
- Single-file artifact simplifies upload/download UX

**Future cloud optimizations (not in MVP):**
- Content-addressed blob storage for deduplication across runs
- Chunked uploads for resumability
- Server-side bundle inspection without full download
- Incremental sync for run forks

---

## 9. Non-Goals (MVP)

Per RUN_BUNDLE.md architectural constraints:

- **No streaming export** - Bundle is created atomically after run closes
- **No incremental sync** - Each export is complete and independent
- **No background upload** - Export is synchronous, explicit
- **No encryption** - Layer on top if needed (bundle is a regular file)
- **No remote import** - Import from local path only
- **No WAL tailing** - Only closed runs can be exported

**Included in tar.zst format:**
- ✅ **Compression** - zstd provides excellent ratio with fast decompression
- ✅ **Streaming read** - tar format supports streaming extraction

---

## 10. Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Bundle format vs database export? | Separate format (`.runbundle.tar.zst`) |
| Archive format? | `tar.zst` - streamable, inspectable, compressed |
| Compression strategy? | Stream compression (entire tar), zstd level 3 default |
| What to include? | WAL (required) + snapshot (optional) + index (optional) |
| Import ID handling? | Preserve by default, option for NewRunId |
| Which states exportable? | Terminal only (Completed, Failed, Cancelled, Archived) |
| API location? | Database level |
| Checksum algorithm? | xxh3 (fast, high quality) |
| Cloud readiness? | Format supports future Strata Cloud (metadata front-loaded) |

---

## 11. Implementation Order

```
Phase 1: Core Types & Dependencies
    ↓
Phase 2: WAL.runlog Writer/Reader
    ↓
Phase 3: Archive Writer (tar.zst)
    ↓
Phase 4: Archive Reader (tar.zst)
    ↓
Phase 5: Database Export API
    ↓
Phase 6: Database Import API
    ↓
Phase 7: Verification Utilities
```

### Dependency Graph

```
                    ┌─────────────────┐
                    │ Phase 1: Types  │
                    └────────┬────────┘
                             │
              ┌──────────────┼──────────────┐
              ▼              ▼              │
    ┌─────────────────┐  ┌─────────────────┐│
    │ Phase 2: WAL.log│  │ (can start     ││
    └────────┬────────┘  │  concurrently) ││
             │           └─────────────────┘│
             ▼                              │
    ┌─────────────────┐                     │
    │ Phase 3: Writer │◄────────────────────┘
    └────────┬────────┘
             │
             ▼
    ┌─────────────────┐
    │ Phase 4: Reader │
    └────────┬────────┘
             │
    ┌────────┴────────┐
    ▼                 ▼
┌─────────┐     ┌─────────┐
│Phase 5: │     │Phase 6: │
│ Export  │     │ Import  │
└────┬────┘     └────┬────┘
     │               │
     └───────┬───────┘
             ▼
    ┌─────────────────┐
    │ Phase 7: Verify │
    └─────────────────┘
```

---

## 12. Success Criteria

### MVP Success Criteria

1. **Export works**: Terminal runs can be exported to `.runbundle.tar.zst`
2. **Verify works**: Bundle integrity can be validated before import
3. **Import works**: Bundle can be imported into empty database
4. **Round-trip correctness**: Export → Import produces identical logical state
5. **Determinism**: Same run exported twice produces byte-identical bundles
6. **Inspectable**: `tar -tf` shows contents, `jq` reads metadata

### Post-MVP Success Criteria

7. **Conflict handling**: Import into non-empty database with NewRunId option
8. **Snapshot acceleration**: Faster import when snapshot present
9. **Streamable**: Large runs don't require holding entire bundle in memory

---

## 13. Related Documents

- `docs/architecture/RUN_BUNDLE.md` - Architectural design
- `docs/architecture/M10_ARCHITECTURE.md` - Storage backend context
- `docs/architecture/M7_ARCHITECTURE.md` - Durability model
- `docs/defects/RUNINDEX_DEFECTS.md` - Run lifecycle reference
