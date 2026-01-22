# M7 Durability, Snapshots, Replay & Storage Stabilization - Autonomous Execution Prompt

**Usage**: `claude --dangerously-skip-permissions -p "$(cat docs/prompts/M7/M7_AUTONOMOUS_EXECUTION.md)"`

---

## Task

Execute M7 Epics 40-46 sequentially with epic-end validation after each.

## M7 Philosophy

> M7 is not about features. M7 is about truth.
>
> After crash recovery, the database must correspond to a **prefix of the committed transaction history**. No partial transactions may be visible. If a transaction spans KV + JSON + Event + State, after crash recovery you must see either all effects or none.

## Execution Pattern

For each epic (42 → 40 → 41 → 44 → 43 → 45 → 46):

1. **Read specs first**:
   - `docs/architecture/M7_ARCHITECTURE.md` (AUTHORITATIVE)
   - `docs/prompts/M7/epic-{N}-claude-prompts.md`
   - `docs/milestones/M7/EPIC_{N}_*.md`

2. **Start epic branch**: `./scripts/start-story.sh {epic} {first-story} {desc}`

3. **Implement all stories** per epic prompt

4. **Run epic-end validation**: `docs/prompts/EPIC_END_VALIDATION.md` Phase 6d

5. **Merge to develop**:
   ```bash
   git checkout develop
   git merge --no-ff epic-{N}-* -m "Epic {N}: {Name} complete"
   git push origin develop
   ```

6. **Proceed to next epic**

## Recommended Execution Order

The implementation plan recommends this order based on dependencies:

1. **Epic 42: WAL Enhancement** (CRC32, transaction framing) - CRITICAL FOUNDATION
2. **Epic 40: Snapshot Format & Writer** - depends on WAL format
3. **Epic 41: Crash Recovery** - depends on Snapshot + WAL
4. **Epic 44: Cross-Primitive Atomicity** - depends on WAL transaction framing
5. **Epic 43: Run Lifecycle & Replay** - depends on Recovery
6. **Epic 45: Storage Stabilization** - depends on Recovery + WAL
7. **Epic 46: Validation & Benchmarks** - depends on ALL

## GitHub Issue Mapping

| Epic | GitHub Issue | Story Issues |
|------|--------------|--------------|
| Epic 40 | #338 | #347-#352 |
| Epic 41 | #339 | #353-#359 |
| Epic 42 | #340 | #360-#364 |
| Epic 43 | #341 | #365-#371 |
| Epic 44 | #342 | #372-#375 |
| Epic 45 | #343 | #376-#380 |
| Epic 46 | #344 | #381-#384 |

## Stop Conditions

- Any architectural rule violation
- Epic-end validation failure
- Test failures that can't be resolved
- Recovery invariant violation (R1-R6)
- Replay invariant violation (P1-P6)
- M4/M5/M6 non-regression failures

## Start

Begin with Epic 42: WAL Enhancement. Read the specs, implement, validate, merge, continue.
