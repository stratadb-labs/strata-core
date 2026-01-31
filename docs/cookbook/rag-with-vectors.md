# RAG with Vectors

This recipe shows how to build a Retrieval-Augmented Generation (RAG) pattern using the Vector Store for embeddings and the KV Store for document text.

## Pattern

1. **Index**: Store document embeddings in a vector collection, full text in KV
2. **Search**: Find similar vectors, then retrieve the associated text
3. **Generate**: Pass the retrieved context to your LLM

## Implementation

### Step 1: Index Documents

```rust
use stratadb::{Strata, Value, DistanceMetric};

fn index_documents(db: &Strata, documents: &[(String, String, Vec<f32>)]) -> stratadb::Result<()> {
    // documents: Vec of (id, text, embedding)

    // Create a collection for document embeddings
    let dimension = documents[0].2.len() as u64;
    db.vector_create_collection("knowledge", dimension, DistanceMetric::Cosine)?;

    for (id, text, embedding) in documents {
        // Store the embedding with metadata
        let metadata: Value = serde_json::json!({
            "text_key": format!("doc:{}", id),
        }).into();
        db.vector_upsert("knowledge", id, embedding.clone(), Some(metadata))?;

        // Store the full text in KV
        db.kv_put(&format!("doc:{}", id), text.as_str())?;
    }

    Ok(())
}
```

### Step 2: Search and Retrieve

```rust
fn retrieve_context(
    db: &Strata,
    query_embedding: Vec<f32>,
    top_k: u64,
) -> stratadb::Result<Vec<String>> {
    // Find similar documents
    let matches = db.vector_search("knowledge", query_embedding, top_k)?;

    // Retrieve the full text for each match
    let mut context = Vec::new();
    for m in &matches {
        let text_key = format!("doc:{}", m.key);
        if let Some(value) = db.kv_get(&text_key)? {
            if let Some(text) = value.as_str() {
                context.push(text.to_string());
            }
        }
    }

    Ok(context)
}
```

### Step 3: Putting It Together

```rust
fn rag_query(db: &Strata, query: &str, embed_fn: impl Fn(&str) -> Vec<f32>) -> stratadb::Result<String> {
    // Embed the query
    let query_embedding = embed_fn(query);

    // Retrieve relevant context
    let context = retrieve_context(db, query_embedding, 5)?;

    // Build the prompt with context
    let context_str = context.join("\n\n");
    let prompt = format!(
        "Context:\n{}\n\nQuestion: {}\n\nAnswer based on the context above:",
        context_str, query
    );

    // Send to your LLM (placeholder)
    Ok(prompt)
}
```

## Branch-Scoped Knowledge Bases

Use branches to maintain different knowledge bases:

```rust
let mut db = Strata::open_temp()?;

// General knowledge base in default branch
db.vector_create_collection("kb", 384, DistanceMetric::Cosine)?;
// ... index general documents ...

// Session-specific knowledge
db.create_branch("session-001")?;
db.set_branch("session-001")?;
db.vector_create_collection("kb", 384, DistanceMetric::Cosine)?;
// ... index session-specific documents ...
// Search only sees session-specific documents
```

## Incremental Updates

Add new documents at any time:

```rust
fn add_document(db: &Strata, id: &str, text: &str, embedding: Vec<f32>) -> stratadb::Result<()> {
    let metadata: Value = serde_json::json!({"text_key": format!("doc:{}", id)}).into();
    db.vector_upsert("knowledge", id, embedding, Some(metadata))?;
    db.kv_put(&format!("doc:{}", id), text)?;
    Ok(())
}
```

## See Also

- [Vector Store Guide](../guides/vector-store.md) — full vector API
- [KV Store Guide](../guides/kv-store.md) — key-value operations
- [Search Guide](../guides/search.md) — hybrid keyword + semantic search
