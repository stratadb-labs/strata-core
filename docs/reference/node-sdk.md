# Node.js SDK Reference

The StrataDB Node.js SDK provides native bindings via NAPI-RS with full TypeScript support.

## Installation

```bash
npm install stratadb
# or
yarn add stratadb
```

## Quick Start

```typescript
import { Strata } from 'stratadb';

// Open a database
const db = Strata.open('/path/to/data');

// Store and retrieve data
db.kvPut('greeting', 'Hello, World!');
console.log(db.kvGet('greeting'));  // "Hello, World!"

// Use transactions
db.begin();
try {
  db.kvPut('a', 1);
  db.kvPut('b', 2);
  db.commit();
} catch (e) {
  db.rollback();
  throw e;
}

// Vector search
db.vectorCreateCollection('embeddings', 384);
db.vectorUpsert('embeddings', 'doc-1', new Array(384).fill(0.1));
const results = db.vectorSearch('embeddings', new Array(384).fill(0.1), 5);
```

---

## Opening a Database

### Strata.open(path)

Open a database at the given path.

```typescript
const db = Strata.open('/path/to/data');
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | string | Path to the database directory |

**Returns:** `Strata` instance

**Throws:** Error if the database cannot be opened

### Strata.cache()

Create an ephemeral in-memory database.

```typescript
const db = Strata.cache();
```

**Returns:** `Strata` instance

---

## KV Store

### kvPut(key, value)

Store a key-value pair.

```typescript
const version = db.kvPut('user:123', { name: 'Alice', age: 30 });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | The key |
| `value` | JsonValue | The value |

**Returns:** `number` - Version number

### kvGet(key)

Get a value by key.

```typescript
const value = db.kvGet('user:123');
if (value !== null) {
  console.log(value.name);
}
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | The key |

**Returns:** `JsonValue` - Value or `null` if not found

### kvDelete(key)

Delete a key.

```typescript
const deleted = db.kvDelete('user:123');
```

**Returns:** `boolean` - True if the key existed

### kvList(prefix?)

List keys with optional prefix filter.

```typescript
const allKeys = db.kvList();
const userKeys = db.kvList('user:');
```

**Returns:** `string[]` - Key names

### kvHistory(key)

Get version history for a key.

```typescript
const history = db.kvHistory('user:123');
if (history) {
  for (const entry of history) {
    console.log(`v${entry.version}: ${entry.value}`);
  }
}
```

**Returns:** `VersionedValue[] | null`

### kvGetVersioned(key)

Get a value with version info.

```typescript
const result = db.kvGetVersioned('user:123');
if (result) {
  console.log(`Value: ${result.value}, Version: ${result.version}`);
}
```

**Returns:** `VersionedValue | null`

### kvListPaginated(prefix?, limit?, cursor?)

List keys with pagination.

```typescript
const result = db.kvListPaginated('user:', 100);
console.log(result.keys);
```

**Returns:** `KvListResult` with `keys` array

---

## State Cell

### stateSet(cell, value)

Set a state cell value.

```typescript
const version = db.stateSet('counter', 0);
```

**Returns:** `number` - Version number

### stateGet(cell)

Get a state cell value.

```typescript
const value = db.stateGet('counter');
```

**Returns:** `JsonValue` - Value or `null`

### stateInit(cell, value)

Initialize a state cell only if it doesn't exist.

```typescript
const version = db.stateInit('counter', 0);
```

**Returns:** `number` - Version number

### stateCas(cell, newValue, expectedVersion?)

Compare-and-swap update.

```typescript
// Only update if version is 5
const newVersion = db.stateCas('counter', 10, 5);
if (newVersion === null) {
  console.log('CAS failed - version mismatch');
}
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `cell` | string | The cell name |
| `newValue` | JsonValue | The new value |
| `expectedVersion` | number? | Expected current version |

**Returns:** `number | null` - New version or null if CAS failed

### stateDelete(cell)

Delete a state cell.

```typescript
const deleted = db.stateDelete('counter');
```

**Returns:** `boolean`

### stateList(prefix?)

List state cell names.

```typescript
const cells = db.stateList();
const configCells = db.stateList('config:');
```

**Returns:** `string[]`

### stateHistory(cell)

Get version history for a state cell.

```typescript
const history = db.stateHistory('counter');
```

**Returns:** `VersionedValue[] | null`

### stateGetVersioned(cell)

Get a state cell with version info.

