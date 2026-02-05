# A/B Testing with Branches

This recipe shows how to use branches to compare different agent strategies side by side.

## Pattern

1. Create one branch per variant (strategy A, strategy B)
2. Run each strategy in its own isolated branch
3. Compare the results by reading from each branch

## Implementation

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"

# Create branches for each variant
strata $DB branch create variant-a
strata $DB branch create variant-b

# === Run Strategy A ===
strata $DB --branch variant-a kv put config:strategy conservative
strata $DB --branch variant-a kv put config:temperature 0.3

for i in $(seq 0 4); do
    strata $DB --branch variant-a event append action "{\"step\":$i,\"strategy\":\"conservative\",\"action\":\"careful_action\"}"
done
strata $DB --branch variant-a kv put result:score 85
strata $DB --branch variant-a state set status completed

# === Run Strategy B ===
strata $DB --branch variant-b kv put config:strategy aggressive
strata $DB --branch variant-b kv put config:temperature 0.9

for i in $(seq 0 7); do
    strata $DB --branch variant-b event append action "{\"step\":$i,\"strategy\":\"aggressive\",\"action\":\"bold_action\"}"
done
strata $DB --branch variant-b kv put result:score 92
strata $DB --branch variant-b state set status completed

# === Compare Results ===
SCORE_A=$(strata $DB --branch variant-a --raw kv get result:score)
SCORE_B=$(strata $DB --branch variant-b --raw kv get result:score)
EVENTS_A=$(strata $DB --branch variant-a event len)
EVENTS_B=$(strata $DB --branch variant-b event len)
STRATEGY_A=$(strata $DB --branch variant-a --raw kv get config:strategy)
STRATEGY_B=$(strata $DB --branch variant-b --raw kv get config:strategy)

echo "=== A/B Test Results ==="
echo "Variant A ($STRATEGY_A): score=$SCORE_A, actions=$EVENTS_A"
echo "Variant B ($STRATEGY_B): score=$SCORE_B, actions=$EVENTS_B"

if [ "$SCORE_A" -gt "$SCORE_B" ]; then
    echo "Winner: variant-a"
else
    echo "Winner: variant-b"
fi
```

## Scaling to Many Variants

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"
VARIANTS=("low:0.1" "medium:0.5" "high:0.9")

# Run each variant
for variant in "${VARIANTS[@]}"; do
    name="${variant%%:*}"
    temp="${variant##*:}"
    branch="variant-$name"

    strata $DB branch create "$branch"
    strata $DB --branch "$branch" kv put config:temperature "$temp"
    # ... run the agent ...
    strata $DB --branch "$branch" kv put result:score 0  # placeholder
done

# Compare all variants
best_score=0
best_variant=""

for variant in "${VARIANTS[@]}"; do
    name="${variant%%:*}"
    branch="variant-$name"
    score=$(strata $DB --branch "$branch" --raw kv get result:score)
    echo "Variant $name: score=$score"
    if [ "$score" -gt "$best_score" ]; then
        best_score=$score
        best_variant=$name
    fi
done

echo "Best variant: $best_variant (score: $best_score)"

# Clean up losing variants
for variant in "${VARIANTS[@]}"; do
    name="${variant%%:*}"
    if [ "$name" != "$best_variant" ]; then
        strata $DB branch del "variant-$name"
    fi
done
```

## See Also

- [Branches Concept](../concepts/branches.md) — data isolation model
- [Branch Management Guide](../guides/branch-management.md) — creating and managing branches
- [Agent State Management](agent-state-management.md) — full session pattern
