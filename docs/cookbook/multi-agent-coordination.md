# Multi-Agent Coordination

This recipe shows how multiple agents can coordinate through a shared StrataDB database using State Cells for CAS-based coordination and branches for isolation.

## Pattern 1: Shared Database, Same Branch (CAS Coordination)

Multiple agents share a branch and use compare-and-swap to coordinate:

```rust
use stratadb::{Strata, Value};

fn agent_worker(db: &Strata, agent_id: &str) -> stratadb::Result<()> {
    // Try to claim the next task using CAS
    loop {
        let current = db.state_read("task:next")?;
        let task_id = match current {
            Some(v) => v.as_int().unwrap_or(0),
            None => {
                // Initialize task counter
                db.state_init("task:next", 0i64)?;
                0
            }
        };

        // Try to increment (claim this task)
        // CAS ensures only one agent gets each task
        let result = db.state_cas("task:next", Some(task_id as u64 + 1), (task_id + 1) as i64)?;
        if result.is_some() {
            // We claimed task_id
            println!("Agent {} claimed task {}", agent_id, task_id);

            // Record assignment
            db.kv_put(
                &format!("task:{}:owner", task_id),
                agent_id,
            )?;
            db.kv_put(
                &format!("task:{}:status", task_id),
                "in_progress",
            )?;

            break;
        }
        // CAS failed — another agent claimed it, retry
    }

    Ok(())
}
```

## Pattern 2: Different Branches (Full Isolation)

Each agent works in its own branch, then results are aggregated:

```rust
fn run_parallel_agents(db: &mut Strata) -> stratadb::Result<()> {
    // Create a branch per agent
    for i in 0..3 {
        let branch_name = format!("agent-{}", i);
        db.create_branch(&branch_name)?;
    }

    // Each agent works independently in its branch
    // (In practice, this would be on different threads)
    for i in 0..3 {
        let branch_name = format!("agent-{}", i);
        db.set_branch(&branch_name)?;

        db.kv_put("result", format!("Agent {} result", i))?;
        db.state_set("status", "done")?;
    }

    // Aggregate results from the default branch
    db.set_branch("default")?;
    for i in 0..3 {
        let branch_name = format!("agent-{}", i);
        db.set_branch(&branch_name)?;

        let result = db.kv_get("result")?;
        println!("Agent {} result: {:?}", i, result);

        db.set_branch("default")?;
    }

    Ok(())
}
```

## Pattern 3: Leader Election

Use StateCell CAS for simple leader election:

```rust
fn try_become_leader(db: &Strata, agent_id: &str) -> stratadb::Result<bool> {
    // Try to create the leader cell (only succeeds if it doesn't exist)
    match db.state_cas("leader", None, agent_id)? {
        Some(_) => {
            println!("{} is now the leader", agent_id);
            Ok(true)
        }
        None => {
            let current = db.state_read("leader")?;
            println!("{} is not the leader (current: {:?})", agent_id, current);
            Ok(false)
        }
    }
}
```

## See Also

- [State Cell Guide](../guides/state-cell.md) — CAS semantics
- [Branch Management Guide](../guides/branch-management.md) — creating and switching branches
- [Transactions Guide](../guides/sessions-and-transactions.md) — atomic multi-key operations