```typescript
const result = db.stateGetVersioned('counter');
```

**Returns:** `VersionedValue | null`

---

## Event Log

### eventAppend(eventType, payload)

Append an event to the log.

```typescript
const seq = db.eventAppend('user_action', { action: 'click', target: 'button' });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `eventType` | string | The event type |
| `payload` | JsonValue | The event payload |

**Returns:** `number` - Sequence number

### eventGet(sequence)

Get an event by sequence number.

```typescript
const event = db.eventGet(0);
if (event) {
  console.log(event.value);
}
```

**Returns:** `VersionedValue | null`

### eventList(eventType)

List events by type.

```typescript
const events = db.eventList('user_action');
for (const event of events) {
  console.log(event.value);
}
```

**Returns:** `VersionedValue[]`

### eventLen()

Get total event count.

```typescript
const count = db.eventLen();
```

**Returns:** `number`

### eventListPaginated(eventType, limit?, after?)

List events with pagination.

```typescript
const events = db.eventListPaginated('user_action', 100, 500);
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `eventType` | string | The event type |
| `limit` | number? | Maximum events |
| `after` | number? | Return events after this sequence |

**Returns:** `VersionedValue[]`

---

## JSON Store

### jsonSet(key, path, value)

Set a value at a JSONPath.

```typescript
// Set entire document
db.jsonSet('user:123', '$', { name: 'Alice', age: 30 });

// Set nested field
db.jsonSet('user:123', '$.email', 'alice@example.com');
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | string | Document key |
| `path` | string | JSONPath (use `$` for root) |
| `value` | JsonValue | The value |

**Returns:** `number` - Version number

### jsonGet(key, path)

Get a value at a JSONPath.

```typescript
const doc = db.jsonGet('user:123', '$');
const name = db.jsonGet('user:123', '$.name');
```

**Returns:** `JsonValue` - Value or `null`

### jsonDelete(key, path)

Delete a value at a JSONPath.

```typescript
const deletedCount = db.jsonDelete('user:123', '$.email');
```

**Returns:** `number` - Count of elements deleted

### jsonList(limit, prefix?, cursor?)

List JSON document keys with pagination.

```typescript
const result = db.jsonList(100, 'user:');
const { keys, cursor } = result;
```

**Returns:** `JsonListResult` with `keys` and optional `cursor`

### jsonHistory(key)

Get version history for a JSON document.

```typescript
const history = db.jsonHistory('user:123');
```

**Returns:** `VersionedValue[] | null`

### jsonGetVersioned(key)

Get a JSON document with version info.

```typescript
const result = db.jsonGetVersioned('user:123');
```

**Returns:** `VersionedValue | null`

---

## Vector Store

### vectorCreateCollection(collection, dimension, metric?)

Create a vector collection.

```typescript
db.vectorCreateCollection('embeddings', 384);
db.vectorCreateCollection('images', 512, 'euclidean');
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | string | Collection name |
| `dimension` | number | Vector dimension |
| `metric` | string? | `cosine` (default), `euclidean`, `dot_product` |

**Returns:** `number` - Version number

### vectorDeleteCollection(collection)

Delete a vector collection.

```typescript
const deleted = db.vectorDeleteCollection('embeddings');
```

**Returns:** `boolean`

### vectorListCollections()

List all vector collections.

```typescript
const collections = db.vectorListCollections();
for (const c of collections) {
  console.log(`${c.name}: ${c.count} vectors`);
}
```

**Returns:** `CollectionInfo[]`

### vectorUpsert(collection, key, vector, metadata?)

Insert or update a vector.

