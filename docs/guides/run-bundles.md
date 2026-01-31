# Run Bundles Guide

Run bundles let you export a run as a portable archive file (`.runbundle.tar.zst`) and import it into another database instance.

## Use Cases

- **Backup** — export a run before deleting it
- **Transfer** — move a run between machines
- **Debugging** — share a run's state for reproduction
- **Archival** — compress and store completed runs

## Export

Export a run to a bundle file:

```rust
let result = db.run_export("my-run", "./exports/my-run.runbundle.tar.zst")?;
println!("Exported run: {}", result.run_id);
println!("Entries: {}", result.entry_count);
println!("Size: {} bytes", result.bundle_size);
```

The export creates a compressed tar archive containing:
- `MANIFEST.json` — format version and file checksums
- `RUN.json` — run metadata (ID, status, tags, timestamps)
- `WAL.runlog` — all WAL entries for that run

## Import

Import a bundle into the current database:

```rust
let result = db.run_import("./exports/my-run.runbundle.tar.zst")?;
println!("Imported run: {}", result.run_id);
println!("Transactions applied: {}", result.transactions_applied);
println!("Keys written: {}", result.keys_written);
```

The imported run becomes available immediately. You can switch to it with `set_run()`.

## Validate

Check a bundle's integrity without importing:

```rust
let result = db.run_validate_bundle("./exports/my-run.runbundle.tar.zst")?;
println!("Run ID: {}", result.run_id);
println!("Format version: {}", result.format_version);
println!("Entry count: {}", result.entry_count);
println!("Checksums valid: {}", result.checksums_valid);
```

## Bundle Format

Bundles use the `.runbundle.tar.zst` format — a zstd-compressed tar archive:

```
<run_id>.runbundle.tar.zst
  runbundle/
    MANIFEST.json     # Format version, xxh3 checksums
    RUN.json          # Run metadata
    WAL.runlog        # Binary WAL entries with per-entry CRC32
```

### WAL.runlog Format

```
Header (16 bytes):
  Magic: "STRATA_WAL" (10 bytes)
  Version: u16 (2 bytes)
  Entry Count: u32 (4 bytes)

Per entry:
  Length: u32 (4 bytes)
  Data: bincode-serialized WALEntry
  CRC32: u32 (4 bytes)
```

## Errors

| Error | Cause |
|-------|-------|
| `RunNotFound` | The specified run doesn't exist |
| `RunAlreadyExists` | A run with the same ID already exists in the target database |
| `InvalidBundle` | Malformed archive |
| `ChecksumMismatch` | Integrity check failed |
| `UnsupportedVersion` | Unknown bundle format version |

## Next

- [Error Handling](error-handling.md) — error categories and patterns
- [Run Management](run-management.md) — creating and managing runs
