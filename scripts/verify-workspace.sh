#!/bin/bash
# Verify Cargo workspace structure
# Run this after Rust is installed

set -e

# Source Rust environment if it exists
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

echo "🔍 Verifying workspace structure..."
echo ""

# Check workspace Cargo.toml exists
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Root Cargo.toml not found"
    exit 1
fi
echo "✅ Root Cargo.toml exists"

# Check all crate directories exist
CRATES=("core" "storage" "concurrency" "durability" "primitives" "engine" "api")
for crate in "${CRATES[@]}"; do
    if [ ! -d "crates/$crate" ]; then
        echo "❌ crates/$crate directory not found"
        exit 1
    fi
    if [ ! -f "crates/$crate/Cargo.toml" ]; then
        echo "❌ crates/$crate/Cargo.toml not found"
        exit 1
    fi
    if [ ! -f "crates/$crate/src/lib.rs" ]; then
        echo "❌ crates/$crate/src/lib.rs not found"
        exit 1
    fi
    echo "✅ crate $crate structure complete"
done

echo ""
echo "🔨 Building workspace..."
cargo build --all

echo ""
echo "🧪 Running tests..."
cargo test --all

echo ""
echo "🎨 Checking formatting..."
cargo fmt --all -- --check

echo ""
echo "📎 Running clippy..."
cargo clippy --all -- -D warnings

echo ""
echo "✅ Workspace verification complete!"
echo ""
echo "Crate structure:"
echo "  - in-mem-core: Core types and traits"
echo "  - in-mem-storage: Storage layer"
echo "  - in-mem-concurrency: OCC transactions (M2)"
echo "  - in-mem-durability: WAL and snapshots"
echo "  - in-mem-primitives: Six primitives"
echo "  - in-mem-engine: Database orchestration"
echo "  - in-mem-api: Public API layer"
