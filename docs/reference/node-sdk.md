# Node.js SDK Reference

The StrataDB Node.js SDK provides native bindings via NAPI-RS with full TypeScript support. All data methods are async and return Promises.

## Installation

```bash
npm install @stratadb/core
```

## Quick Start

```typescript
import { Strata } from '@stratadb/core';

// Open a database
const db = Strata.open('/path/to/data');

// Store and retrieve data
await db.kv.set('greeting', 'Hello, World!');
console.log(await db.kv.get('greeting'));  // "Hello, World!"

// Transaction with auto-commit/rollback
await db.transaction(async (tx) => {
  await tx.kv.set('a', 1);
  await tx.kv.set('b', 2);
});

// Time-travel reads
const range = await db.timeRange();
const snapshot = db.at(range.oldestTs);
console.log(await snapshot.kv.get('greeting'));  // value at that time

// Vector search
await db.vector.createCollection('embeddings', { dimension: 384 });
await db.vector.upsert('embeddings', 'doc-1', new Array(384).fill(0.1));
const results = await db.vector.search('embeddings', new Array(384).fill(0.1), { limit: 5 });

// Close when done
await db.close();
```

---

## Opening a Database

### Strata.open(path, options?)

Open a database at the given path. Synchronous.

```typescript
const db = Strata.open('/path/to/data');
const db = Strata.open('/path/to/data', { autoEmbed: true });
const db = Strata.open('/path/to/data', { readOnly: true });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | string | Path to the database directory |
| `options` | OpenOptions? | Optional settings |

**OpenOptions:**
| Name | Type | Description |
|------|------|-------------|
| `autoEmbed` | boolean? | Enable automatic text embedding |
| `readOnly` | boolean? | Open in read-only mode |

**Returns:** `Strata` instance

**Throws:** `IoError` if the database cannot be opened

### Strata.cache()

Create an ephemeral in-memory database. Synchronous.

```typescript
const db = Strata.cache();
```

**Returns:** `Strata` instance

---

## KV Store — `db.kv`

### kv.set(key, value)

Store a key-value pair.

```typescript
const version = await db.kv.set('user:123', { name: 'Alice', age: 30 });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | The key |
| `value` | JsonValue | The value |

**Returns:** `Promise<number>` — Version number

### kv.get(key, options?)

Get a value by key.

```typescript
const value = await db.kv.get('user:123');

// Time-travel read
const value = await db.kv.get('user:123', { asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | The key |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<JsonValue>` — Value or `null` if not found

### kv.delete(key)

Delete a key.

```typescript
const deleted = await db.kv.delete('user:123');
```

**Returns:** `Promise<boolean>` — True if the key existed

### kv.keys(options?)

List keys with optional prefix filter and pagination.

```typescript
const allKeys = await db.kv.keys();
const userKeys = await db.kv.keys({ prefix: 'user:' });
const page = await db.kv.keys({ prefix: 'user:', limit: 100 });
const past = await db.kv.keys({ prefix: 'user:', asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `options.prefix` | string? | Filter keys by prefix |
| `options.limit` | number? | Maximum keys to return |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<string[]>`

### kv.history(key)

Get version history for a key.

```typescript
const history = await db.kv.history('user:123');
if (history) {
  for (const entry of history) {
    console.log(`v${entry.version}: ${JSON.stringify(entry.value)}`);
  }
}
```

**Returns:** `Promise<VersionedValue[] | null>`

### kv.getVersioned(key)

Get a value with version and timestamp info.

```typescript
const result = await db.kv.getVersioned('user:123');
if (result) {
  console.log(`Value: ${result.value}, Version: ${result.version}`);
}
```

**Returns:** `Promise<VersionedValue | null>`

---

## State Cell — `db.state`

### state.set(cell, value)

Set a state cell value (unconditional write).

```typescript
const version = await db.state.set('counter', 0);
```

**Returns:** `Promise<number>` — Version number

### state.get(cell, options?)

Get a state cell value.

