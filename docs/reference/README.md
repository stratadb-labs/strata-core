# in-mem Reference Documentation

Complete reference documentation for **in-mem** - a fast, durable, embedded database for AI agent workloads.

## Quick Links

### For Users

- **[Getting Started](getting-started.md)** - Installation, quick start, common patterns
- **[API Reference](api-reference.md)** - Complete API documentation
- **[Architecture Overview](architecture.md)** - How in-mem works internally

### For Developers

- **[M1 Architecture Spec](../architecture/M1_ARCHITECTURE.md)** - Storage layer specification
- **[M2 Transaction Semantics](../architecture/M2_TRANSACTION_SEMANTICS.md)** - OCC and snapshot isolation specification
- **[Development Workflow](../development/DEVELOPMENT_WORKFLOW.md)** - Git workflow
- **[TDD Methodology](../development/TDD_METHODOLOGY.md)** - Testing approach

### Project Information

- **[Milestones](../milestones/MILESTONES.md)** - Roadmap M1-M5
- **[GitHub Repository](https://github.com/anibjoshi/in-mem)** - Source code

## Documentation Structure

```
docs/
├── reference/              # User-facing reference docs
│   ├── getting-started.md  # Quick start guide
│   ├── api-reference.md    # Complete API reference
│   └── architecture.md     # Architecture overview
│
├── architecture/           # Technical specifications
│   ├── M1_ARCHITECTURE.md  # Storage layer spec
│   └── M2_TRANSACTION_SEMANTICS.md  # OCC & snapshot isolation spec
│
├── development/            # Developer guides
│   ├── GETTING_STARTED.md  # Developer onboarding
│   ├── TDD_METHODOLOGY.md  # Testing strategy
│   └── DEVELOPMENT_WORKFLOW.md  # Git workflow
│
├── diagrams/               # Architecture diagrams
│   └── m1-architecture.md  # Visual diagrams
│
└── milestones/             # Project management
    └── MILESTONES.md       # Roadmap
```

## What is in-mem?

**in-mem** is an embedded database designed specifically for AI agent workloads. It provides:

- **Run-Scoped Operations**: Every operation tagged with a RunId for deterministic replay
- **Unified Storage**: Six primitives (KV, Event Log, State Machine, Trace, Vector, Run Index) sharing one storage layer
- **Durable by Default**: Write-ahead log with configurable fsync modes
- **Embedded Library**: Zero-copy in-process API (network layer in M7)

### Current Status: M2 Transactions Complete ✅

- ✅ 630+ tests
- ✅ OCC with snapshot isolation
- ✅ Multi-key atomic transactions
- ✅ Compare-and-swap (CAS) operations
- ✅ 37K txns/s (canonical agent workload)
- ✅ >95% success rate under contention

See [Milestones](../milestones/MILESTONES.md) for roadmap.

## Quick Start

```rust
use in_mem::Database;

// Open database
let db = Database::open("./my-agent-db")?;

// Begin a run
let run_id = db.begin_run();

// Simple operations
db.put(run_id, &key, value)?;
let value = db.get(run_id, &key)?;

// Atomic transactions (M2)
db.transaction(run_id, |txn| {
    let val = txn.get(&key)?;
    txn.put(key.clone(), new_value)?;
    Ok(())
})?;

// Compare-and-swap (M2)
db.cas(run_id, key, expected_version, new_value)?;

// End run
db.end_run(run_id)?;
```

See [Getting Started](getting-started.md) for full guide.

## Support

- **Issues**: [GitHub Issues](https://github.com/anibjoshi/in-mem/issues)
- **Discussions**: [GitHub Discussions](https://github.com/anibjoshi/in-mem/discussions)
- **Documentation**: This site

## License

[MIT License](https://github.com/anibjoshi/in-mem/blob/main/LICENSE)

---

**Version**: 0.2.0 (M2 Transactions)
**Last Updated**: 2026-01-14
