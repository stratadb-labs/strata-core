# Value Type Reference

Complete specification of the `Value` enum and its conversions.

## Variants

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}
```

## Type Rules

| Rule | Specification |
|------|--------------|
| **VAL-1** | Exactly 8 variants. No extensions. |
| **VAL-2** | No implicit type coercions. `i32` promotes to `i64` in `From`, not at runtime. |
| **VAL-3** | Different types are never equal: `Int(1) != Float(1.0)` |
| **VAL-4** | `Bytes` and `String` are distinct: `Bytes(b"hello") != String("hello")` |
| **VAL-5** | Float equality follows IEEE-754: `NaN != NaN`, `-0.0 == 0.0` |

## `From<T>` Implementations

| Source Type | Target Variant | Example |
|-------------|---------------|---------|
| `&str` | `String` | `"hello".into()` |
| `String` | `String` | `String::from("hello").into()` |
| `i32` | `Int` | `42i32.into()` → `Int(42)` |
| `i64` | `Int` | `42i64.into()` → `Int(42)` |
| `f32` | `Float` | `2.5f32.into()` → `Float(2.5)` |
| `f64` | `Float` | `3.14f64.into()` → `Float(3.14)` |
| `bool` | `Bool` | `true.into()` → `Bool(true)` |
| `Vec<u8>` | `Bytes` | `vec![1u8, 2, 3].into()` |
| `&[u8]` | `Bytes` | `(&[1u8, 2, 3][..]).into()` |
| `Vec<Value>` | `Array` | `vec![Value::Int(1)].into()` |
| `HashMap<String, Value>` | `Object` | `map.into()` |
| `()` | `Null` | `().into()` → `Null` |
| `serde_json::Value` | Corresponding | `serde_json::json!(42).into()` → `Int(42)` |

## Accessor Methods

| Method | Return Type | For Variant |
|--------|-------------|-------------|
| `as_bool()` | `Option<bool>` | `Bool` |
| `as_int()` | `Option<i64>` | `Int` |
| `as_float()` | `Option<f64>` | `Float` |
| `as_str()` | `Option<&str>` | `String` |
| `as_bytes()` | `Option<&[u8]>` | `Bytes` |
| `as_array()` | `Option<&[Value]>` | `Array` |
| `as_object()` | `Option<&HashMap<String, Value>>` | `Object` |

All accessors return `None` when called on the wrong variant.

## Type Checking Methods

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
| `type_name()` | Returns `"Null"`, `"Bool"`, `"Int"`, `"Float"`, `"String"`, `"Bytes"`, `"Array"`, or `"Object"` |

## serde_json Conversion

### `serde_json::Value` → `Value`

| JSON | Value |
|------|-------|
| `null` | `Null` |
| `true` / `false` | `Bool(b)` |
| Integer that fits `i64` | `Int(n)` |
| Other number | `Float(f)` |
| String | `String(s)` |
| Array | `Array(vec)` |
| Object | `Object(map)` |

### `Value` → `serde_json::Value`

| Value | JSON | Notes |
|-------|------|-------|
| `Null` | `null` | |
| `Bool(b)` | `true`/`false` | |
| `Int(n)` | Number | |
| `Float(f)` | Number | `NaN` and `Infinity` become `null` |
| `String(s)` | String | |
| `Bytes(b)` | String (base64) | **Lossy** — round-trip produces `String` |
| `Array(a)` | Array | Recursive conversion |
| `Object(o)` | Object | Recursive conversion |

## Equality Semantics

`Value` implements `PartialEq` but not `Eq` (because `Float(NaN) != Float(NaN)`).

### Cross-Type Inequality

Different variants are never equal regardless of the values they contain:

```rust
assert_ne!(Value::Int(0), Value::Float(0.0));
assert_ne!(Value::Int(0), Value::Bool(false));
assert_ne!(Value::Null, Value::Bool(false));
assert_ne!(Value::Null, Value::Int(0));
assert_ne!(Value::String("hello".into()), Value::Bytes(b"hello".to_vec()));
```

### Object Equality

Objects are equal when they have the same keys with equal values. Key order does not matter.

### Float Edge Cases

```rust
assert_ne!(Value::Float(f64::NAN), Value::Float(f64::NAN));  // NaN != NaN
assert_eq!(Value::Float(-0.0), Value::Float(0.0));            // -0.0 == 0.0
assert_eq!(Value::Float(f64::INFINITY), Value::Float(f64::INFINITY));
assert_ne!(Value::Float(f64::INFINITY), Value::Float(f64::NEG_INFINITY));
```

## Serialization

`Value` implements `Serialize` and `Deserialize` (serde). The serialized format uses tagged variants:

```json
{"Int": 42}
{"String": "hello"}
{"Array": [{"Int": 1}, {"Bool": true}]}
{"Object": {"key": {"String": "value"}}}
```