```typescript
const value = await db.state.get('counter');
const past = await db.state.get('counter', { asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `cell` | string | The cell name |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<JsonValue>` — Value or `null`

### state.init(cell, value)

Initialize a state cell only if it doesn't exist.

```typescript
const version = await db.state.init('counter', 0);
```

**Returns:** `Promise<number>` — Version number

### state.cas(cell, newValue, options?)

Compare-and-swap update.

```typescript
const newVersion = await db.state.cas('counter', 10, { expectedVersion: 5 });
if (newVersion === null) {
  console.log('CAS failed - version mismatch');
}
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `cell` | string | The cell name |
| `newValue` | JsonValue | The new value |
| `options.expectedVersion` | number? | Expected current version |

**Returns:** `Promise<number | null>` — New version or null if CAS failed

### state.delete(cell)

Delete a state cell.

```typescript
const deleted = await db.state.delete('counter');
```

**Returns:** `Promise<boolean>`

### state.keys(options?)

List state cell names.

```typescript
const cells = await db.state.keys();
const configCells = await db.state.keys({ prefix: 'config:' });
const past = await db.state.keys({ prefix: 'config:', asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `options.prefix` | string? | Filter by prefix |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<string[]>`

### state.history(cell)

Get version history for a state cell.

```typescript
const history = await db.state.history('counter');
```

**Returns:** `Promise<VersionedValue[] | null>`

### state.getVersioned(cell)

Get a state cell with version info.

```typescript
const result = await db.state.getVersioned('counter');
```

**Returns:** `Promise<VersionedValue | null>`

---

## Event Log — `db.events`

### events.append(eventType, payload)

Append an event to the log.

```typescript
const seq = await db.events.append('user_action', { action: 'click', target: 'button' });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `eventType` | string | The event type |
| `payload` | JsonValue | The event payload |

**Returns:** `Promise<number>` — Sequence number

### events.get(sequence, options?)

Get an event by sequence number.

```typescript
const event = await db.events.get(0);
const past = await db.events.get(0, { asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `sequence` | number | The sequence number |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<VersionedValue | null>`

### events.list(eventType, options?)

List events by type with optional pagination.

```typescript
const events = await db.events.list('user_action');
const page = await db.events.list('user_action', { limit: 100, after: 500 });
const past = await db.events.list('user_action', { asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `eventType` | string | The event type |
| `options.limit` | number? | Maximum events |
| `options.after` | number? | Return events after this sequence |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<VersionedValue[]>`

### events.count()

Get total event count.

```typescript
const count = await db.events.count();
```

**Returns:** `Promise<number>`

---

## JSON Store — `db.json`

### json.set(key, path, value)

Set a value at a JSONPath.

```typescript
await db.json.set('user:123', '$', { name: 'Alice', age: 30 });
await db.json.set('user:123', '$.email', 'alice@example.com');
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | Document key |
| `path` | string | JSONPath (use `$` for root) |
| `value` | JsonValue | The value |

**Returns:** `Promise<number>` — Version number

### json.get(key, path, options?)

Get a value at a JSONPath.

```typescript
const doc = await db.json.get('user:123', '$');
const name = await db.json.get('user:123', '$.name');
const past = await db.json.get('user:123', '$', { asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | Document key |
| `path` | string | JSONPath |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<JsonValue>` — Value or `null`

### json.delete(key, path)

Delete a value at a JSONPath.

```typescript
const count = await db.json.delete('user:123', '$.email');
```

**Returns:** `Promise<number>` — Count of elements deleted

### json.keys(options?)

List JSON document keys with pagination.

```typescript
const result = await db.json.keys();
const filtered = await db.json.keys({ prefix: 'user:', limit: 100 });
const past = await db.json.keys({ prefix: 'user:', asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `options.prefix` | string? | Filter by prefix |
| `options.limit` | number? | Maximum keys (default: 100) |
| `options.cursor` | string? | Pagination cursor |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<JsonListResult>` — `{ keys: string[], cursor?: string }`

### json.history(key)

Get version history for a JSON document.

```typescript
const history = await db.json.history('user:123');
```

**Returns:** `Promise<VersionedValue[] | null>`

### json.getVersioned(key)

Get a JSON document with version info.

```typescript
const result = await db.json.getVersioned('user:123');
```

**Returns:** `Promise<VersionedValue | null>`

---

## Vector Store — `db.vector`

### vector.createCollection(name, options)

Create a vector collection.

```typescript
await db.vector.createCollection('embeddings', { dimension: 384 });
await db.vector.createCollection('images', { dimension: 512, metric: 'euclidean' });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `name` | string | Collection name |
| `options.dimension` | number | Vector dimension |
| `options.metric` | string? | `cosine` (default), `euclidean`, `dot_product` |

**Returns:** `Promise<number>` — Version number

### vector.deleteCollection(name)

Delete a vector collection.

```typescript
const deleted = await db.vector.deleteCollection('embeddings');
```

**Returns:** `Promise<boolean>`

### vector.listCollections()

List all vector collections.

```typescript
const collections = await db.vector.listCollections();
for (const c of collections) {
  console.log(`${c.name}: ${c.count} vectors`);
}
```

**Returns:** `Promise<CollectionInfo[]>`

### vector.stats(collection)

Get detailed collection statistics.

```typescript
const stats = await db.vector.stats('embeddings');
console.log(`Count: ${stats.count}, Memory: ${stats.memoryBytes} bytes`);
```

**Returns:** `Promise<CollectionInfo>`

### vector.upsert(collection, key, vector, options?)

Insert or update a vector.

```typescript
const embedding = new Array(384).fill(0).map(() => Math.random());
await db.vector.upsert('embeddings', 'doc-1', embedding);
await db.vector.upsert('embeddings', 'doc-1', embedding, { metadata: { title: 'Hello' } });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | string | Collection name |
| `key` | string | Vector key |
| `vector` | number[] | Vector embedding |
| `options.metadata` | JsonValue? | Optional metadata |

**Returns:** `Promise<number>` — Version number

### vector.get(collection, key, options?)

Get a vector by key.

```typescript
const result = await db.vector.get('embeddings', 'doc-1');
if (result) {
  console.log(result.embedding);
  console.log(result.metadata);
}

// Time-travel read
const past = await db.vector.get('embeddings', 'doc-1', { asOf: timestamp });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | string | Collection name |
| `key` | string | Vector key |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Returns:** `Promise<VectorData | null>`

### vector.delete(collection, key)

Delete a vector.

```typescript
const deleted = await db.vector.delete('embeddings', 'doc-1');
```

**Returns:** `Promise<boolean>`

### vector.batchUpsert(collection, entries)

Batch insert/update vectors.

```typescript
const versions = await db.vector.batchUpsert('embeddings', [
  { key: 'doc-1', vector: [...], metadata: { title: 'A' } },
  { key: 'doc-2', vector: [...] },
]);
```

**Returns:** `Promise<number[]>` — Version numbers

### vector.search(collection, query, options?)

Search for similar vectors with optional filters and metric override.

```typescript
const matches = await db.vector.search('embeddings', queryVector, { limit: 10 });

// With filters and metric override
const filtered = await db.vector.search('embeddings', queryVector, {
  limit: 10,
  metric: 'euclidean',
  filter: [
    { field: 'category', op: 'eq', value: 'science' },
    { field: 'year', op: 'gte', value: 2020 },
  ],
});

// Time-travel search
const past = await db.vector.search('embeddings', queryVector, {
  limit: 10,
  asOf: timestamp,
});
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | string | Collection name |
| `query` | number[] | Query vector |
| `options.limit` | number? | Number of results (default: 10) |
| `options.metric` | string? | Override distance metric |
| `options.filter` | MetadataFilter[]? | Metadata filters |
| `options.asOf` | number? | Microsecond timestamp for time-travel reads |

**Filter operators:** `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`

**Returns:** `Promise<SearchMatch[]>` — `{ key, score, metadata }`

---

## Branches — `db.branch`

### branch.current()

Get the current branch name.

```typescript
const branch = await db.branch.current();
```

**Returns:** `Promise<string>`

### branch.create(name)

Create a new empty branch.

```typescript
await db.branch.create('experiment');
```

### branch.switch(name)

Switch to a different branch.

```typescript
await db.branch.switch('feature');
```

### branch.list()

List all branches.

```typescript
const branches = await db.branch.list();
```

**Returns:** `Promise<string[]>`

### branch.exists(name)

Check if a branch exists.

```typescript
const exists = await db.branch.exists('feature');
```

**Returns:** `Promise<boolean>`

### branch.get(name)

Get branch metadata.

```typescript
const info = await db.branch.get('default');
if (info) {
  console.log(`Created: ${info.createdAt}, Version: ${info.version}`);
}
```

**Returns:** `Promise<BranchInfo | null>`

### branch.delete(name)

Delete a branch.

```typescript
await db.branch.delete('experiment');
```

### branch.fork(destination)

Fork the current branch with all its data.

```typescript
const result = await db.branch.fork('experiment-copy');
console.log(`Copied ${result.keysCopied} keys`);
```

**Returns:** `Promise<ForkResult>`

### branch.diff(branchA, branchB)

Compare two branches.

```typescript
const diff = await db.branch.diff('default', 'feature');
console.log(`Added: ${diff.summary.totalAdded}`);
```

**Returns:** `Promise<DiffResult>`

### branch.merge(source, options?)

Merge a branch into the current branch.

```typescript
const result = await db.branch.merge('feature');
const result = await db.branch.merge('feature', { strategy: 'last_writer_wins' });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `source` | string | Source branch |
| `options.strategy` | string? | `last_writer_wins` (default) or `strict` |

**Returns:** `Promise<MergeResult>`

### branch.export(branch, path)

Export a branch to a bundle file.

```typescript
const result = await db.branch.export('default', '/tmp/backup.bundle');
console.log(`Exported ${result.entryCount} entries`);
```

**Returns:** `Promise<BranchExportResult>`

### branch.import(path)

Import a branch from a bundle file.

```typescript
const result = await db.branch.import('/tmp/backup.bundle');
console.log(`Imported to branch ${result.branchId}`);
```

**Returns:** `Promise<BranchImportResult>`

### branch.validateBundle(path)

Validate a bundle file without importing.

```typescript
const result = await db.branch.validateBundle('/tmp/backup.bundle');
console.log(`Valid: ${result.checksumsValid}`);
```

**Returns:** `Promise<BundleValidateResult>`

---

## Spaces — `db.space`

### space.current()

Get the current space name.

```typescript
const space = await db.space.current();
```

**Returns:** `Promise<string>`

### space.create(name)

Create a new space.

```typescript
await db.space.create('archive');
```

### space.switch(name)

Switch to a different space.

```typescript
await db.space.switch('conversations');
```

### space.list()

List all spaces.

```typescript
const spaces = await db.space.list();
```

**Returns:** `Promise<string[]>`

### space.exists(name)

Check if a space exists.

```typescript
const exists = await db.space.exists('archive');
```

**Returns:** `Promise<boolean>`

### space.delete(name, options?)

Delete a space.

```typescript
await db.space.delete('old-space');                    // must be empty
await db.space.delete('old-space', { force: true });   // delete even if non-empty
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `name` | string | Space name |
| `options.force` | boolean? | Delete even if non-empty |

---

## Time Travel — `db.at()`

Create an immutable snapshot at a given timestamp. The snapshot provides read-only access to all namespaces as they existed at that point in time. Write operations throw `StateError`.

```typescript
// Get the available time range
const range = await db.timeRange();
console.log(`Data from ${range.oldestTs} to ${range.latestTs}`);

// Create a snapshot and read from it
const snapshot = db.at(range.oldestTs);
const oldValue = await snapshot.kv.get('key');
const oldKeys = await snapshot.kv.keys({ prefix: 'user:' });
const oldState = await snapshot.state.get('counter');
const oldEvents = await snapshot.events.list('clicks');
const oldDoc = await snapshot.json.get('config', '$');
const oldResults = await snapshot.vector.search('embeddings', queryVec, { limit: 5 });

// Writes throw StateError
snapshot.kv.set('key', 'value');  // throws StateError
```

**Available snapshot namespaces:** `kv`, `state`, `events`, `json`, `vector`

### timeRange()

Get the available time range for the current branch.

```typescript
const range = await db.timeRange();
// { oldestTs: number | null, latestTs: number | null }
```

**Returns:** `Promise<TimeRange>`

---

## Transactions — `db.transaction()`

Execute a function inside a transaction with auto-commit on success and auto-rollback on error.

```typescript
// Auto-commit on success
await db.transaction(async (tx) => {
  await tx.kv.set('a', 1);
  await tx.kv.set('b', 2);
});

// Return values from transactions
const result = await db.transaction(async (tx) => {
  await tx.kv.set('key', 'value');
  return 42;
});
console.log(result);  // 42

// Read-only transaction (auto-rollback instead of commit)
const data = await db.transaction(async (tx) => {
  const a = await tx.kv.get('a');
  const b = await tx.kv.get('b');
  return { a, b };
}, { readOnly: true });

// Auto-rollback on error
try {
  await db.transaction(async (tx) => {
    await tx.kv.set('key', 'value');
    throw new Error('something went wrong');
  });
} catch (e) {
  // transaction was rolled back automatically
}
```

### Manual Transaction Control

For advanced use cases, manual transaction methods are still available:

```typescript
await db.begin();
try {
  await db.kv.set('a', 1);
  await db.kv.set('b', 2);
  await db.commit();
} catch (e) {
  await db.rollback();
  throw e;
}

// Transaction info
const info = await db.txnInfo();     // { id, status, startedAt } or null
const active = await db.txnIsActive(); // boolean
```

---

## Database Operations

### ping()

Check database connectivity.

```typescript
const version = await db.ping();
```

**Returns:** `Promise<string>`

### info()

Get database information.

```typescript
const info = await db.info();
console.log(`Version: ${info.version}, Total keys: ${info.totalKeys}`);
```

**Returns:** `Promise<DatabaseInfo>`

### flush()

Flush pending writes to disk.

```typescript
await db.flush();
```

### compact()

Trigger database compaction.

```typescript
await db.compact();
```

### close()

Release all database resources.

```typescript
await db.close();
```

### search(query, options?)

Search across multiple primitives with optional time filtering, query expansion, and reranking.

```typescript
// Basic search
const results = await db.search('hello world', { k: 10, primitives: ['kv', 'json'] });
for (const hit of results) {
  console.log(`${hit.entity} (${hit.primitive}): ${hit.score}`);
}

// Time-scoped search
const recent = await db.search('deployment failures', {
  k: 10,
  timeRange: {
    start: '2026-02-07T00:00:00Z',
    end: '2026-02-09T23:59:59Z',
  },
});

// Keyword-only mode, disable expansion
const keywordResults = await db.search('auth login', { mode: 'keyword', expand: false });

// Force reranking on
const reranked = await db.search('database issues', { rerank: true });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `query` | string | Search query |
| `options.k` | number? | Maximum results (default: 10) |
| `options.primitives` | string[]? | Primitives to search (e.g., `['kv', 'json']`) |
| `options.timeRange` | TimeRangeInput? | `{ start: string, end: string }` ISO 8601 time filter |
| `options.mode` | string? | `'hybrid'` (default) or `'keyword'` |
| `options.expand` | boolean? | Enable query expansion (default: auto) |
| `options.rerank` | boolean? | Enable result reranking (default: auto) |

When `expand` or `rerank` are not specified, they are automatically enabled if a model is configured. Set to `false` to force off, or `true` to force on (silently skipped if no model).

**Returns:** `Promise<SearchHit[]>`

### retentionApply()

Apply the retention policy to expire old data.

```typescript
await db.retentionApply();
```

---

## Error Handling

All errors are typed subclasses of `StrataError` with a machine-readable `code` property:

```typescript
import {
  StrataError,
  NotFoundError,
  ValidationError,
  ConflictError,
  StateError,
  ConstraintError,
  AccessDeniedError,
  IoError,
} from '@stratadb/core';

try {
  await db.vector.search('nonexistent', [1, 0, 0, 0], { limit: 1 });
} catch (err) {
  if (err instanceof NotFoundError) {
    console.log(err.code);     // "NOT_FOUND"
    console.log(err.message);  // human-readable message
  }
}
```

**Error classes:**

| Class | Code | Description |
|-------|------|-------------|
| `NotFoundError` | `NOT_FOUND` | Key, collection, or branch not found |
| `ValidationError` | `VALIDATION` | Invalid input (bad metric, bad path, etc.) |
| `ConflictError` | `CONFLICT` | Version conflict or merge conflict |
| `StateError` | `STATE` | Invalid state (e.g., writing to a snapshot) |
| `ConstraintError` | `CONSTRAINT` | Constraint violation (e.g., dimension mismatch) |
| `AccessDeniedError` | `ACCESS_DENIED` | Operation not permitted |
| `IoError` | `IO` | File system or I/O error |

All error classes extend `StrataError`, which extends `Error`.

---

## Auto-Embedding Setup

To use automatic text embedding for semantic search:

```typescript
import { Strata, setup } from '@stratadb/core';

// Download model files (one-time)
const modelDir = setup();

// Open with auto-embed enabled
const db = Strata.open('/path/to/data', { autoEmbed: true });
```

---

## TypeScript Types

```typescript
type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

interface VersionedValue {
  value: JsonValue;
  version: number;
  timestamp: number;
}

interface CollectionInfo {
  name: string;
  dimension: number;
  metric: string;
  count: number;
  indexType: string;
  memoryBytes: number;
}

interface VectorData {
  key: string;
  embedding: number[];
  metadata?: JsonValue;
  version: number;
  timestamp: number;
}

interface SearchMatch {
  key: string;
  score: number;
  metadata?: JsonValue;
}

interface MetadataFilter {
  field: string;
  op: 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'contains';
  value: JsonValue;
}

interface SearchHit {
  entity: string;
  primitive: string;
  score: number;
  rank: number;
  snippet?: string;
}

interface TimeRangeInput {
  start: string;  // ISO 8601 datetime
  end: string;    // ISO 8601 datetime
}

interface SearchOptions {
  k?: number;
  primitives?: string[];
  timeRange?: TimeRangeInput;
  mode?: 'keyword' | 'hybrid';
  expand?: boolean;
  rerank?: boolean;
}

interface TimeRange {
  oldestTs: number | null;
  latestTs: number | null;
}

interface TransactionInfo {
  id: string;
  status: string;
  startedAt: number;
}

interface BranchInfo {
  id: string;
  status: string;
  createdAt: number;
  updatedAt: number;
  parentId?: string;
  version: number;
  timestamp: number;
}

interface DatabaseInfo {
  version: string;
  uptimeSecs: number;
  branchCount: number;
  totalKeys: number;
}

interface ForkResult {
  source: string;
  destination: string;
  keysCopied: number;
}

interface DiffResult {
  branchA: string;
  branchB: string;
  summary: { totalAdded: number; totalRemoved: number; totalModified: number };
}

interface MergeResult {
  keysApplied: number;
  spacesMerged: number;
  conflicts: { key: string; space: string }[];
}

interface JsonListResult {
  keys: string[];
  cursor?: string;
}

interface BranchExportResult {
  branchId: string;
  path: string;
  entryCount: number;
  bundleSize: number;
}

interface BranchImportResult {
  branchId: string;
  transactionsApplied: number;
  keysWritten: number;
}

interface BundleValidateResult {
  branchId: string;
  formatVersion: number;
  entryCount: number;
  checksumsValid: boolean;
}
```
