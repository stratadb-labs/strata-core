# Strata Magic APIs

> **Status**: Vision Document
> **Stability**: Aspirational (Post-MVP)
> **Scope**: The five APIs that make Strata unique

---

## Purpose

This document describes the **five APIs** that transform Strata from "agent storage" into "a substrate for reasoning about agent behavior over time."

These are not basic CRUD operations. These are the features that make Strata magical.

---

## Relationship to Core

| Layer | Document | What It Defines |
|-------|----------|-----------------|
| Invariants | PRIMITIVE_CONTRACT.md | What must be true |
| API Shape | CORE_API_SHAPE.md | How to express operations |
| Product Surfaces | PRODUCT_SURFACES.md | Features built on core |
| **Magic APIs** | This document | What makes Strata unique |

The Magic APIs are built on top of the invariants:
- **replay()** requires Invariant 2 (Everything is Versioned)
- **diff()** requires Invariant 2 + Invariant 6 (Introspectable)
- **branch()** requires Invariant 5 (Run-scoped) + Invariant 2
- **explain()** requires Invariant 3 (Transactional) + Invariant 6
- **search()** requires all invariants working together

---

## The Five Magic APIs

### 1. replay(): Time Travel

**What it does**: Reconstructs the full world state at any point in time.

**Spec**:
```rust
/// Replay entire run from beginning
fn replay(run_id: &RunId) -> Result<View>;

/// Replay run until specific point
fn replay_until(run_id: &RunId, timestamp: Timestamp) -> Result<View>;

/// Replay a range, returning the event sequence
fn replay_range(run_id: &RunId, t1: Timestamp, t2: Timestamp) -> Result<EventSequence>;
```

**Cross-primitive behavior**:

| Primitive | What Gets Replayed |
|-----------|-------------------|
| KV | Full key-value state at point in time |
| JSON | Document state with all paths |
| StateCell | Cell values |
| Vector | Embeddings and metadata |
| Trace | Execution history |
| Event | Event log up to that point |
| Run | Run metadata and status |

**Why this is magical**:

This makes Strata:
- A **debugger**: Step through agent execution
- A **simulator**: Re-run scenarios from any point
- A **game engine substrate**: Save states, load states
- A **robotics memory**: Replay sensor/actuator history
- A **learning loop engine**: Learn from past runs

**Most databases forget history. Strata remembers.**

**Implementation notes**:
- Built on WAL replay infrastructure (M7)
- Requires version retention policy
- Can be expensive for long runs - consider checkpointing

---

### 2. diff(): Change Intelligence

**What it does**: Explains what changed between two worlds.

**Spec**:
```rust
/// Diff two different runs
fn diff_runs(run_a: &RunId, run_b: &RunId) -> Result<Diff>;

/// Diff two views (snapshots)
fn diff_states(view_a: &View, view_b: &View) -> Result<Diff>;

/// Diff a run between two points in time
fn diff_range(run_id: &RunId, t1: Timestamp, t2: Timestamp) -> Result<Diff>;
```

**Cross-primitive behavior**:

| Primitive | Diff Output |
|-----------|-------------|
| KV | Keys added, removed, modified (with values) |
| JSON | Path-level diffs (added, removed, changed) |
| StateCell | State transitions |
| Vector | Vectors added, removed, similarity changes |
| Trace | Trace divergences |
| Event | Event deltas (new events, missing events) |

**Why this is magical**:

This enables:
- **Regression detection**: "What changed between working and broken?"
- **Learning systems**: "How did the agent improve?"
- **Game balance analysis**: "What shifted after the patch?"
- **Behavior evolution**: "How did strategy change over time?"
- **Policy tuning**: "What's the delta from this tweak?"

**No major database has a native diff engine.**

**Implementation notes**:
- Diff computation can be expensive - consider caching
- Structural diffs (JSON) are more complex than value diffs (KV)
- May need configurable diff granularity

---

### 3. branch(): Counterfactuals

**What it does**: Forks reality.

**Spec**:
```rust
/// Branch from a specific point in time
fn branch_from(run_id: &RunId, timestamp: Timestamp) -> Result<RunId>;

/// Fork current state (branch from now)
fn fork(run_id: &RunId) -> Result<RunId>;
```

**Cross-primitive behavior**:

All primitives are snapshotted into the new branch:
- KV entries copied
- JSON documents copied
- StateCells copied
- Vectors copied
- Traces start fresh (new branch, new traces)
- Events start fresh (new branch, new events)
- Run metadata indicates parent run and branch point

**Why this is magical**:

This enables:
- **Save states**: Checkpoint and restore
- **What-if simulations**: "What if the agent chose differently?"
- **Planning systems**: Explore multiple futures
- **Game timelines**: Branching narratives
- **Monte Carlo rollouts**: Sample many possible futures
- **Agent imagination**: "What would happen if...?"

**This turns Strata into a multiverse engine.**

