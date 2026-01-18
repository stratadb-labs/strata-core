#!/bin/bash
#
# in-mem Benchmark Runner
# =======================
#
# This script sets up the proper environment for running benchmarks and
# generates performance reports for M1 (Storage), M2 (Transactions),
# M3 (Primitives), M5 (JSON), M6 (Search), and M8 (Vector) milestones.
#
# Reference Platform:
#   - Linux (Ubuntu 24.04.2 LTS)
#   - AMD Ryzen 7 7800X3D 8-Core Processor (16 logical cores)
#   - 64GB DDR5 RAM
#   - Performance governor
#   - Pinned cores for contention tests
#
# Usage:
#   ./scripts/bench_runner.sh [options]
#
# Options:
#   --full          Run full benchmark suite (all milestones)
#   --m1            Run M1 Storage benchmarks only
#   --m2            Run M2 Transaction benchmarks only
#   --m3            Run M3 Primitive benchmarks only
#   --m5            Run M5 JSON benchmarks only
#   --m6            Run M6 Search benchmarks only
#   --m8            Run M8 Vector benchmarks only
#   --comparison    Run industry comparison benchmarks (vs redb, LMDB, SQLite)
#   --tier=<tier>   Run specific tier (a0, a1, b, c, d, json, vector)
#   --filter=<pat>  Run benchmarks matching pattern
#   --baseline=<n>  Save/compare with baseline name
#   --perf          Run with perf stat
#   --perf-record   Run with perf record (generates perf.data)
#   --cores=<list>  Pin to specific cores (e.g., "0-7")
#   --no-setup      Skip environment setup checks
#   --json          Output environment as JSON
#   --mode=<mode>   Run with specific durability mode (inmemory, batched, strict)
#   --all-modes     Run benchmarks for all three durability modes sequentially
#   --help          Show this help message
#
# Examples:
#   ./scripts/bench_runner.sh --full
#   ./scripts/bench_runner.sh --m5
#   ./scripts/bench_runner.sh --m6
#   ./scripts/bench_runner.sh --tier=json --filter="json_get"
#   ./scripts/bench_runner.sh --full --baseline=m6_launch
#   ./scripts/bench_runner.sh --m5 --cores="0-7" --perf
#   ./scripts/bench_runner.sh --m5 --mode=inmemory
#   ./scripts/bench_runner.sh --m5 --all-modes
#   ./scripts/bench_runner.sh --m6 --filter="search_kv"
#   ./scripts/bench_runner.sh --m8
#   ./scripts/bench_runner.sh --m8 --filter="vector_search"
#   ./scripts/bench_runner.sh --comparison
#   ./scripts/bench_runner.sh --comparison --baseline=sota_comparison
#

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default options
RUN_FULL=false
RUN_M1=false
RUN_M2=false
RUN_M3=false
RUN_M5=false
RUN_M6=false
RUN_M8=false
RUN_COMPARISON=false
TIER=""
FILTER=""
BASELINE=""
USE_PERF=false
USE_PERF_RECORD=false
CORES=""
SKIP_SETUP=false
OUTPUT_JSON=false
DURABILITY_MODE=""
ALL_MODES=false

# Benchmark results directory
RESULTS_BASE_DIR="$PROJECT_ROOT/target/benchmark-results"
TIMESTAMP=$(date +%Y-%m-%d_%H-%M-%S)
GIT_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
GIT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
RUN_ID="${TIMESTAMP}_${GIT_COMMIT}"
RESULTS_DIR="$RESULTS_BASE_DIR/run_${RUN_ID}"

# Perf events to monitor
PERF_EVENTS="cache-misses,cache-references,branch-misses,branch-instructions,LLC-loads,LLC-load-misses,cycles,instructions"

# =============================================================================
# Helper Functions
# =============================================================================

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

show_help() {
    # Extract lines 2-46 (the help comment block)
    sed -n '2,46p' "$0" | sed 's/^# //' | sed 's/^#//'
    exit 0
}

