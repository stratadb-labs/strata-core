# Your First Database

This tutorial walks through all six StrataDB primitives. Every code block is a complete, runnable example using `Strata::open_temp()` so you can follow along without disk setup.

## Prerequisites

- [StrataDB installed](installation.md)
- `serde_json` in your dependencies (for JSON examples)

## Step 1: Open a Database

```rust
use stratadb::Strata;

fn main() -> stratadb::Result<()> {
    // Ephemeral database — no files created
    let db = Strata::open_temp()?;
    println!("Database opened, current branch: {}", db.current_branch());
    // Output: Database opened, current branch: default
    Ok(())
}
```

StrataDB starts you on the "default" branch. All data operations target the current branch.

## Step 2: KV Store — Working Memory

The KV Store is the most general-purpose primitive. Store any value by key.

```rust
use stratadb::{Strata, Value};

fn main() -> stratadb::Result<()> {
    let db = Strata::open_temp()?;

    // Put values — accepts &str, i64, f64, bool directly via Into<Value>
    db.kv_put("agent:name", "Alice")?;
    db.kv_put("agent:score", 95i64)?;
    db.kv_put("agent:active", true)?;

    // Get a value
    let name = db.kv_get("agent:name")?;
    assert_eq!(name, Some(Value::String("Alice".into())));

    // List keys with a prefix
    let agent_keys = db.kv_list(Some("agent:"))?;
    assert_eq!(agent_keys.len(), 3);

    // Delete a key
    let existed = db.kv_delete("agent:score")?;
    assert!(existed);
    assert!(db.kv_get("agent:score")?.is_none());

    Ok(())
}
```

## Step 3: Event Log — Immutable History

The Event Log records events that cannot be modified after writing. Each event has a type and an Object payload.

```rust
use stratadb::{Strata, Value};
use std::collections::HashMap;

fn main() -> stratadb::Result<()> {
    let db = Strata::open_temp()?;

    // Append events — payloads must be Value::Object
    let payload = Value::Object(
        [("tool".to_string(), Value::String("search".into())),
         ("query".to_string(), Value::String("weather".into()))]
            .into_iter().collect()
    );
    let seq = db.event_append("tool_call", payload)?;
    println!("Event written at sequence: {}", seq);

    // Read a specific event by sequence number
    let event = db.event_read(seq)?;
    assert!(event.is_some());

    // Read all events of a type
    let tool_calls = db.event_read_by_type("tool_call")?;
    assert_eq!(tool_calls.len(), 1);

    // Count total events
    assert_eq!(db.event_len()?, 1);

    Ok(())
}
```

## Step 4: State Cell — Coordination

State cells provide mutable state with compare-and-swap (CAS) for safe concurrent updates.

```rust
use stratadb::{Strata, Value};

fn main() -> stratadb::Result<()> {
    let db = Strata::open_temp()?;

    // Initialize only if absent (idempotent)
    db.state_init("status", "idle")?;
    db.state_init("status", "should-not-overwrite")?;
    assert_eq!(db.state_read("status")?, Some(Value::String("idle".into())));

    // Unconditional set
    db.state_set("counter", 0i64)?;

    // Compare-and-swap: update only if the version matches
    // First write has version counter 1
    let new_version = db.state_cas("counter", Some(1), 1i64)?;
    assert!(new_version.is_some()); // CAS succeeded

    // CAS with wrong expected version fails
    let failed = db.state_cas("counter", Some(999), 2i64)?;
    // CAS returns None when the expected version doesn't match
    // (depends on implementation — check your return value)

    Ok(())
}
```

## Step 5: JSON Store — Structured Documents

Store JSON documents and mutate them at specific paths.

```rust
use stratadb::{Strata, Value};

fn main() -> stratadb::Result<()> {
    let db = Strata::open_temp()?;

    // Create a document at root path "$"
    let config: Value = serde_json::json!({
        "model": "gpt-4",
        "temperature": 0.7,
        "max_tokens": 1000
    }).into();
    db.json_set("config", "$", config)?;

    // Read the whole document
    let doc = db.json_get("config", "$")?;
    assert!(doc.is_some());

    // Update a nested path
    db.json_set("config", "$.temperature", 0.9)?;

    // Read just the updated field
    let temp = db.json_get("config", "$.temperature")?;
    assert_eq!(temp, Some(Value::Float(0.9)));

    // List documents with a prefix
    let (keys, _cursor) = db.json_list(None, None, 100)?;
    assert_eq!(keys.len(), 1);

    // Delete the document
    db.json_delete("config", "$")?;

    Ok(())
}
```

