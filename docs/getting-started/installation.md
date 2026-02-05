# Installation

## Install the CLI

### From crates.io (recommended)

```bash
cargo install strata-cli
```

### From source

```bash
git clone https://github.com/anibjoshi/strata.git
cd strata
cargo build --release
```

The binary is located at `target/release/strata`.

### Running Tests (development)

```bash
# All tests across the workspace
cargo test --workspace

# Specific crate
cargo test -p strata-executor

# With output
cargo test --workspace -- --nocapture
```

## Verify Installation

Run a quick command to confirm the CLI is working:

```bash
strata --cache ping
```

Expected output:

```
PONG
```

You can also try a quick interactive session:

```
$ strata --cache
strata:default/default> kv put hello world
(version) 1
strata:default/default> kv get hello
"world"
strata:default/default> quit
```

If you see the output above, you are ready to go. Continue to [Your First Database](first-database.md) for a complete tutorial.

## Next

- [Your First Database](first-database.md) — hands-on tutorial
- [Concepts](../concepts/index.md) — understand the mental model
