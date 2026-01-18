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
# M3 Epics
EPIC_NAMES[13]="primitives-foundation"
EPIC_NAMES[14]="kvstore-primitive"
EPIC_NAMES[15]="eventlog-primitive"
EPIC_NAMES[16]="statecell-primitive"
EPIC_NAMES[17]="tracestore-primitive"
EPIC_NAMES[18]="runindex-primitive"
EPIC_NAMES[19]="integration-validation"
# M4 Epics
EPIC_NAMES[20]="performance-foundation"
EPIC_NAMES[21]="durability-modes"
EPIC_NAMES[22]="sharded-storage"
EPIC_NAMES[23]="transaction-pooling"
EPIC_NAMES[24]="read-path-optimization"
EPIC_NAMES[25]="validation-red-flags"
# M5 Epics
EPIC_NAMES[26]="core-types"
EPIC_NAMES[27]="path-operations"
EPIC_NAMES[28]="jsonstore-core"
EPIC_NAMES[29]="wal-integration"
EPIC_NAMES[30]="transaction-integration"
EPIC_NAMES[31]="conflict-detection"
EPIC_NAMES[32]="validation"
# M6 Epics
EPIC_NAMES[33]="core-search-types"
EPIC_NAMES[34]="primitive-search"
EPIC_NAMES[35]="scoring"
EPIC_NAMES[36]="composite-search"
EPIC_NAMES[37]="fusion"
EPIC_NAMES[38]="indexing"
EPIC_NAMES[39]="validation"

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

# Milestone-specific reminders
if [ "$EPIC_NUM" -ge 6 ] && [ "$EPIC_NUM" -le 12 ]; then
    echo ""
    echo "🔴 M2 REMINDER: Read docs/architecture/M2_TRANSACTION_SEMANTICS.md"
    echo "   This spec is GOSPEL. No deviations allowed."
elif [ "$EPIC_NUM" -ge 13 ] && [ "$EPIC_NUM" -le 19 ]; then
    echo ""
    echo "🔴 M3 REMINDER: Read docs/architecture/M3_ARCHITECTURE.md"
    echo "   This spec is GOSPEL. No deviations allowed."
    echo "   See also: docs/prompts/M3_PROMPT_HEADER.md"
elif [ "$EPIC_NUM" -ge 20 ] && [ "$EPIC_NUM" -le 25 ]; then
    echo ""
    echo "🔴 M4 REMINDER: Read docs/architecture/M4_ARCHITECTURE.md"
    echo "   This spec is GOSPEL. No deviations allowed."
    echo "   See also: docs/prompts/M4_PROMPT_HEADER.md"
    echo "   CRITICAL: Red flag thresholds are non-negotiable."
elif [ "$EPIC_NUM" -ge 26 ] && [ "$EPIC_NUM" -le 32 ]; then
    echo ""
    echo "🔴 M5 REMINDER: Read docs/architecture/M5_ARCHITECTURE.md"
    echo "   THIS IS THE AUTHORITATIVE SPEC. No deviations allowed."
    echo ""
    echo "   Implementation details: docs/milestones/M5/M5_IMPLEMENTATION_PLAN.md"
    echo "   Story specs: docs/milestones/M5/EPIC_${EPIC_NUM}_*.md"
    echo ""
    echo "   The SIX ARCHITECTURAL RULES are NON-NEGOTIABLE:"
    echo "   1. JSON lives in ShardedStore (no separate DashMap)"
    echo "   2. JsonStore is stateless (Arc<Database> only)"
    echo "   3. JSON extends TransactionContext (no separate type)"
    echo "   4. Path semantics in API layer (not storage)"
    echo "   5. WAL remains unified (entry types 0x20-0x23)"
    echo "   6. JSON API feels like other primitives"
    echo ""
    echo "   TypeTag::Json = 0x11 (not 0x06)"
    echo "   See also: docs/prompts/M5/M5_PROMPT_HEADER.md"
elif [ "$EPIC_NUM" -ge 33 ] && [ "$EPIC_NUM" -le 39 ]; then
    echo ""
    echo "🔴 M6 REMINDER: Read docs/architecture/M6_ARCHITECTURE.md"
    echo "   THIS IS THE AUTHORITATIVE SPEC. No deviations allowed."
    echo ""
    echo "   Story specs: docs/milestones/M6/EPIC_${EPIC_NUM}_*.md"
    echo "   Implementation prompts: docs/prompts/M6/epic-${EPIC_NUM}-claude-prompts.md"
    echo ""
    echo "   The SIX ARCHITECTURAL RULES are NON-NEGOTIABLE:"
    echo "   1. No Data Movement - DocRef references only, no content copying"
    echo "   2. Primitive Search is First-Class - every primitive has .search()"
    echo "   3. Composite Orchestrates, Doesn't Replace - db.hybrid() delegates to primitives"
    echo "   4. Snapshot-Consistent Search - single snapshot for all primitive searches"
    echo "   5. Zero Overhead When Disabled - no allocations when indexing off"
    echo "   6. Algorithm Swappable - Scorer and Fuser are traits, not hardcoded"
    echo ""
    echo "   Key types: SearchRequest, SearchResponse, SearchHit, DocRef, PrimitiveKind"
    echo "   Scorer: BM25LiteScorer (default), Fuser: RRFFuser (Reciprocal Rank Fusion)"
    echo "   See also: docs/prompts/M6/M6_PROMPT_HEADER.md"
fi