## Step 6: Vector Store — Similarity Search

Store embeddings and search by similarity.

```rust
use stratadb::Strata;
use stratadb::DistanceMetric;

fn main() -> stratadb::Result<()> {
    let db = Strata::open_temp()?;

    // Create a collection with 4-dimensional vectors and cosine similarity
    db.vector_create_collection("embeddings", 4, DistanceMetric::Cosine)?;

    // Upsert vectors (with optional metadata)
    db.vector_upsert("embeddings", "doc-1", vec![1.0, 0.0, 0.0, 0.0], None)?;
    db.vector_upsert("embeddings", "doc-2", vec![0.0, 1.0, 0.0, 0.0], None)?;
    db.vector_upsert("embeddings", "doc-3", vec![0.9, 0.1, 0.0, 0.0], None)?;

    // Search for the 2 most similar vectors
    let results = db.vector_search("embeddings", vec![1.0, 0.0, 0.0, 0.0], 2)?;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].key, "doc-1"); // Most similar
    println!("Best match: {} (score: {})", results[0].key, results[0].score);

    // Delete a vector
    db.vector_delete("embeddings", "doc-2")?;

    Ok(())
}
```

## Step 7: Branches — Data Isolation

Branches give you isolated namespaces for data, like git branches.

```rust
use stratadb::{Strata, Value};

fn main() -> stratadb::Result<()> {
    let mut db = Strata::open_temp()?;

    // You start on the "default" branch
    db.kv_put("shared-key", "default-value")?;

    // Create a new branch and switch to it
    db.create_branch("experiment")?;
    db.set_branch("experiment")?;

    // The key doesn't exist in this branch
    assert!(db.kv_get("shared-key")?.is_none());

    // Write data in the experiment branch
    db.kv_put("shared-key", "experiment-value")?;

    // Switch back — original data is intact
    db.set_branch("default")?;
    assert_eq!(
        db.kv_get("shared-key")?,
        Some(Value::String("default-value".into()))
    );

    // List all branches
    let branches = db.list_branches()?;
    println!("Branches: {:?}", branches);

    // Clean up
    db.delete_branch("experiment")?;

    Ok(())
}
```

## Putting It All Together

Here is a small program that simulates an AI agent session using multiple primitives:

```rust
use stratadb::{Strata, Value};
use std::collections::HashMap;

fn main() -> stratadb::Result<()> {
    let mut db = Strata::open_temp()?;

    // Create a branch for this agent session
    db.create_branch("session-001")?;
    db.set_branch("session-001")?;

    // Store agent config (KV)
    db.kv_put("config:model", "gpt-4")?;
    db.kv_put("config:max_retries", 3i64)?;

    // Initialize agent status (StateCell)
    db.state_init("status", "started")?;

    // Log a tool call (EventLog)
    let payload = Value::Object(
        [("tool".to_string(), Value::String("web_search".into())),
         ("query".to_string(), Value::String("StrataDB docs".into()))]
            .into_iter().collect()
    );
    db.event_append("tool_call", payload)?;

    // Store structured conversation (JSON)
    let conversation: Value = serde_json::json!({
        "messages": [
            {"role": "user", "content": "What is StrataDB?"},
            {"role": "assistant", "content": "An embedded database for AI agents."}
        ]
    }).into();
    db.json_set("conversation", "$", conversation)?;

    // Update status
    db.state_set("status", "completed")?;

    // Review the session
    println!("Events recorded: {}", db.event_len()?);
    println!("Status: {:?}", db.state_read("status")?);

    Ok(())
}
```

## Next Steps

- [Concepts](../concepts/index.md) — understand branches, value types, and transactions
- [Guides](../guides/index.md) — deep dives into each primitive
- [Cookbook](../cookbook/index.md) — real-world patterns