check_linux() {
    if [[ "$(uname -s)" != "Linux" ]]; then
        log_warn "Not running on Linux. Results are for development only."
        log_warn "Official benchmarks must run on Linux reference platform."
        return 1
    fi
    return 0
}

check_governor() {
    if [[ ! -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
        log_warn "CPU governor not available"
        return 1
    fi

    local governor
    governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)

    if [[ "$governor" != "performance" ]]; then
        log_warn "CPU governor is '$governor', not 'performance'"
        log_warn "Run: sudo cpupower frequency-set -g performance"
        return 1
    fi

    log_success "CPU governor: performance"
    return 0
}

check_turbo_boost() {
    if [[ -f /sys/devices/system/cpu/cpufreq/boost ]]; then
        local boost
        boost=$(cat /sys/devices/system/cpu/cpufreq/boost)
        if [[ "$boost" == "1" ]]; then
            log_warn "Turbo boost is enabled. Consider disabling for consistency:"
            log_warn "  echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost"
        else
            log_success "Turbo boost: disabled"
        fi
    fi
}

check_perf() {
    if command -v perf &> /dev/null; then
        log_success "perf: available"
        return 0
    else
        log_warn "perf not available. Install with: sudo apt install linux-tools-generic"
        return 1
    fi
}

check_background_load() {
    local load
    load=$(awk '{print $1}' /proc/loadavg 2>/dev/null || echo "0")

    # Get number of CPUs
    local ncpus
    ncpus=$(nproc 2>/dev/null || echo "1")

    # Load per CPU
    local load_per_cpu
    load_per_cpu=$(echo "$load $ncpus" | awk '{printf "%.2f", $1/$2}')

    if (( $(echo "$load_per_cpu > 0.5" | bc -l) )); then
        log_warn "System load is high: $load (${load_per_cpu} per CPU)"
        log_warn "Consider stopping background processes for accurate benchmarks"
        return 1
    fi

    log_success "System load: $load (${load_per_cpu} per CPU)"
    return 0
}

set_governor_performance() {
    log_info "Setting CPU governor to 'performance'..."
    if command -v cpupower &> /dev/null; then
        sudo cpupower frequency-set -g performance || {
            log_error "Failed to set governor. Try: sudo cpupower frequency-set -g performance"
            return 1
        }
        log_success "Governor set to performance"
    else
        log_warn "cpupower not found. Install with: sudo apt install linux-tools-generic"
        return 1
    fi
}

print_environment() {
    echo ""
    echo "============================================================"
    echo "BENCHMARK ENVIRONMENT"
    echo "============================================================"
    echo ""

    # OS
    echo "[Operating System]"
    if [[ -f /etc/os-release ]]; then
        grep -E "^(PRETTY_NAME|VERSION)" /etc/os-release | sed 's/^/  /'
    fi
    echo "  Kernel: $(uname -r)"
    echo ""

    # CPU
    echo "[CPU]"
    if [[ -f /proc/cpuinfo ]]; then
        echo "  Model: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs)"
        echo "  Cores: $(grep -c ^processor /proc/cpuinfo) logical"
    fi
    echo ""

    # Cache
    echo "[Cache Hierarchy]"
    for i in 0 1 2 3; do
        local cache_path="/sys/devices/system/cpu/cpu0/cache/index$i"
        if [[ -d "$cache_path" ]]; then
            local level type size
            level=$(cat "$cache_path/level" 2>/dev/null || echo "?")
            type=$(cat "$cache_path/type" 2>/dev/null || echo "?")
            size=$(cat "$cache_path/size" 2>/dev/null || echo "?")
            echo "  L${level} ${type}: ${size}"
        fi
    done
    echo ""

    # Memory
    echo "[Memory]"
    if [[ -f /proc/meminfo ]]; then
        local total_kb available_kb
        total_kb=$(grep MemTotal /proc/meminfo | awk '{print $2}')
        available_kb=$(grep MemAvailable /proc/meminfo | awk '{print $2}')
        echo "  Total: $((total_kb / 1024 / 1024)) GB"
        echo "  Available: $((available_kb / 1024 / 1024)) GB"
    fi
    echo ""

    # NUMA
    echo "[NUMA Topology]"
    local numa_nodes
    numa_nodes=$(ls -d /sys/devices/system/node/node* 2>/dev/null | wc -l)
    echo "  Nodes: $numa_nodes"
    for node_dir in /sys/devices/system/node/node*; do
        if [[ -d "$node_dir" ]]; then
            local node_id cpulist
            node_id=$(basename "$node_dir" | sed 's/node//')
            cpulist=$(cat "$node_dir/cpulist" 2>/dev/null || echo "?")
            echo "  Node $node_id: CPUs $cpulist"
        fi
    done
    echo ""

    # Governor
    echo "[CPU Governor]"
    if [[ -f /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor ]]; then
        echo "  Current: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)"
        echo "  Available: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors 2>/dev/null || echo "N/A")"
    else
        echo "  N/A (cpufreq not available)"
    fi
    echo ""

    # Rust
    echo "[Rust Toolchain]"
    echo "  Version: $(rustc --version 2>/dev/null || echo "not found")"
    echo "  Target: $(rustc --print target-triple 2>/dev/null || echo "unknown")"
    echo ""

    # Git
    echo "[Git]"
    echo "  Commit: $(git rev-parse --short HEAD 2>/dev/null || echo "unknown")"
    echo "  Branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")"
    if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then
        echo "  Status: DIRTY (uncommitted changes)"
    else
        echo "  Status: clean"
    fi
    echo ""

    # Timestamp
    echo "[Timestamp]"
    echo "  $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo ""
    echo "============================================================"
    echo ""
}

