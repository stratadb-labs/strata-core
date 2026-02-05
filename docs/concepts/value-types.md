# Value Types

All data in StrataDB is represented by the `Value` enum — a closed set of 8 variants. There are no extensions and no implicit coercions.

## The 8 Variants

| Variant | Description | CLI Example |
|---------|-------------|-------------|
| `Null` | Absence of value | `null` |
| `Bool` | true / false | `true`, `false` |
| `Int` | 64-bit signed integer | `42`, `-1` |
| `Float` | 64-bit IEEE-754 floating point | `3.14`, `0.7` |
| `String` | UTF-8 string | `hello`, `"hello world"` |
| `Bytes` | Raw binary data | (not directly supported in CLI) |
| `Array` | Ordered sequence of values | `'[1, 2, 3]'` (JSON) |
| `Object` | String-keyed map (JSON object) | `'{"key":"value"}'` (JSON) |

## Type Rules

StrataDB enforces strict typing with no surprises:

| Rule | Description | Example |
|------|-------------|---------|
| **VAL-1** | Exactly 8 types, no more | No `Timestamp`, `Decimal`, or custom types |
| **VAL-2** | No implicit coercions | Storing an integer doesn't create a float |
| **VAL-3** | Different types are never equal | `Int(1) != Float(1.0)` |
| **VAL-4** | Bytes are not strings | `Bytes(b"hello") != String("hello")` |
| **VAL-5** | IEEE-754 float equality | `NaN != NaN`, `-0.0 == 0.0` |

## CLI Type Detection

The CLI auto-detects types from input format. You rarely need to think about types:

```
$ strata --cache
strata:default/default> kv put name Alice
(version) 1
strata:default/default> kv put age 30
(version) 1
strata:default/default> kv put score 95.5
(version) 1
strata:default/default> kv put active true
(version) 1
strata:default/default> state set counter 0
(version) 1
```

### Detection Rules

| Input Format | Detected Type |
|-------------|---------------|
| `true`, `false` | Bool |
| Digits only (e.g., `42`, `-1`) | Int |
| Digits with decimal (e.g., `3.14`) | Float |
| `null` | Null |
| JSON object `'{...}'` | Object |
| JSON array `'[...]'` | Array |
| Everything else | String |

## Reading Values

The CLI displays values in a readable format:

```
$ strata --cache
strata:default/default> kv put name Alice
(version) 1
strata:default/default> kv get name
"Alice"
strata:default/default> kv put count 42
(version) 1
strata:default/default> kv get count
42
strata:default/default> kv get missing
(nil)
```

Values that don't exist return `(nil)`.

### Accessor Table

| Type | CLI Display |
|------|-------------|
| Bool | `true` / `false` |
| Int | `42` |
| Float | `3.14` |
| String | `"Alice"` |
| Null | `null` |
| Array | `[1, 2, 3]` |
| Object | `{"key":"value"}` |

## JSON Interop

The CLI accepts JSON strings for complex values:

```
$ strata --cache
strata:default/default> json set config $ '{"key":42,"nested":[1,2,3]}'
(version) 1
strata:default/default> json get config
{"key":42,"nested":[1,2,3]}
```

### Edge Cases in JSON Conversion

| Value | To JSON | Notes |
|-------|---------|-------|
| `Float(NaN)` | `null` | Lossy |
| `Float(Infinity)` | `null` | Lossy |
| `Bytes` | Base64 string | Lossy round-trip |
| `Int(i64::MAX)` | Number | Lossless |

The `Bytes → JSON → Value` round-trip is lossy: bytes become a base64 string, and converting back produces a String, not Bytes.

## Event Log Payloads

Event log payloads must be JSON objects. Pass them as JSON strings:

```
$ strata --cache
strata:default/default> event append tool_call '{"tool":"search"}'
(seq) 1
strata:default/default> event append tool_call '{"tool":"search","results":10}'
(seq) 2
```

## Next

- [Transactions](transactions.md) — how concurrent access works
- [Value Type Reference](../reference/value-type-reference.md) — complete specification
