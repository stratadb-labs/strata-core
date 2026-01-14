#!/bin/bash
# Complete a story and create PR
# Usage: ./scripts/complete-story.sh <story-number>

# Source Rust environment if it exists
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

STORY_NUM=$1

if [ -z "$STORY_NUM" ]; then
    echo "Usage: ./scripts/complete-story.sh <story-number>"
    echo "Example: ./scripts/complete-story.sh 6"
    exit 1
fi

echo "🔍 Checking story #${STORY_NUM}..."
echo ""

# Get current branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

# Extract epic number from branch name
if [[ $CURRENT_BRANCH =~ ^epic-([0-9]+)-story-${STORY_NUM}- ]]; then
    EPIC_NUM="${BASH_REMATCH[1]}"
else
    echo "❌ Not on a story-${STORY_NUM} branch"
    echo "Current branch: $CURRENT_BRANCH"
    exit 1
fi

# Epic name mapping
declare -A EPIC_NAMES
# M1 Epics
EPIC_NAMES[1]="workspace-core-types"
EPIC_NAMES[2]="storage-layer"
EPIC_NAMES[3]="wal-implementation"
EPIC_NAMES[4]="basic-recovery"
EPIC_NAMES[5]="database-engine"
# M2 Epics
EPIC_NAMES[6]="transaction-foundations"
EPIC_NAMES[7]="transaction-semantics"
EPIC_NAMES[8]="durability-commit"
EPIC_NAMES[9]="recovery-support"
EPIC_NAMES[10]="database-api-integration"
EPIC_NAMES[11]="backwards-compatibility"
EPIC_NAMES[12]="occ-validation-benchmarking"
# M3 Epics
EPIC_NAMES[13]="primitives-foundation"
EPIC_NAMES[14]="kvstore-primitive"
EPIC_NAMES[15]="eventlog-primitive"
EPIC_NAMES[16]="statecell-primitive"
EPIC_NAMES[17]="tracestore-primitive"
EPIC_NAMES[18]="runindex-primitive"
EPIC_NAMES[19]="integration-validation"

EPIC_NAME=${EPIC_NAMES[$EPIC_NUM]}
EPIC_BRANCH="epic-${EPIC_NUM}-${EPIC_NAME}"

echo "✓ Story branch: $CURRENT_BRANCH"
echo "✓ Epic branch: $EPIC_BRANCH"
echo ""

# Milestone-specific spec compliance reminder
if [ "$EPIC_NUM" -ge 6 ] && [ "$EPIC_NUM" -le 12 ]; then
    echo "🔴 M2 SPEC COMPLIANCE CHECK"
    echo "   Before completing this story, verify:"
    echo "   - Code complies with docs/architecture/M2_TRANSACTION_SEMANTICS.md"
    echo "   - Tests validate spec-compliant behavior"
    echo "   - No deviations from the spec for ANY reason"
    echo ""
elif [ "$EPIC_NUM" -ge 13 ] && [ "$EPIC_NUM" -le 19 ]; then
    echo "🔴 M3 SPEC COMPLIANCE CHECK"
    echo "   Before completing this story, verify:"
    echo "   - Code complies with docs/architecture/M3_ARCHITECTURE.md"
    echo "   - Primitives are stateless facades (only Arc<Database>)"
    echo "   - All operations scoped to RunId"
    echo "   - Invariants enforced by primitives"
    echo "   - No deviations from the spec for ANY reason"
    echo ""
fi

# Run checks
echo "🧪 Running tests..."
if ! cargo test --all; then
    echo "❌ Tests failed. Fix tests before creating PR."
    exit 1
fi

echo ""
echo "🎨 Checking formatting..."
if ! cargo fmt --all -- --check; then
    echo "❌ Formatting issues found. Run: cargo fmt --all"
    exit 1
fi

echo ""
echo "📎 Running clippy..."
if ! cargo clippy --all -- -D warnings; then
    echo "❌ Clippy warnings found. Fix before creating PR."
    exit 1
fi

echo ""
echo "✅ All checks passed!"
echo ""

# Push current branch
echo "📤 Pushing to origin..."
git push

echo ""
echo "🎯 Creating pull request..."
echo ""

# Create PR (use full path to gh)
GH_PATH="${GH_PATH:-/opt/homebrew/bin/gh}"
# Build PR body based on milestone
if [ "$EPIC_NUM" -ge 13 ] && [ "$EPIC_NUM" -le 19 ]; then
    # M3 PR body with spec compliance section
    PR_BODY="Implements #${STORY_NUM}

## Changes
$(git log --oneline ${EPIC_BRANCH}..HEAD | sed 's/^/- /')

## M3 Spec Compliance
- [ ] Code complies with \`docs/architecture/M3_ARCHITECTURE.md\`
- [ ] Primitive is stateless facade (only holds Arc<Database>)
- [ ] All operations scoped to RunId
- [ ] Invariants enforced by primitive
- [ ] No spec deviations for any reason

## Testing
- [x] Tests pass: \`cargo test --all\`
- [x] Formatting: \`cargo fmt --all -- --check\`
- [x] Linting: \`cargo clippy --all -- -D warnings\`

## Checklist
- [x] Code written
- [x] Tests added
- [x] Documentation updated
- [x] CI ready to pass"
elif [ "$EPIC_NUM" -ge 6 ] && [ "$EPIC_NUM" -le 12 ]; then
    # M2 PR body with spec compliance section
    PR_BODY="Implements #${STORY_NUM}

## Changes
$(git log --oneline ${EPIC_BRANCH}..HEAD | sed 's/^/- /')

## M2 Spec Compliance
- [ ] Code complies with \`docs/architecture/M2_TRANSACTION_SEMANTICS.md\`
- [ ] Isolation level: Snapshot Isolation (NOT Serializability)
- [ ] Visibility rules match spec exactly
- [ ] Conflict detection follows spec
- [ ] No spec deviations for any reason

## Testing
- [x] Tests pass: \`cargo test --all\`
- [x] Formatting: \`cargo fmt --all -- --check\`
- [x] Linting: \`cargo clippy --all -- -D warnings\`

## Checklist
- [x] Code written
- [x] Tests added
- [x] Documentation updated
- [x] CI ready to pass"
else
    # M1 PR body (original)
    PR_BODY="Implements #${STORY_NUM}

## Changes
$(git log --oneline ${EPIC_BRANCH}..HEAD | sed 's/^/- /')

## Testing
- [x] Tests pass: \`cargo test --all\`
- [x] Formatting: \`cargo fmt --all -- --check\`
- [x] Linting: \`cargo clippy --all -- -D warnings\`

## Checklist
- [x] Code written
- [x] Tests added
- [x] Documentation updated
- [x] CI ready to pass"
fi

"$GH_PATH" pr create \
    --base "$EPIC_BRANCH" \
    --head "$CURRENT_BRANCH" \
    --title "Story #${STORY_NUM}: $(git log -1 --pretty=%s | sed 's/Implement story #[0-9]*: //')" \
    --body "$PR_BODY"

echo ""
echo "✅ Pull request created!"
echo ""
echo "View PR: $GH_PATH pr view --web"
