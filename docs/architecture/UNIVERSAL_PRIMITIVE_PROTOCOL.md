# Strata Universal Primitive Protocol

> **Status**: Superseded - Split into three focused documents
> **Date**: 2026-01-19

---

## This Document Has Been Restructured

The original Universal Primitive Protocol document has been split into three separate documents with different stability levels:

### 1. [PRIMITIVE_CONTRACT.md](PRIMITIVE_CONTRACT.md) - The Invariants

**Stability**: Constitutional (changes require RFC)

Defines the **seven invariants** that every Strata primitive must obey:

1. Everything is Addressable
2. Everything is Versioned
3. Everything is Transactional
4. Everything Has a Lifecycle
5. Everything Exists Within a Run
6. Everything is Introspectable
7. Reads and Writes Have Consistent Semantics

This is the **substrate constitution**. It answers: "What must be true for something to be a Strata primitive?"

### 2. [CORE_API_SHAPE.md](CORE_API_SHAPE.md) - The API Pattern

**Stability**: Stable (changes require migration path)

Defines **how the invariants are expressed in code**:

- `EntityRef`: Universal addressing
- `Versioned<T>`: Universal version wrapper
- `Transaction`: Universal transaction pattern
- Handle patterns for each primitive
- Error handling patterns

This is the **API structure**. It answers: "How do users express operations against primitives?"

### 3. [PRODUCT_SURFACES.md](PRODUCT_SURFACES.md) - The Features

**Stability**: Evolving (can change with user needs)

Defines **capabilities built on top of the core**:

- Search and discovery
- History and time travel
- Diff and change tracking
- Provenance
- CLI
- Wire protocol
- Language SDKs
- Observability
- Administration

This is the **feature layer**. It answers: "What can users do beyond basic CRUD?"

---

## Why the Split?

The original document conflated three concerns with different stability requirements:

| Concern | Changes Should Be... | Original Treatment |
|---------|---------------------|-------------------|
| Invariants | Rare, RFC-gated | Mixed with API details |
| API Shape | Careful, backwards-compatible | Mixed with product features |
| Product Surfaces | Frequent, user-driven | Over-specified too early |

By separating them:

1. **Invariants** are protected from casual changes
2. **API shape** can evolve without breaking invariants
3. **Product surfaces** can experiment without destabilizing the core

---

## The Core Insight (Preserved)

The fundamental insight from this document remains:

> **There is one interaction model, not seven.**

Users learn one mental model. That model applies to all primitives. The primitives may have different operations internally, but externally they must feel the same.

This is expressed through the seven invariants in PRIMITIVE_CONTRACT.md.

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-19 | Initial comprehensive document |
| 2.0 | 2026-01-19 | Split into three focused documents |