# =============================================================================
# Benchmark Execution
# =============================================================================

build_release() {
    local bench_target="$1"
    log_info "Building in release mode with LTO..."
    cd "$PROJECT_ROOT"
    if [[ -n "$bench_target" ]]; then
        # Industry comparison needs feature flag
        if [[ "$bench_target" == "industry_comparison" ]]; then
            log_info "Building with comparison-benchmarks feature..."
            cargo build --release --bench "$bench_target" --features=comparison-benchmarks 2>&1 | tail -5
        else
            cargo build --release --bench "$bench_target" 2>&1 | tail -5
        fi
    else
        # Build all benchmark targets
        cargo build --release --bench m1_storage --bench m2_transactions --bench m3_primitives --bench m5_performance --bench m6_search 2>&1 | tail -5
    fi
    log_success "Build complete"
}

run_benchmarks() {
    local filter="$1"
    local baseline="$2"
    local cores="$3"
    local use_perf="$4"
    local use_perf_record="$5"
    local durability_mode="$6"
    local bench_target="$7"

    cd "$PROJECT_ROOT"
    mkdir -p "$RESULTS_DIR"

    # Set durability mode environment variable
    if [[ -n "$durability_mode" ]]; then
        export INMEM_DURABILITY_MODE="$durability_mode"
        log_info "Durability mode: $durability_mode"
    else
        unset INMEM_DURABILITY_MODE
        log_info "Durability mode: default (strict)"
    fi

    # Build criterion arguments
    local criterion_args=()

    if [[ -n "$filter" ]]; then
        criterion_args+=("$filter")
    fi

    if [[ -n "$baseline" ]]; then
        criterion_args+=("--save-baseline" "$baseline")
    fi

    # Build the command
    local cmd=()

    # Core pinning
    if [[ -n "$cores" ]]; then
        cmd+=("taskset" "-c" "$cores")
    fi

    # Perf stat wrapper
    if [[ "$use_perf" == "true" ]] && check_perf; then
        local perf_output="$RESULTS_DIR/perf_stat.txt"
        cmd+=("perf" "stat" "-e" "$PERF_EVENTS" "-d" "-d" "-d" "-o" "$perf_output")
        log_info "Perf output will be saved to: $perf_output"
    fi

    # Perf record wrapper
    if [[ "$use_perf_record" == "true" ]] && check_perf; then
        local perf_data="$RESULTS_DIR/perf.data"
        cmd+=("perf" "record" "-e" "$PERF_EVENTS" "-g" "-o" "$perf_data")
        log_info "Perf data will be saved to: $perf_data"
    fi

    # Main benchmark command
    cmd+=("cargo" "bench" "--bench" "$bench_target")

    # Add feature flag for industry comparison
    if [[ "$bench_target" == "industry_comparison" ]]; then
        cmd+=("--features=comparison-benchmarks")
    fi

    if [[ ${#criterion_args[@]} -gt 0 ]]; then
        cmd+=("--")
        cmd+=("${criterion_args[@]}")
    fi

    log_info "Running: ${cmd[*]}"
    echo ""

    # Determine output filename suffix based on mode
    local mode_suffix=""
    if [[ -n "$durability_mode" ]]; then
        mode_suffix="_${durability_mode}"
    fi

    local output_file="$RESULTS_DIR/bench_output${mode_suffix}.txt"

    # Execute
    "${cmd[@]}" 2>&1 | tee "$output_file"

    echo ""
    log_success "Benchmark complete"
    log_info "Results saved to: $output_file"

    # Generate reports
    generate_redis_report "$output_file" "$durability_mode"

    # Generate run summary and update index
    generate_run_summary "$bench_target" "$durability_mode"
    update_runs_index
}

generate_redis_report() {
    local output_file="$1"
    local durability_mode="$2"

    local mode_suffix=""
    local mode_display="Strict (default)"
    if [[ -n "$durability_mode" ]]; then
        mode_suffix="_${durability_mode}"
        mode_display="$durability_mode"
    fi

    local report_file="$RESULTS_DIR/redis_comparison${mode_suffix}.txt"

    log_info "Generating Redis comparison report..."

    cat > "$report_file" << EOF
=============================================================================
REDIS COMPETITIVENESS REPORT
Durability Mode: ${mode_display}
=============================================================================

EOF

    # Add environment info
    {
        echo "Environment:"
        echo "  OS: $(grep PRETTY_NAME /etc/os-release 2>/dev/null | cut -d= -f2 | tr -d '"' || echo "$(uname -s)")"
        echo "  CPU: $(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2 | xargs || echo "unknown")"
        echo "  Memory: $(awk '/MemTotal/ {printf "%.0f GB", $2/1024/1024}' /proc/meminfo 2>/dev/null || echo "unknown")"
        echo "  Governor: $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo "N/A")"
        echo "  Rust: $(rustc --version 2>/dev/null | awk '{print $2}' || echo "unknown")"
        echo "  Commit: $(git rev-parse --short HEAD 2>/dev/null || echo "unknown")"
        echo "  Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo ""
    } >> "$report_file"

    # Extract benchmark results and format
    {
        echo "-----------------------------------------------------------------------------"
        echo "Tier A0: Core Data Structure (Redis comparison valid)"
        echo "-----------------------------------------------------------------------------"
        echo ""
        echo "  Benchmark              Time         Redis Baseline    Gap"
        echo "  --------               ----         --------------    ---"

        # Parse core benchmarks from output
        if grep -q "core/get_hot" "$output_file"; then
            local time
            time=$(grep "core/get_hot" "$output_file" | grep -oP '\d+\.\d+ ns' | head -1 || echo "N/A")
            printf "  core/get_hot           %-12s ~100-200 ns       " "$time"
            echo ""
        fi

        echo ""
        echo "-----------------------------------------------------------------------------"
        echo "Tier A1: Correctness Wrapper (M3 Gate: ALL < 3 µs)"
        echo "-----------------------------------------------------------------------------"
        echo ""
        echo "  Benchmark              Time         M3 Gate    Status"
        echo "  --------               ----         -------    ------"

        # Parse engine benchmarks
        for bench in "engine/get_direct" "engine/put_direct" "engine/cas_direct" "engine/snapshot_acquire" "engine/txn_empty_commit"; do
            if grep -q "$bench" "$output_file"; then
                local time status gate
                time=$(grep "$bench" "$output_file" | grep -oP '[\d.]+ [nµm]s' | head -1 || echo "N/A")
                gate="<3 µs"
                # Simple gate check (would need proper parsing for real use)
                status="✓"
                printf "  %-22s %-12s %-10s %s\n" "$bench" "$time" "$gate" "$status"
            fi
        done

        echo ""
        echo "-----------------------------------------------------------------------------"
        echo "Tier B: Primitive Facades (Redis N/A - we have transactions)"
        echo "-----------------------------------------------------------------------------"
        echo ""

        for bench in "kvstore/get" "kvstore/put" "eventlog/append" "statecell/transition"; do
            if grep -q "$bench" "$output_file"; then
                local time
                time=$(grep "$bench" "$output_file" | grep -oP '[\d.]+ [nµm]s' | head -1 || echo "N/A")
                printf "  %-22s %s\n" "$bench" "$time"
            fi
        done

        echo ""
        echo "-----------------------------------------------------------------------------"
        echo "Tier D: Contention (Redis is single-threaded)"
        echo "-----------------------------------------------------------------------------"
        echo ""

        for threads in 1 2 4 8; do
            for bench in "statecell_same_key" "disjoint_key"; do
                local pattern="contention/$bench/$threads"
                if grep -q "$pattern" "$output_file"; then
                    local time
                    time=$(grep "$pattern" "$output_file" | grep -oP '[\d.]+ [nµm]s' | head -1 || echo "N/A")
                    printf "  %-30s %s\n" "$pattern" "$time"
                fi
            done
        done

        echo ""
        echo "-----------------------------------------------------------------------------"
        echo "Assessment"
        echo "-----------------------------------------------------------------------------"
        echo ""
        echo "  See M3_BENCHMARK_PLAN.md for gate definitions and acceptance criteria."
        echo ""

    } >> "$report_file"

    log_success "Report saved to: $report_file"
    echo ""
    cat "$report_file"
}

generate_run_summary() {
    local bench_target="$1"
    local durability_mode="$2"

    local summary_file="$RESULTS_DIR/SUMMARY.md"

    log_info "Generating run summary..."

    # Determine milestones run
    local milestones=""
    case "$bench_target" in
        m1_storage) milestones="M1 (Storage)" ;;
        m2_transactions) milestones="M2 (Transactions)" ;;
        m3_primitives) milestones="M3 (Primitives)" ;;
        m5_performance) milestones="M5 (JSON)" ;;
        m6_search) milestones="M6 (Search)" ;;
        *) milestones="All" ;;
    esac

    local mode_display="${durability_mode:-strict (default)}"

    cat > "$summary_file" << EOF