```typescript
const embedding = new Array(384).fill(0).map(() => Math.random());
db.vectorUpsert('embeddings', 'doc-1', embedding, { title: 'Hello' });
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | string | Collection name |
| `key` | string | Vector key |
| `vector` | number[] | Vector embedding |
| `metadata` | JsonValue? | Optional metadata |

**Returns:** `number` - Version number

### vectorGet(collection, key)

Get a vector by key.

```typescript
const result = db.vectorGet('embeddings', 'doc-1');
if (result) {
  console.log(result.embedding);
  console.log(result.metadata);
}
```

**Returns:** `VectorData | null`

### vectorDelete(collection, key)

Delete a vector.

```typescript
const deleted = db.vectorDelete('embeddings', 'doc-1');
```

**Returns:** `boolean`

### vectorSearch(collection, query, k)

Search for similar vectors.

```typescript
const query = new Array(384).fill(0.1);
const matches = db.vectorSearch('embeddings', query, 10);
for (const match of matches) {
  console.log(`${match.key}: ${match.score}`);
}
```

**Returns:** `SearchMatch[]` with `key`, `score`, `metadata`

### vectorSearchFiltered(collection, query, k, metric?, filter?)

Search with filter and metric override.

```typescript
const matches = db.vectorSearchFiltered(
  'embeddings',
  query,
  10,
  'euclidean',
  [
    { field: 'category', op: 'eq', value: 'science' },
    { field: 'year', op: 'gte', value: 2020 }
  ]
);
```

**Filter operators:** `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`

**Returns:** `SearchMatch[]`

### vectorCollectionStats(collection)

Get detailed collection statistics.

```typescript
const stats = db.vectorCollectionStats('embeddings');
console.log(`Count: ${stats.count}, Memory: ${stats.memoryBytes} bytes`);
```

**Returns:** `CollectionInfo`

### vectorBatchUpsert(collection, vectors)

Batch insert/update vectors.

```typescript
const vectors = [
  { key: 'doc-1', vector: [...], metadata: { title: 'A' } },
  { key: 'doc-2', vector: [...] },
];
const versions = db.vectorBatchUpsert('embeddings', vectors);
```

**Returns:** `number[]` - Version numbers

---

## Branches

### currentBranch()

Get the current branch name.

```typescript
const branch = db.currentBranch();
```

**Returns:** `string`

### setBranch(branch)

Switch to a different branch.

```typescript
db.setBranch('feature');
```

### createBranch(branch)

Create a new empty branch.

```typescript
db.createBranch('experiment');
```

### listBranches()

List all branches.

```typescript
const branches = db.listBranches();
```

**Returns:** `string[]`

### deleteBranch(branch)

Delete a branch.

```typescript
db.deleteBranch('experiment');
```

### branchExists(name)

Check if a branch exists.

```typescript
const exists = db.branchExists('feature');
```

**Returns:** `boolean`

### branchGet(name)

Get branch metadata.

```typescript
const info = db.branchGet('default');
if (info) {
  console.log(`Created: ${info.createdAt}, Version: ${info.version}`);
}
```

**Returns:** `BranchInfo | null`

### forkBranch(destination)

Fork the current branch with all its data.

```typescript
const result = db.forkBranch('experiment-copy');
console.log(`Copied ${result.keysCopied} keys`);
```

**Returns:** `ForkResult`

### diffBranches(branchA, branchB)

Compare two branches.

```typescript
const diff = db.diffBranches('default', 'feature');
console.log(`Added: ${diff.summary.totalAdded}`);
```

**Returns:** `DiffResult`

### mergeBranches(source, strategy?)

Merge a branch into the current branch.

```typescript
const result = db.mergeBranches('feature', 'last_writer_wins');
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `source` | string | Source branch |
| `strategy` | string? | `last_writer_wins` (default) or `strict` |

**Returns:** `MergeResult`

---

## Spaces

### currentSpace()

Get the current space name.

```typescript
const space = db.currentSpace();
```

**Returns:** `string`

### setSpace(space)

Switch to a different space.

```typescript
db.setSpace('conversations');
```

### listSpaces()

List all spaces.

```typescript
const spaces = db.listSpaces();
```

**Returns:** `string[]`

### deleteSpace(space)

Delete an empty space.

```typescript
db.deleteSpace('old-space');
```

### deleteSpaceForce(space)

Delete a space and all its data.

```typescript
db.deleteSpaceForce('old-space');
```

### spaceCreate(space)

Create a new space explicitly.

```typescript
db.spaceCreate('archive');
```

### spaceExists(space)

Check if a space exists.

```typescript
const exists = db.spaceExists('archive');
```

**Returns:** `boolean`

---

## Transactions

### begin(readOnly?)

Begin a new transaction.

