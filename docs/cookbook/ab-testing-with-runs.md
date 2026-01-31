# A/B Testing with Runs

This recipe shows how to use runs to compare different agent strategies side by side.

## Pattern

1. Create one run per variant (strategy A, strategy B)
2. Run each strategy in its own isolated run
3. Compare the results by reading from each run

## Implementation

```rust
use stratadb::{Strata, Value};

fn ab_test(db: &mut Strata) -> stratadb::Result<()> {
    // Create runs for each variant
    db.create_run("variant-a")?;
    db.create_run("variant-b")?;

    // === Run Strategy A ===
    db.set_run("variant-a")?;

    db.kv_put("config:strategy", "conservative")?;
    db.kv_put("config:temperature", 0.3)?;

    // Simulate agent execution
    for i in 0..5 {
        let payload: Value = serde_json::json!({
            "step": i,
            "strategy": "conservative",
            "action": "careful_action",
        }).into();
        db.event_append("action", payload)?;
    }
    db.kv_put("result:score", 85i64)?;
    db.state_set("status", "completed")?;

    // === Run Strategy B ===
    db.set_run("variant-b")?;

    db.kv_put("config:strategy", "aggressive")?;
    db.kv_put("config:temperature", 0.9)?;

    // Simulate agent execution
    for i in 0..8 {
        let payload: Value = serde_json::json!({
            "step": i,
            "strategy": "aggressive",
            "action": "bold_action",
        }).into();
        db.event_append("action", payload)?;
    }
    db.kv_put("result:score", 92i64)?;
    db.state_set("status", "completed")?;

    // === Compare Results ===
    compare_variants(db, "variant-a", "variant-b")?;

    db.set_run("default")?;
    Ok(())
}

fn compare_variants(db: &mut Strata, a: &str, b: &str) -> stratadb::Result<()> {
    // Read variant A results
    db.set_run(a)?;
    let score_a = db.kv_get("result:score")?
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    let events_a = db.event_len()?;
    let strategy_a = db.kv_get("config:strategy")?;

    // Read variant B results
    db.set_run(b)?;
    let score_b = db.kv_get("result:score")?
        .and_then(|v| v.as_int())
        .unwrap_or(0);
    let events_b = db.event_len()?;
    let strategy_b = db.kv_get("config:strategy")?;

    println!("=== A/B Test Results ===");
    println!("Variant A ({:?}): score={}, actions={}", strategy_a, score_a, events_a);
    println!("Variant B ({:?}): score={}, actions={}", strategy_b, score_b, events_b);
    println!("Winner: {}", if score_a > score_b { a } else { b });

    Ok(())
}
```

## Scaling to Many Variants

```rust
fn multi_variant_test(db: &mut Strata, variants: &[(&str, f64)]) -> stratadb::Result<()> {
    // variants: (name, temperature) pairs

    for (name, temperature) in variants {
        let run_name = format!("variant-{}", name);
        db.create_run(&run_name)?;
        db.set_run(&run_name)?;

        db.kv_put("config:temperature", *temperature)?;
        // ... run the agent ...
        db.kv_put("result:score", 0i64)?; // placeholder
    }

    // Compare all variants
    db.set_run("default")?;
    let mut best_score = 0i64;
    let mut best_variant = String::new();

    for (name, _) in variants {
        let run_name = format!("variant-{}", name);
        db.set_run(&run_name)?;
        let score = db.kv_get("result:score")?
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        if score > best_score {
            best_score = score;
            best_variant = name.to_string();
        }
    }

    println!("Best variant: {} (score: {})", best_variant, best_score);

    // Clean up losing variants
    db.set_run("default")?;
    for (name, _) in variants {
        if *name != best_variant.as_str() {
            db.delete_run(&format!("variant-{}", name))?;
        }
    }

    Ok(())
}
```

## See Also

- [Runs Concept](../concepts/runs.md) — data isolation model
- [Run Management Guide](../guides/run-management.md) — creating and managing runs
- [Agent State Management](agent-state-management.md) — full session pattern
