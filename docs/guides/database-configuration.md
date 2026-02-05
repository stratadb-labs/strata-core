# Database Configuration Guide

This guide covers the different ways to open a StrataDB database and configure its behavior.

## Opening Methods

### Ephemeral (In-Memory)

For testing and development. No files are created on disk.

```bash
strata --cache
```

### Persistent

Creates or opens a database at the specified path:

```bash
strata --db /path/to/data
```

If the directory doesn't exist, it is created. If a database already exists at that path, it is opened and any WAL entries are replayed for recovery.

## Database Operations

### Ping

Verify the database is responsive:

```bash
strata --cache ping
```

Output:

```
PONG
```

### Info

Get database statistics:

```
$ strata --cache
strata:default/default> info
version: 0.1.0
branches: 1
total_keys: 0
```

Or from the shell:

```bash
strata --cache info
```

### Flush

Force pending writes to disk (relevant in Buffered durability mode):

```bash
strata --db ./data flush
```

### Compact

Trigger storage compaction:

```bash
strata --db ./data compact
```

## Shell Flags

The CLI supports several flags for controlling behavior:

| Flag | Description |
|------|-------------|
| `--db <path>` | Open a persistent database at the given path |
| `--cache` | Open an ephemeral in-memory database |
| `--branch <name>` | Set the active branch (default: `default`) |
| `--space <name>` | Set the active space (default: `default`) |
| `--json` | Output results as JSON |
| `--raw` | Output raw values without formatting |
| `--read-only` | Open in read-only mode |

### Examples

```bash
# Persistent database with specific branch
strata --db ./data --branch experiment kv get config

# Ephemeral with JSON output
strata --cache --json kv get key

# Read-only access
strata --db ./data --read-only kv list
```

## Next

- [Branch Bundles](branch-bundles.md) — exporting and importing branches
- [Error Handling](error-handling.md) — error categories