**Implementation notes**:
- Copy-on-write for efficiency (don't duplicate unchanged data)
- Branch metadata tracks lineage
- Consider branch limits (cleanup policy)

---

### 4. explain(): Causal Reasoning

**What it does**: Explains why something is the way it is.

**Spec**:
```rust
/// Explain current state of a key
fn explain(entity_ref: &EntityRef) -> Result<Explanation>;

/// Explain entire view state
fn explain_state(view: &View) -> Result<Explanation>;

/// Explain a specific transition
fn explain_transition(entity_ref: &EntityRef, timestamp: Timestamp) -> Result<Explanation>;
```

**Output structure**:
```rust
struct Explanation {
    /// The entity being explained
    entity: EntityRef,

    /// The value/state being explained
    current_state: Value,

    /// Prior states that led here
    prior_states: Vec<VersionedValue>,

    /// Events that influenced this state
    related_events: Vec<Event>,

    /// Decisions/operations that modified this
    operations: Vec<Operation>,

    /// Trace steps that touched this entity
    trace_steps: Vec<TraceStep>,

    /// Causal chain (what caused what)
    causal_chain: Vec<CausalLink>,
}
```

**Why this is magical**:

This is not LLM explainability. This is **system explainability**.

This enables:
- **Debugging**: "Why is this value wrong?"
- **Trust**: "How did the agent reach this conclusion?"
- **Auditing**: "What led to this decision?"
- **Safety**: "Was this change intentional?"
- **Learning loops**: "What inputs produced this output?"

**This is incredibly rare.** Most systems can tell you *what* the state is. Strata can tell you *why*.

**Implementation notes**:
- Requires tracking read/write sets in transactions
- May need explicit causality hints for complex chains
- Depth-limited by default (full chain can be expensive)

---

### 5. search(): Semantic Memory Over Time

**What it does**: Searches history, not just current state.

**Spec**:
```rust
/// Search across historical states
fn search_states(query: SearchQuery) -> Result<Vec<StateMatch>>;

/// Search event history
fn search_events(query: SearchQuery) -> Result<Vec<EventMatch>>;

/// Search trace history
fn search_traces(query: SearchQuery) -> Result<Vec<TraceMatch>>;

/// Search across runs
fn search_runs(query: SearchQuery) -> Result<Vec<RunMatch>>;
```

**Cross-primitive behavior**:

Combines:
- **Keyword search**: Text matching
- **Vector similarity**: Semantic search
- **Structural filters**: JSON path queries, metadata filters
- **Temporal constraints**: Time ranges, version ranges

**Why this is magical**:

This lets you ask:
- "When did this NPC become aggressive?"
- "Show me runs where the agent hesitated."
- "Find moments like this." (semantic similarity over time)
- "What was the agent doing when this metric spiked?"
- "Find all runs that reached this state."

**This is time-aware semantic memory.**

**Implementation notes**:
- Builds on M6 retrieval infrastructure
- Requires indexing historical states (storage tradeoff)
- May need configurable retention for search indexes

---

## Why These Five?

These APIs were chosen because they address the **unique needs of agent systems**:

| Need | Traditional DB | Strata Magic API |
|------|---------------|------------------|
| Debug agent behavior | Log files, manual inspection | `replay()` + `explain()` |
| Compare agent versions | Manual diffing | `diff()` |
| Explore counterfactuals | Not possible | `branch()` |
| Find patterns in history | Custom analytics | `search()` |
| Understand causality | Not tracked | `explain()` |

**Together, these APIs make Strata a reasoning substrate, not just storage.**

---

## Implementation Roadmap

These APIs are **Post-MVP**. They require:

1. **Solid foundation** (M1-M8): All primitives working, versioned, transactional
2. **Stable API** (M9): Universal protocol defined
3. **History infrastructure**: Version retention, efficient snapshots
4. **Causality tracking**: Read/write set tracking, optional causality hints

**Suggested order**:
1. `replay()` - Most foundational, builds on M7 WAL replay
2. `diff()` - Builds on replay, natural extension
3. `search()` (temporal) - Extends M6 search to history
4. `branch()` - Requires efficient copy-on-write
5. `explain()` - Most complex, requires causality tracking

---

## What This Enables

With these five APIs, Strata becomes:

| Use Case | How |
|----------|-----|
| **Agent Debugger** | `replay()` to step through, `explain()` to understand |
| **Simulation Platform** | `branch()` to fork, `diff()` to compare outcomes |
| **Learning System** | `search()` to find patterns, `diff()` to measure improvement |
| **Game Engine Memory** | `branch()` for save states, `replay()` for replays |
| **Robotics Memory** | `replay()` sensor history, `explain()` decisions |
| **Audit System** | `explain()` for accountability, `search()` for investigation |

**Strata isn't just where agents store data. It's where agents reason about their history.**

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-19 | Initial vision document |
