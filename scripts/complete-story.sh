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
EPIC_NAMES[1]="workspace-core-types"
EPIC_NAMES[2]="storage-layer"
EPIC_NAMES[3]="wal-implementation"
EPIC_NAMES[4]="basic-recovery"
EPIC_NAMES[5]="database-engine"

EPIC_NAME=${EPIC_NAMES[$EPIC_NUM]}
EPIC_BRANCH="epic-${EPIC_NUM}-${EPIC_NAME}"

echo "✓ Story branch: $CURRENT_BRANCH"
echo "✓ Epic branch: $EPIC_BRANCH"
echo ""

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
"$GH_PATH" pr create \
    --base "$EPIC_BRANCH" \
    --head "$CURRENT_BRANCH" \
    --title "Story #${STORY_NUM}: $(git log -1 --pretty=%s | sed 's/Implement story #[0-9]*: //')" \
    --body "Implements #${STORY_NUM}

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

echo ""
echo "✅ Pull request created!"
echo ""
echo "View PR: $GH_PATH pr view --web"
