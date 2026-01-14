#!/bin/bash
# Start a new story branch
# Usage: ./scripts/start-story.sh <epic-number> <story-number> <description>

EPIC_NUM=$1
STORY_NUM=$2
DESC=$3

if [ -z "$EPIC_NUM" ] || [ -z "$STORY_NUM" ] || [ -z "$DESC" ]; then
    echo "Usage: ./scripts/start-story.sh <epic-number> <story-number> <description>"
    echo "Example: ./scripts/start-story.sh 1 6 cargo-workspace"
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

EPIC_NAME=${EPIC_NAMES[$EPIC_NUM]}
EPIC_BRANCH="epic-${EPIC_NUM}-${EPIC_NAME}"
STORY_BRANCH="epic-${EPIC_NUM}-story-${STORY_NUM}-${DESC}"

echo "🚀 Starting story #${STORY_NUM}..."
echo ""

# Ensure on develop
git checkout develop
git pull origin develop

# Create or checkout epic branch
if git rev-parse --verify "$EPIC_BRANCH" >/dev/null 2>&1; then
    echo "✓ Epic branch exists: $EPIC_BRANCH"
    git checkout "$EPIC_BRANCH"
    git pull origin "$EPIC_BRANCH"
else
    echo "✓ Creating epic branch: $EPIC_BRANCH"
    git checkout -b "$EPIC_BRANCH"
    git push -u origin "$EPIC_BRANCH"
fi

# Create story branch
echo "✓ Creating story branch: $STORY_BRANCH"
git checkout -b "$STORY_BRANCH"
git push -u origin "$STORY_BRANCH"

echo ""
echo "✅ Story branch ready!"
echo ""
echo "Branch: $STORY_BRANCH"
echo "Epic: $EPIC_BRANCH"
echo ""
echo "Next steps:"
echo "1. Implement the story"
echo "2. Write tests"
echo "3. Run: ./scripts/complete-story.sh $STORY_NUM"

# M2-specific reminder (epics 6-12)
if [ "$EPIC_NUM" -ge 6 ] && [ "$EPIC_NUM" -le 12 ]; then
    echo ""
    echo "🔴 M2 REMINDER: Read docs/architecture/M2_TRANSACTION_SEMANTICS.md"
    echo "   This spec is GOSPEL. No deviations allowed."
fi
