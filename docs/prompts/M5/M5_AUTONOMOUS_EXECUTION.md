# M5 JSON Primitive - Autonomous Execution Prompt

**Usage**: `claude --dangerously-skip-permissions -p "$(cat docs/prompts/M5/M5_AUTONOMOUS_EXECUTION.md)"`

---

## Task

Execute M5 Epics 26-32 sequentially with epic-end validation after each.

## Execution Pattern

For each epic (26 → 27 → 28 → 29 → 30 → 31 → 32):

1. **Read specs first**:
   - `docs/architecture/M5_ARCHITECTURE.md` (AUTHORITATIVE)
   - `docs/prompts/M5/epic-{N}-claude-prompts.md`
   - `docs/milestones/M5/EPIC_{N}_*.md`

2. **Start epic branch**: `./scripts/start-story.sh {epic} {first-story} {desc}`

3. **Implement all stories** per epic prompt

4. **Run epic-end validation**: `docs/prompts/EPIC_END_VALIDATION.md` Phase 6c

5. **Merge to develop**:
   ```bash
   git checkout develop
   git merge --no-ff epic-{N}-* -m "Epic {N}: {Name} complete"
   git push origin develop
   ```

6. **Proceed to next epic**

## Stop Conditions

- Any architectural rule violation
- Epic-end validation failure
- Test failures that can't be resolved

## Start

Begin with Epic 26: Core Types. Read the specs, implement, validate, merge, continue.
