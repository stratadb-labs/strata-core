# Strata-Wire Removal

> **Status**: Approved for Removal
> **Date**: 2026-01-25
> **Reason**: Architectural mismatch - wire protocol designed for non-existent server

---

## Summary

The `strata-wire` crate should be removed from the codebase. It implements a wire protocol (request/response envelope, RPC-style message format) for a client-server architecture that Strata does not have and does not need.

---

## Background

### What strata-wire provides

1. **Value ↔ JSON encoding** with special wrappers (`$bytes`, `$f64`, `$absent`)
2. **Request/Response envelope** (`{id, op, params}` / `{id, ok, result}`)
3. **Version/Versioned encoding**
4. **Error encoding**

### Why it was created

The crate was designed anticipating a future where Strata might have:
- A network server (TCP, HTTP, WebSocket)
- Remote clients
- RPC-style communication

### Why it's not needed

Strata is an **embedded database**. All API surfaces are in-process:

| Surface | Integration Method | Wire Protocol? |
|---------|-------------------|----------------|
| Rust | Direct library calls | No |
| Python | PyO3 FFI bindings | No |
| Node | napi-rs FFI bindings | No |
| CLI | Binary using Rust lib | No (JSON for output only) |
| MCP | JSON-RPC (standard) | MCP defines its own format |

**There is no server. There are no remote clients. There is no network transport.**

The Request/Response envelope format is designed for RPC communication that will never happen.

---

## Implementation Issues Found

Beyond architectural mismatch, the implementation has significant bugs:

### 1. Request Parameter Decoding is Broken

```rust
// envelope.rs:172-175
// Always returns Generic, never parses KvGet or KvSet!
let params = match obj.get("params") {
    Some(v) => RequestParams::Generic(v.clone()),
    None => RequestParams::Generic(Value::Object(HashMap::new())),
};
```

The typed `RequestParams` variants (`KvGet`, `KvSet`) are defined but never decoded - all requests decode to `Generic(Value)`.

### 2. Large Integer Precision Loss

```rust
// decode.rs:274-283
// Numbers > i64::MAX become Float, losing precision
if let Ok(i) = num_str.parse::<i64>() {
    Ok(Value::Int(i))
} else {
    num_str.parse::<f64>().map(Value::Float)  // Precision loss!
}
```

### 3. No Recursion Depth Limit

The JSON parser has no depth limit, risking stack overflow on malicious input.

---

## What Replaces It

### For JSON Serialization

The `strata-executor` crate (M13) will include minimal JSON utilities for:
- CLI output formatting
- MCP payload serialization
- Debug/logging

This will be a simple utility module, not a wire protocol.

### For API Surfaces

The Command Execution Layer (`strata-executor`) provides:
- Canonical `Command` enum for all operations
- Canonical `Output` and `Error` types
- Single `Executor` that enforces all invariants

All API surfaces (Rust, Python, Node, CLI, MCP) build `Command` values and call the executor.

---

## Removal Checklist

### Files to Delete

```
crates/wire/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── json/
        ├── mod.rs
        ├── encode.rs
        ├── decode.rs
        ├── envelope.rs
        ├── version.rs
        └── error.rs
```

### Workspace Updates

**Cargo.toml** (workspace root):
```diff
 members = [
     "crates/core",
     "crates/storage",
     "crates/concurrency",
     "crates/durability",
     "crates/primitives",
     "crates/engine",
     "crates/api",
     "crates/search",
-    "crates/wire",
 ]
```

### Documentation Updates

| File | Action |
|------|--------|
| `PRODUCTION_READINESS_REPORT.md` | Remove strata_wire references |
| `TESTING_AUDIT_REPORT.md` | Remove strata-wire section |
| `docs/TEST_REPORT.md` | Remove strata-wire row |
| `docs/milestones/M12_UNIFIED_API_PLAN.md` | Remove from internal crates list |

---

## Salvageable Code

The following code patterns from strata-wire may be useful in strata-executor:

1. **Base64 encoding for bytes** - Standard approach, easy to reimplement
2. **Special float handling** (NaN, ±Inf, -0.0) - Useful for JSON output
3. **Deterministic object key ordering** - Good for reproducible output

These are simple utilities (~50 lines) that can be reimplemented cleanly in the executor's JSON module.

---

## Migration

No migration needed - strata-wire has zero dependents in the codebase:

```
$ grep -r "strata-wire\|strata_wire" crates/*/Cargo.toml
crates/wire/Cargo.toml:name = "strata-wire"
```

Only the wire crate itself references the name. No other crate depends on it.

---

## Approval

- [x] Architectural review: Wire protocol not needed for embedded database
- [x] Dependency check: No crates depend on strata-wire
- [x] Replacement plan: strata-executor (M13) provides better abstraction
- [x] Documentation: Updates identified

**Decision**: Remove `crates/wire/` entirely.
