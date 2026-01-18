# M6 Retrieval Surfaces - Autonomous Execution Prompt

**Usage**: `claude --dangerously-skip-permissions -p "$(cat docs/prompts/M6/M6_AUTONOMOUS_EXECUTION.md)"`

---

## Task

Execute M6 Epics 33-39 sequentially with epic-end validation after each.

## Execution Pattern

For each epic (33 → 34 → 35 → 36 → 37 → 38 → 39):

1. **Read specs first**:
   - `docs/architecture/M6_ARCHITECTURE.md` (AUTHORITATIVE)
   - `docs/prompts/M6/epic-{N}-claude-prompts.md`
   - `docs/milestones/M6/EPIC_{N}_*.md`

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

## Stop Conditions

- Any architectural rule violation
- Epic-end validation failure
- Test failures that can't be resolved
- M4/M5 non-regression failures

## Start

Begin with Epic 33: Core Search Types. Read the specs, implement, validate, merge, continue.
