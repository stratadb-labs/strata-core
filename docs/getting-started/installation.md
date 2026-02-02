# Installation

## Requirements

- **Rust 1.70** or later
- A standard Rust toolchain (`rustup`, `cargo`)

## Add to Your Project

Add StrataDB to your `Cargo.toml`:

```toml
[dependencies]
stratadb = "0.1"
```

For JSON construction in examples, you may also want `serde_json`:

```toml
[dependencies]
stratadb = "0.1"
serde_json = "1.0"
```

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `default` | Core database functionality | Yes |
| `perf-trace` | Per-layer timing instrumentation | No |

## Building from Source

```bash
git clone https://github.com/anibjoshi/strata.git
cd strata
cargo build --release
```

### Running Tests

```bash
# All tests across the workspace
cargo test --workspace

# Specific crate
cargo test -p strata-executor

# With output
cargo test --workspace -- --nocapture
```

## Verify Installation

Create a minimal program to verify everything works:

```rust
use stratadb::Strata;

fn main() -> stratadb::Result<()> {
    let db = Strata::cache()?;
    db.kv_put("hello", "world")?;

    let value = db.kv_get("hello")?;
    assert!(value.is_some());
    println!("StrataDB is working!");

    Ok(())
}
```

Run it:

```bash
cargo run
```

If you see "StrataDB is working!", you are ready to go. Continue to [Your First Database](first-database.md) for a complete tutorial.

## Next

- [Your First Database](first-database.md) — hands-on tutorial
- [Concepts](../concepts/index.md) — understand the mental model
