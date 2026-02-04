# StrataDB Documentation

Welcome to the StrataDB documentation. StrataDB is an embedded database for AI agents, providing six data primitives with branch-based isolation, OCC transactions, and three durability modes.

## Quick Links

| I want to... | Go to |
|---|---|
| Install StrataDB and run my first example | [Getting Started](getting-started/installation.md) |
| Understand what branches are and how data isolation works | [Concepts: Branches](concepts/branches.md) |
| Organize data within branches using spaces | [Guide: Spaces](guides/spaces.md) |
| Learn how to use a specific primitive | [Guides](guides/index.md) |
| See every method at a glance | [API Quick Reference](reference/api-quick-reference.md) |
| Build a real-world pattern (agent state, RAG, etc.) | [Cookbook](cookbook/index.md) |
| Understand the architecture | [Architecture Overview](architecture/index.md) |
| Contribute to StrataDB | [Contributing](../CONTRIBUTING.md) |

## For Users

### [Getting Started](getting-started/installation.md)

Installation, feature flags, and a step-by-step tutorial that covers all six primitives.

### [Concepts](concepts/index.md)

Core ideas you need to understand: [branches](concepts/branches.md), [primitives](concepts/primitives.md), [value types](concepts/value-types.md), [transactions](concepts/transactions.md), and [durability](concepts/durability.md).

### [Guides](guides/index.md)

Per-primitive walkthroughs: [KV Store](guides/kv-store.md), [Event Log](guides/event-log.md), [State Cell](guides/state-cell.md), [JSON Store](guides/json-store.md), [Vector Store](guides/vector-store.md), [Branch Management](guides/branch-management.md). Plus cross-cutting guides on [spaces](guides/spaces.md), [observability](guides/observability.md), [search](guides/search.md), [sessions and transactions](guides/sessions-and-transactions.md), [branch bundles](guides/branch-bundles.md), [configuration](guides/database-configuration.md), and [error handling](guides/error-handling.md).

### [Cookbook](cookbook/index.md)

Recipes for real-world patterns: [agent state management](cookbook/agent-state-management.md), [multi-agent coordination](cookbook/multi-agent-coordination.md), [RAG with vectors](cookbook/rag-with-vectors.md), [deterministic replay](cookbook/deterministic-replay.md), and [A/B testing with branches](cookbook/ab-testing-with-branches.md).

### [Reference](reference/index.md)

Complete specifications: [API quick reference](reference/api-quick-reference.md), [value types](reference/value-type-reference.md), [errors](reference/error-reference.md), [commands](reference/command-reference.md), and [configuration](reference/configuration-reference.md).

### [Troubleshooting](troubleshooting.md) and [FAQ](faq.md)

Common issues, error messages, and frequently asked questions.

## For Contributors

### [Contributing](../CONTRIBUTING.md)

Development setup, workspace structure, running tests, code style, and pull request process.

### [Architecture](architecture/index.md)

How StrataDB works internally: [crate structure](architecture/crate-structure.md), [storage engine](architecture/storage-engine.md), [durability and recovery](architecture/durability-and-recovery.md), and [concurrency model](architecture/concurrency-model.md).

### [Changelog](../CHANGELOG.md)

Release history and notable changes.
