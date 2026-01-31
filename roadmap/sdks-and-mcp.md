# SDKs and MCP Server

**Theme**: Thin client SDKs and an MCP server, all powered by the Command/Output protocol.

## Why the SDKs are boring (by design)

The `strata-executor` crate defines a `Command` enum (53 variants) and an `Output` enum. Every operation in StrataDB — across all six primitives — is a serializable command in, serializable output out. There is no business logic outside this boundary.

This means every SDK is the same pattern:

1. Accept typed method call from the user
2. Serialize it into a `Command`
3. Send it to the engine (FFI, network, or stdio)
4. Deserialize the `Output`
5. Return typed result to the user

No SDK reimplements validation, transaction coordination, branch resolution, or error handling. That all lives in the executor.

## Python SDK

### Transport: PyO3 FFI

Embed the Rust library directly in a Python native extension via PyO3. No network hop, no serialization overhead beyond the Python↔Rust boundary.

```python
from stratadb import Strata

db = Strata.open("/path/to/data")
db.kv_put("user:123", "Alice")
value = db.kv_get("user:123")

db.create_branch("experiment")
db.set_branch("experiment")

results = db.search("what tools were used?")
```

### Scope

- Pythonic API: context managers for transactions, iterators for list/scan operations, type hints throughout
- 1:1 mapping to the Strata public API — no Python-only features
- NumPy integration for vector operations (accept/return `np.ndarray` for embeddings)
- pip-installable with prebuilt wheels (manylinux, macOS, Windows)
- Async variant via `asyncio` wrappers (if demand warrants it)

### Crate

```
sdks/python/
├── Cargo.toml        # PyO3 dependency, links strata-executor
├── src/
│   └── lib.rs        # PyO3 module: #[pyclass] Strata, #[pymethods]
├── python/
│   └── stratadb/
│       ├── __init__.py
│       └── py.typed   # PEP 561 marker
├── pyproject.toml
└── tests/
    └── test_strata.py
```

## Node.js SDK

### Transport: NAPI-RS FFI

Same approach as Python — embed the Rust library as a native addon via NAPI-RS. No network hop.

```typescript
import { Strata } from 'stratadb';

const db = await Strata.open('/path/to/data');
await db.kvPut('user:123', 'Alice');
const value = await db.kvGet('user:123');

await db.createBranch('experiment');
await db.setBranch('experiment');

const results = await db.search('what tools were used?');
```

### Scope

- TypeScript-first: full type definitions, no `any`
- Async/await API (Node native addons run on the libuv thread pool)
- camelCase method names (JS convention) mapping to snake_case Rust methods
- `Buffer` support for binary values
- npm-installable with prebuilt binaries (via `@napi-rs/cli`)

### Package

```
sdks/node/
├── Cargo.toml          # napi-rs dependency, links strata-executor
├── src/
│   └── lib.rs          # NAPI module
├── index.d.ts          # TypeScript definitions
├── package.json
└── __tests__/
    └── strata.test.ts
```

## MCP Server

### What

A Model Context Protocol server that exposes StrataDB as a tool provider for AI agents (Claude, GPT, etc.). The agent calls tools like `kv_put`, `search`, `create_branch` through the MCP protocol, and the server translates them into `Command` variants.

### Transport: stdio

MCP servers communicate over stdin/stdout with JSON-RPC. The server is a small Rust binary that:

1. Opens a StrataDB instance
2. Reads JSON-RPC requests from stdin
3. Maps MCP tool calls to `Command` variants
4. Executes via the executor
5. Maps `Output` to JSON-RPC responses on stdout

### Tool definitions

Each public Strata method becomes an MCP tool:

```json
{
  "name": "kv_put",
  "description": "Store a key-value pair in the current branch",
  "parameters": {
    "key": { "type": "string" },
    "value": { "type": "string" }
  }
}
```

The full tool set mirrors the Strata public API. Tools are grouped by primitive for discoverability.

### Scope

- All 6 primitives exposed as tools
- Branch management tools (create, switch, list, delete)
- Search tool (natural language, backed by intelligence layer)
- Transaction tools (begin, commit, rollback)
- Read-only mode support (via `strata-security` AccessMode)

### Binary

```
sdks/mcp/
├── Cargo.toml          # depends on strata-executor, serde_json
├── src/
│   ├── main.rs         # stdio loop, JSON-RPC dispatch
│   ├── tools.rs        # MCP tool definitions
│   └── convert.rs      # MCP params ↔ Command/Output mapping
└── tests/
    └── mcp_tests.rs
```

## Shared testing strategy

All three SDKs and the MCP server are thin wrappers around the same executor. The test strategy reflects this:

- **Rust-side correctness**: Already covered by the existing 2,500+ test suite
- **SDK tests**: Verify the binding layer works — types cross the FFI boundary correctly, errors propagate, async behaves properly
- **Black-box tests**: The same black-box test scenarios can be implemented in Python, TypeScript, and MCP tool calls to verify parity across all interfaces

## Dependencies

- `strata-executor` public API must be stable before SDKs ship (breaking API changes ripple into every SDK)
- `strata-security` (SDKs should support `AccessMode` from day one)
- For MCP: no additional dependencies beyond what the executor already provides
- For Python: PyO3, maturin (build tool)
- For Node: napi-rs, @napi-rs/cli (build tool)

## Ordering

1. **MCP server first** — smallest surface area, highest immediate value for AI agent use cases, validates the Command/Output serialization path
2. **Python SDK second** — largest potential user base (AI/ML ecosystem)
3. **Node SDK third** — web and server-side JS ecosystem
