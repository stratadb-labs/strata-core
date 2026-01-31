# Value Types

All data in StrataDB is represented by the `Value` enum — a closed set of 8 variants. There are no extensions and no implicit coercions.

## The 8 Variants

```rust
pub enum Value {
    Null,                          // Absence of value
    Bool(bool),                    // true / false
    Int(i64),                      // 64-bit signed integer
    Float(f64),                    // 64-bit IEEE-754 floating point
    String(String),                // UTF-8 string
    Bytes(Vec<u8>),                // Raw binary data
    Array(Vec<Value>),             // Ordered sequence of values
    Object(HashMap<String, Value>),// String-keyed map (JSON object)
}
```

## Type Rules

StrataDB enforces strict typing with no surprises:

| Rule | Description | Example |
|------|-------------|---------|
| **VAL-1** | Exactly 8 types, no more | No `Timestamp`, `Decimal`, or custom types |
| **VAL-2** | No implicit coercions | Storing an `i32` doesn't create a float |
| **VAL-3** | Different types are never equal | `Int(1) != Float(1.0)` |
| **VAL-4** | Bytes are not strings | `Bytes(b"hello") != String("hello")` |
| **VAL-5** | IEEE-754 float equality | `NaN != NaN`, `-0.0 == 0.0` |

## Ergonomic Conversions

You rarely need to construct `Value` directly. StrataDB implements `From<T>` for common Rust types, and all methods accept `impl Into<Value>`:

```rust
// All of these work — no explicit Value construction needed
db.kv_put("name", "Alice")?;        // &str → Value::String
db.kv_put("age", 30i64)?;           // i64 → Value::Int
db.kv_put("score", 95.5)?;          // f64 → Value::Float
db.kv_put("active", true)?;         // bool → Value::Bool
db.state_set("counter", 0i64)?;     // i64 → Value::Int
```

### Full Conversion Table

| Rust Type | Value Variant |
|-----------|--------------|
| `&str` | `Value::String` |
| `String` | `Value::String` |
| `i32` | `Value::Int` (widened to i64) |
| `i64` | `Value::Int` |
| `f32` | `Value::Float` (widened to f64) |
| `f64` | `Value::Float` |
| `bool` | `Value::Bool` |
| `Vec<u8>` | `Value::Bytes` |
| `&[u8]` | `Value::Bytes` |
| `Vec<Value>` | `Value::Array` |
| `HashMap<String, Value>` | `Value::Object` |
| `()` | `Value::Null` |
| `serde_json::Value` | Corresponding variant |

## Reading Values

Use the `as_*()` accessors to extract typed data:

```rust
let value = db.kv_get("name")?;
if let Some(v) = value {
    // Type-safe extraction — returns None for wrong type
    if let Some(name) = v.as_str() {
        println!("Name: {}", name);
    }
}
```

### Accessor Table

| Accessor | Returns | For Variant |
|----------|---------|-------------|
| `as_bool()` | `Option<bool>` | `Bool` |
| `as_int()` | `Option<i64>` | `Int` |
| `as_float()` | `Option<f64>` | `Float` |
| `as_str()` | `Option<&str>` | `String` |
| `as_bytes()` | `Option<&[u8]>` | `Bytes` |
| `as_array()` | `Option<&[Value]>` | `Array` |
| `as_object()` | `Option<&HashMap<String, Value>>` | `Object` |

### Type Checking

| Method | Checks |
|--------|--------|
| `is_null()` | `Null` |
| `is_bool()` | `Bool` |
| `is_int()` | `Int` |
| `is_float()` | `Float` |
| `is_string()` | `String` |
| `is_bytes()` | `Bytes` |
| `is_array()` | `Array` |
| `is_object()` | `Object` |
| `type_name()` | Returns `"Null"`, `"Bool"`, etc. |

## JSON Interop

`Value` converts to and from `serde_json::Value`:

```rust
// serde_json::Value → Value
let json = serde_json::json!({"key": 42, "nested": [1, 2, 3]});
let value: Value = json.into();

// Value → serde_json::Value
let json: serde_json::Value = value.into();
```

### Edge Cases in JSON Conversion

| Value | To JSON | Back to Value |
|-------|---------|---------------|
| `Float(NaN)` | `null` | `Null` (lossy) |
| `Float(Infinity)` | `null` | `Null` (lossy) |
| `Bytes(vec![1,2,3])` | `"AQID"` (base64) | `String("AQID")` (lossy) |
| `Int(i64::MAX)` | Number | `Int(i64::MAX)` (lossless) |

The `Bytes → JSON → Value` round-trip is lossy: bytes become a base64 string, and converting back produces a `Value::String`, not `Value::Bytes`.

## Event Log Payloads

Event log payloads must be `Value::Object`. This is enforced at the API level:

```rust
// This works
let payload = Value::Object(
    [("tool".into(), Value::String("search".into()))].into_iter().collect()
);
db.event_append("tool_call", payload)?;

// Using serde_json::json! macro is more ergonomic
let payload: Value = serde_json::json!({"tool": "search"}).into();
db.event_append("tool_call", payload)?;
```

## Next

- [Transactions](transactions.md) — how concurrent access works
- [Value Type Reference](../reference/value-type-reference.md) — complete specification
