# Branch Bundles Guide

Branch bundles let you export a branch as a portable archive file (`.branchbundle.tar.zst`) and import it into another database instance.

## Use Cases

- **Backup** — export a branch before deleting it
- **Transfer** — move a branch between machines
- **Debugging** — share a branch's state for reproduction
- **Archival** — compress and store completed branches

## Export

Export a branch to a bundle file:

```rust
let result = db.branch_export("my-branch", "./exports/my-branch.branchbundle.tar.zst")?;
println!("Exported branch: {}", result.branch_id);
println!("Entries: {}", result.entry_count);
println!("Size: {} bytes", result.bundle_size);
```

The export creates a compressed tar archive containing:
- `MANIFEST.json` — format version and file checksums
- `BRANCH.json` — branch metadata (ID, status, tags, timestamps)
- `WAL.branchlog` — all WAL entries for that branch

## Import

Import a bundle into the current database:

```rust
let result = db.branch_import("./exports/my-branch.branchbundle.tar.zst")?;
println!("Imported branch: {}", result.branch_id);
println!("Transactions applied: {}", result.transactions_applied);
println!("Keys written: {}", result.keys_written);
```

The imported branch becomes available immediately. You can switch to it with `set_branch()`.

## Validate

Check a bundle's integrity without importing:

```rust
let result = db.branch_validate_bundle("./exports/my-branch.branchbundle.tar.zst")?;
println!("Branch ID: {}", result.branch_id);
println!("Format version: {}", result.format_version);
println!("Entry count: {}", result.entry_count);
println!("Checksums valid: {}", result.checksums_valid);
```

## Bundle Format

Bundles use the `.branchbundle.tar.zst` format — a zstd-compressed tar archive:

```
<branch_id>.branchbundle.tar.zst
  branchbundle/
    MANIFEST.json     # Format version, xxh3 checksums
    BRANCH.json          # Branch metadata
    WAL.branchlog     # Binary WAL entries with per-entry CRC32
```

### WAL.branchlog Format

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
| `BranchNotFound` | The specified branch doesn't exist |
| `BranchAlreadyExists` | A branch with the same ID already exists in the target database |
| `InvalidBundle` | Malformed archive |
| `ChecksumMismatch` | Integrity check failed |
| `UnsupportedVersion` | Unknown bundle format version |

## Next

- [Error Handling](error-handling.md) — error categories and patterns
- [Branch Management](branch-management.md) — creating and managing branches
