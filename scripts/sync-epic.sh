#!/bin/bash
# Sync current story branch with epic branch
# Usage: ./scripts/sync-epic.sh <epic-number>

EPIC_NUM=$1

if [ -z "$EPIC_NUM" ]; then
    echo "Usage: ./scripts/sync-epic.sh <epic-number>"
    echo "Example: ./scripts/sync-epic.sh 1"
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
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)

echo "🔄 Syncing with epic branch..."
echo ""
echo "Current branch: $CURRENT_BRANCH"
echo "Epic branch: $EPIC_BRANCH"
echo ""

# Fetch latest
git fetch origin "$EPIC_BRANCH"

# Rebase onto epic branch
echo "⚙️  Rebasing..."
if git rebase "origin/$EPIC_BRANCH"; then
    echo ""
    echo "✅ Successfully synced with $EPIC_BRANCH"
    echo ""
    echo "Push with: git push --force-with-lease"
else
    echo ""
    echo "❌ Rebase conflicts detected"
    echo ""
    echo "Resolve conflicts, then:"
    echo "  git add <files>"
    echo "  git rebase --continue"
    echo "  git push --force-with-lease"
fi