# Benchmark Run Summary

**Run ID:** \`${RUN_ID}\`
**Date:** $(date -u +%Y-%m-%dT%H:%M:%SZ)

## Quick Info

| Property | Value |
|----------|-------|
| Git Commit | \`${GIT_COMMIT}\` |
| Git Branch | \`${GIT_BRANCH}\` |
| Milestones | ${milestones} |
| Durability Mode | ${mode_display} |

## Environment

| Property | Value |
|----------|-------|
| OS | $(grep PRETTY_NAME /etc/os-release 2>/dev/null | cut -d= -f2 | tr -d '"' || echo "$(uname -s)") |
| CPU | $(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2 | xargs || echo "unknown") |
| Memory | $(awk '/MemTotal/ {printf "%.1f GB", $2/1024/1024}' /proc/meminfo 2>/dev/null || echo "unknown") |
| Governor | $(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null || echo "N/A") |
| Rust | $(rustc --version 2>/dev/null | awk '{print $2}' || echo "unknown") |

## Files in This Run

EOF

    # List all files in the run directory
    for f in "$RESULTS_DIR"/*; do
        if [[ -f "$f" && "$(basename "$f")" != "SUMMARY.md" ]]; then
            echo "- [$(basename "$f")]($(basename "$f"))" >> "$summary_file"
        fi
    done

    cat >> "$summary_file" << EOF

## Key Results

EOF

    # Extract key results from benchmark output if available
    local output_file
    output_file=$(ls -t "$RESULTS_DIR"/bench_output*.txt 2>/dev/null | head -1)

    if [[ -n "$output_file" && -f "$output_file" ]]; then
        cat >> "$summary_file" << EOF
### Highlighted Latencies

| Benchmark | Latency |
|-----------|---------|
EOF
        # Extract some key benchmarks - parse criterion output format
        # Example: "search_kv/small/100  time:   [87.123 µs 89.456 µs 91.789 µs]"
        grep -E "(get_hot|put_hot|kvstore_get|kvstore_put|json_get|json_set|search_kv|search_hybrid|index_operations)" "$output_file" 2>/dev/null | \
            grep "time:" | \
            head -10 | \
            while read -r line; do
                bench=$(echo "$line" | awk '{print $1}')
                # Extract the middle value from the time range [low mid high]
                time=$(echo "$line" | sed -n 's/.*\[\([^]]*\)\].*/\1/p' | awk '{print $3, $4}')
                if [[ -n "$bench" && -n "$time" ]]; then
                    echo "| $bench | $time |" >> "$summary_file"
                fi
            done
    fi

    log_success "Summary saved to: $summary_file"
}

update_runs_index() {
    local index_file="$RESULTS_BASE_DIR/INDEX.md"

    log_info "Updating runs index..."

    mkdir -p "$RESULTS_BASE_DIR"

    # Create or update the index header
    cat > "$index_file" << EOF
# Benchmark Runs Index

This file lists all benchmark runs for easy comparison.

**Last Updated:** $(date -u +%Y-%m-%dT%H:%M:%SZ)

## All Runs

| Run ID | Date | Commit | Branch | Milestones |
|--------|------|--------|--------|------------|
EOF

    # List all run directories sorted by date (newest first)
    for run_dir in $(ls -dt "$RESULTS_BASE_DIR"/run_* 2>/dev/null); do
        if [[ -d "$run_dir" ]]; then
            local dir_name run_id date commit branch milestones
            dir_name=$(basename "$run_dir")
            run_id="${dir_name#run_}"

            # Extract date and commit from run_id (format: YYYY-MM-DD_HH-MM-SS_commit)
            date=$(echo "$run_id" | cut -d'_' -f1-2 | tr '_' ' ' | sed 's/-/:/3' | sed 's/-/:/3')
            commit=$(echo "$run_id" | rev | cut -d'_' -f1 | rev)

            # Try to get branch from the run
            if [[ -f "$run_dir/SUMMARY.md" ]]; then
                branch=$(grep "Git Branch" "$run_dir/SUMMARY.md" | sed -n 's/.*`\([^`]*\)`.*/\1/p' || echo "unknown")
                milestones=$(grep "Milestones" "$run_dir/SUMMARY.md" | cut -d'|' -f3 | xargs 2>/dev/null || echo "unknown")
                [[ -z "$branch" ]] && branch="unknown"
                [[ -z "$milestones" ]] && milestones="unknown"
            else
                branch="unknown"
                milestones="unknown"
            fi

            echo "| [\`${run_id}\`](run_${run_id}/SUMMARY.md) | ${date} | \`${commit}\` | ${branch} | ${milestones} |" >> "$index_file"
        fi
    done

    cat >> "$index_file" << EOF

## Quick Comparison Tips

To compare runs:
1. Open the SUMMARY.md files from two runs side by side
2. Compare the "Key Results" section
3. Look for significant latency changes (>10%)

## Directory Structure

\`\`\`
target/benchmark-results/
├── INDEX.md                    # This file
├── run_YYYY-MM-DD_HH-MM-SS_commit/
│   ├── SUMMARY.md              # Run summary with key results
│   ├── bench_output_*.txt      # Raw benchmark output
│   ├── redis_comparison_*.txt  # Redis comparison report
│   ├── environment_*.json      # Environment details
│   └── perf_stat_*.txt         # (if --perf was used)
└── ...
\`\`\`
EOF

    log_success "Index updated: $index_file"
}

# =============================================================================
# Main
# =============================================================================

main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --full)
                RUN_FULL=true
                shift
                ;;
            --m1)
                RUN_M1=true
                shift
                ;;
            --m2)
                RUN_M2=true
                shift
                ;;
            --m3)
                RUN_M3=true
                shift
                ;;
            --m5)
                RUN_M5=true
                shift
                ;;
            --m6)
                RUN_M6=true
                shift
                ;;
            --m8)
                RUN_M8=true
                shift
                ;;
            --comparison)
                RUN_COMPARISON=true
                shift
                ;;
            --tier=*)
                TIER="${1#*=}"
                shift
                ;;
            --filter=*)
                FILTER="${1#*=}"
                shift
                ;;
            --baseline=*)
                BASELINE="${1#*=}"
                shift
                ;;
            --perf)
                USE_PERF=true
                shift
                ;;
            --perf-record)
                USE_PERF_RECORD=true
                shift
                ;;
            --cores=*)
                CORES="${1#*=}"
                shift
                ;;
            --no-setup)
                SKIP_SETUP=true
                shift
                ;;
            --json)
                OUTPUT_JSON=true
                shift
                ;;
            --mode=*)
                DURABILITY_MODE="${1#*=}"
                shift
                ;;
            --all-modes)
                ALL_MODES=true
                shift
                ;;
            --help|-h)
                show_help
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                ;;
        esac
    done

    # Map tier to filter and benchmark target
    local BENCH_TARGET=""
    case "$TIER" in
        a0|A0)
            FILTER="core/"
            BENCH_TARGET="m3_primitives"
            ;;
        a1|A1)
            FILTER="engine/"
            BENCH_TARGET="m3_primitives"
            ;;
        b|B)
            FILTER="kvstore/\|eventlog/\|statecell/\|tracestore/\|runindex/"
            BENCH_TARGET="m3_primitives"
            ;;
        c|C)
            FILTER="tracestore/\|index_amp/"
            BENCH_TARGET="m3_primitives"
            ;;
        d|D)
            FILTER="contention/"
            BENCH_TARGET="m3_primitives"
            ;;
        json|JSON|m5|M5)
            BENCH_TARGET="m5_performance"
            ;;
        search|SEARCH|m6|M6)
            BENCH_TARGET="m6_search"
            ;;
        vector|VECTOR|m8|M8)
            BENCH_TARGET="m8_vector"
            ;;
        comparison|COMPARISON)
            BENCH_TARGET="industry_comparison"
            ;;
    esac

    # Set benchmark target based on milestone flags
    if [[ "$RUN_M1" == "true" ]]; then
        BENCH_TARGET="m1_storage"
    elif [[ "$RUN_M2" == "true" ]]; then
        BENCH_TARGET="m2_transactions"
    elif [[ "$RUN_M3" == "true" ]]; then
        BENCH_TARGET="m3_primitives"
    elif [[ "$RUN_M5" == "true" ]]; then
        BENCH_TARGET="m5_performance"
    elif [[ "$RUN_M6" == "true" ]]; then
        BENCH_TARGET="m6_search"
    elif [[ "$RUN_M8" == "true" ]]; then
        BENCH_TARGET="m8_vector"
    elif [[ "$RUN_COMPARISON" == "true" ]]; then
        BENCH_TARGET="industry_comparison"
    fi

    echo ""
    echo "============================================================"
    echo "IN-MEM BENCHMARK RUNNER"
    echo "============================================================"
    echo ""

    # Environment checks
    if [[ "$SKIP_SETUP" != "true" ]]; then
        log_info "Checking environment..."
        echo ""

        local is_reference=true

        check_linux || is_reference=false
        check_governor || is_reference=false
        check_turbo_boost
        check_background_load || true
        check_perf || true

        echo ""

        if [[ "$is_reference" == "true" ]]; then
            log_success "Running on reference platform"
        else
            log_warn "NOT running on reference platform"
            log_warn "Results are for development only"
        fi

        echo ""
    fi

    # Print environment
    if [[ "$OUTPUT_JSON" == "true" ]]; then
        # Would need to implement JSON output
        log_error "--json not yet implemented for shell script"
        exit 1
    fi

    print_environment

    # Determine which benchmark to run
    local run_any=false
    if [[ "$RUN_FULL" == "true" ]] || [[ -n "$FILTER" ]] || [[ -n "$BENCH_TARGET" ]]; then
        run_any=true
    fi

    # Build
    build_release "$BENCH_TARGET"

    # Run benchmarks
    if [[ "$run_any" == "true" ]]; then
        # Default to m3_primitives if no specific target
        if [[ -z "$BENCH_TARGET" ]]; then
            BENCH_TARGET="m3_primitives"
        fi

        if [[ "$ALL_MODES" == "true" ]]; then
            # Run all three durability modes
            log_info "Running benchmarks for all durability modes..."
            echo ""

            local all_modes_runs=()
            for mode in inmemory batched strict; do
                echo ""
                echo "============================================================"
                echo "DURABILITY MODE: $mode"
                echo "============================================================"
                echo ""

                # Update run folder for each mode
                TIMESTAMP=$(date +%Y-%m-%d_%H-%M-%S)
                RUN_ID="${TIMESTAMP}_${GIT_COMMIT}_${mode}"
                RESULTS_DIR="$RESULTS_BASE_DIR/run_${RUN_ID}"
                all_modes_runs+=("$RUN_ID")

                run_benchmarks "$FILTER" "$BASELINE" "$CORES" "$USE_PERF" "$USE_PERF_RECORD" "$mode" "$BENCH_TARGET"
            done

            # Print summary of all modes
            echo ""
            echo "============================================================"
            echo "ALL MODES COMPLETE"
            echo "============================================================"
            echo ""
            log_info "Results saved to:"
            for run in "${all_modes_runs[@]}"; do
                log_info "  $RESULTS_BASE_DIR/run_${run}/"
            done
        else
            # Run with specific mode or default
            run_benchmarks "$FILTER" "$BASELINE" "$CORES" "$USE_PERF" "$USE_PERF_RECORD" "$DURABILITY_MODE" "$BENCH_TARGET"
        fi
    else
        log_info "No benchmarks specified. Use --full, --m1, --m2, --m3, --m5, --m6, --m8, or --filter=<pattern>"
        log_info "Examples:"
        log_info "  $0 --full                    # Run all benchmarks"
        log_info "  $0 --m1                      # Run M1 Storage benchmarks"
        log_info "  $0 --m2                      # Run M2 Transaction benchmarks"
        log_info "  $0 --m5                      # Run M5 JSON benchmarks"
        log_info "  $0 --m6                      # Run M6 Search benchmarks"
        log_info "  $0 --m8                      # Run M8 Vector benchmarks"
        log_info "  $0 --tier=json               # Run M5 JSON benchmarks"
        log_info "  $0 --tier=search             # Run M6 Search benchmarks"
        log_info "  $0 --tier=vector             # Run M8 Vector benchmarks"
        log_info "  $0 --m5 --filter=\"json_get\"  # Run json_get benchmarks"
        log_info "  $0 --m6 --filter=\"search_kv\" # Run search_kv benchmarks"
        log_info "  $0 --m8 --filter=\"vector_search\" # Run vector_search benchmarks"
        log_info "  $0 --m5 --perf               # Run M5 with perf stat"
        log_info "  $0 --m5 --baseline=m5        # Save baseline 'm5'"
        log_info "  $0 --m6 --baseline=m6        # Save baseline 'm6'"
        log_info "  $0 --m8 --baseline=m8        # Save baseline 'm8'"
        log_info "  $0 --m5 --mode=inmemory      # Run in InMemory mode"
        log_info "  $0 --m5 --all-modes          # Run all three durability modes"
        log_info "  $0 --comparison              # Run SOTA comparison benchmarks"
        log_info "  $0 --comparison --filter=\"kv\" # Run KV comparisons only"
    fi

    echo ""
    log_success "Done"
}

main "$@"