```typescript
db.begin();
// or read-only
db.begin(true);
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `readOnly` | boolean? | Read-only transaction |

### commit()

Commit the current transaction.

```typescript
const version = db.commit();
```

**Returns:** `number` - Commit version

### rollback()

Rollback the current transaction.

```typescript
db.rollback();
```

### txnInfo()

Get current transaction info.

```typescript
const info = db.txnInfo();
if (info) {
  console.log(`Transaction ${info.id} is ${info.status}`);
}
```

**Returns:** `TransactionInfo | null`

### txnIsActive()

Check if a transaction is active.

```typescript
const active = db.txnIsActive();
```

**Returns:** `boolean`

### Transaction Pattern

```typescript
db.begin();
try {
  db.kvPut('a', 1);
  db.kvPut('b', 2);
  db.commit();
} catch (e) {
  db.rollback();
  throw e;
}
```

---

## Search

### search(query, k?, primitives?)

Search across multiple primitives.

```typescript
const results = db.search('hello world', 10, ['kv', 'json']);
for (const hit of results) {
  console.log(`${hit.entity} (${hit.primitive}): ${hit.score}`);
}
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `query` | string | Search query |
| `k` | number? | Maximum results |
| `primitives` | string[]? | Primitives to search |

**Returns:** `SearchHit[]` with `entity`, `primitive`, `score`, `rank`, `snippet`

---

## Database Operations

### ping()

Check database connectivity.

```typescript
const version = db.ping();
```

**Returns:** `string` - Version string

### info()

Get database information.

```typescript
const info = db.info();
console.log(`Version: ${info.version}`);
console.log(`Total keys: ${info.totalKeys}`);
```

**Returns:** `DatabaseInfo`

### flush()

Flush pending writes to disk.

```typescript
db.flush();
```

### compact()

Trigger database compaction.

```typescript
db.compact();
```

---

## Bundle Operations

### branchExport(branch, path)

Export a branch to a bundle file.

```typescript
const result = db.branchExport('default', '/tmp/backup.bundle');
console.log(`Exported ${result.entryCount} entries`);
```

**Returns:** `BranchExportResult`

### branchImport(path)

Import a branch from a bundle file.

```typescript
const result = db.branchImport('/tmp/backup.bundle');
console.log(`Imported to branch ${result.branchId}`);
```

**Returns:** `BranchImportResult`

### branchValidateBundle(path)

Validate a bundle file without importing.

```typescript
const result = db.branchValidateBundle('/tmp/backup.bundle');
console.log(`Valid: ${result.checksumsValid}`);
```

**Returns:** `BundleValidateResult`

---

## Error Handling

All methods may throw errors for database operations:

```typescript
try {
  db.setBranch('nonexistent');
} catch (e) {
  console.log(`Error: ${e.message}`);
}
```

Common errors:
- Branch not found
- Collection not found
- CAS version mismatch
- Transaction already active
- Invalid input

---

## TypeScript Types

### JsonValue

```typescript
type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };
```

### VersionedValue

```typescript
interface VersionedValue {
  value: JsonValue;
  version: number;
  timestamp: number;
}
```

### CollectionInfo

```typescript
interface CollectionInfo {
  name: string;
  dimension: number;
  metric: string;
  count: number;
  indexType: string;
  memoryBytes: number;
}
```

### VectorData

```typescript
interface VectorData {
  key: string;
  embedding: number[];
  metadata?: JsonValue;
  version: number;
  timestamp: number;
}
```

### SearchMatch

```typescript
interface SearchMatch {
  key: string;
  score: number;
  metadata?: JsonValue;
}
```

### MetadataFilter

```typescript
interface MetadataFilter {
  field: string;
  op: 'eq' | 'ne' | 'gt' | 'gte' | 'lt' | 'lte' | 'in' | 'contains';
  value: JsonValue;
}
```

### SearchHit

```typescript
interface SearchHit {
  entity: string;
  primitive: string;
  score: number;
  rank: number;
  snippet?: string;
}
```

### TransactionInfo

```typescript
interface TransactionInfo {
  id: string;
  status: string;
  startedAt: number;
}
```

### BranchInfo

```typescript
interface BranchInfo {
  id: string;
  status: string;
  createdAt: number;
  updatedAt: number;
  parentId?: string;
  version: number;
  timestamp: number;
}
```

### DatabaseInfo

```typescript
interface DatabaseInfo {
  version: string;
  uptimeSecs: number;
  branchCount: number;
  totalKeys: number;
}
```

---

## Synchronous API

All methods in the Node.js SDK are **synchronous**. This is because StrataDB is an embedded database with no network I/O, making synchronous operations efficient.

For async patterns, wrap in promises:

```typescript
async function getData(key: string): Promise<JsonValue> {
  return db.kvGet(key);
}

// Or use worker threads for heavy operations
import { Worker } from 'worker_threads';
```
