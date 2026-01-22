#!/bin/bash
#
# Baseline Comparison Script
# ==========================
#
# Compares benchmark results between two runs or baselines.
# Highlights regressions and improvements with color-coded output.
#
# Usage:
#   ./scripts/compare_baselines.sh <baseline1> <baseline2>
#   ./scripts/compare_baselines.sh --latest <baseline>
#   ./scripts/compare_baselines.sh --run <run_id1> <run_id2>
#
# Options:
#   --latest <baseline>     Compare latest run against saved baseline
#   --run <id1> <id2>       Compare two specific run IDs
#   --threshold <percent>   Regression threshold (default: 10)
#   --json                  Output as JSON
#   --help                  Show this help message
#
# Examples:
#   ./scripts/compare_baselines.sh m8_initial m8_optimized
#   ./scripts/compare_baselines.sh --latest m8_baseline
#   ./scripts/compare_baselines.sh --run 2026-01-15_10-30-00_abc1234 2026-01-16_14-20-00_def5678
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_DIR="$PROJECT_ROOT/target/benchmark-results"
CRITERION_DIR="$PROJECT_ROOT/target/criterion"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# Default options
THRESHOLD=10
OUTPUT_JSON=false
COMPARE_LATEST=false
COMPARE_RUNS=false

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
    sed -n '2,25p' "$0" | sed 's/^# //' | sed 's/^#//'
    exit 0
}

# Find the latest run directory
find_latest_run() {
    if [[ -d "$RESULTS_DIR" ]]; then
        ls -1d "$RESULTS_DIR"/run_* 2>/dev/null | sort -r | head -1
    else
        echo ""
    fi
}

# Extract benchmark data from a run directory
extract_benchmark_data() {
    local run_dir="$1"
    local output_file

    # Find the benchmark output file
    output_file=$(ls -1 "$run_dir"/bench_output*.txt 2>/dev/null | head -1)

    if [[ -z "$output_file" || ! -f "$output_file" ]]; then
        log_error "No benchmark output found in: $run_dir"
        return 1
    fi

    # Parse criterion output format
    # Example: "benchmark_name    time:   [123.45 ns 125.67 ns 127.89 ns]"
    grep -E "^[a-zA-Z_/]+\s+time:" "$output_file" 2>/dev/null | \
        sed 's/time:\s*\[//' | \
        sed 's/\]//' | \
        awk '{
            name = $1
            # Middle value is the estimate
            gsub(/[^0-9.]/, "", $3)
            unit = $4
            value = $3

            # Normalize to nanoseconds
            if (unit == "µs" || unit == "us") value = value * 1000
            else if (unit == "ms") value = value * 1000000
            else if (unit == "s") value = value * 1000000000

            print name, value
        }'
}

# Compare two sets of benchmark data
compare_benchmarks() {
    local data1="$1"
    local data2="$2"
    local name1="$3"
    local name2="$4"

    echo ""
    echo "============================================================"
    echo "BENCHMARK COMPARISON"
    echo "============================================================"
    echo ""
    echo "Baseline:  $name1"
    echo "Current:   $name2"
    echo "Threshold: ${THRESHOLD}%"
    echo ""
    echo "------------------------------------------------------------"
    printf "%-40s %12s %12s %10s %8s\n" "Benchmark" "$name1" "$name2" "Change" "Status"
    echo "------------------------------------------------------------"

    local regressions=0
    local improvements=0
    local unchanged=0

    # Create temporary files for comparison
    local tmp1=$(mktemp)
    local tmp2=$(mktemp)
    trap "rm -f $tmp1 $tmp2" EXIT

    echo "$data1" | sort > "$tmp1"
    echo "$data2" | sort > "$tmp2"

    # Join on benchmark name
    join -a1 -a2 "$tmp1" "$tmp2" | while read -r name val1 val2; do
        if [[ -z "$val1" ]]; then
            printf "%-40s %12s %12.0f %10s ${CYAN}NEW${NC}\n" "$name" "N/A" "$val2" "N/A"
        elif [[ -z "$val2" ]]; then
            printf "%-40s %12.0f %12s %10s ${YELLOW}REMOVED${NC}\n" "$name" "$val1" "N/A" "N/A"
        else
            # Calculate percentage change
            local change
            change=$(echo "$val1 $val2" | awk '{
                if ($1 == 0) print 0
                else printf "%.1f", (($2 - $1) / $1) * 100
            }')

            local status=""
            local status_color="$NC"

            if (( $(echo "$change > $THRESHOLD" | bc -l) )); then
                status="REGRESSION"
                status_color="$RED"
                ((regressions++)) || true
            elif (( $(echo "$change < -$THRESHOLD" | bc -l) )); then
                status="IMPROVED"
                status_color="$GREEN"
                ((improvements++)) || true
            else
                status="OK"
                status_color="$NC"
                ((unchanged++)) || true
            fi

            # Format values for display
            local disp1 disp2
            disp1=$(format_ns "$val1")
            disp2=$(format_ns "$val2")

            printf "%-40s %12s %12s %+9.1f%% ${status_color}%s${NC}\n" \
                "$name" "$disp1" "$disp2" "$change" "$status"
        fi
    done

    echo "------------------------------------------------------------"
    echo ""
    echo "Summary:"
    echo "  Regressions:  $regressions"
    echo "  Improvements: $improvements"
    echo "  Unchanged:    $unchanged"
    echo ""

    if [[ $regressions -gt 0 ]]; then
        log_warn "Found $regressions regression(s) exceeding ${THRESHOLD}% threshold"
        return 1
    else
        log_success "No significant regressions detected"
        return 0
    fi
}

# Format nanoseconds to human-readable
format_ns() {
    local ns="$1"
    if (( $(echo "$ns >= 1000000000" | bc -l) )); then
        echo "$(echo "$ns" | awk '{printf "%.2f s", $1/1000000000}')"
    elif (( $(echo "$ns >= 1000000" | bc -l) )); then
        echo "$(echo "$ns" | awk '{printf "%.2f ms", $1/1000000}')"
    elif (( $(echo "$ns >= 1000" | bc -l) )); then
        echo "$(echo "$ns" | awk '{printf "%.2f µs", $1/1000}')"
    else
        echo "$(echo "$ns" | awk '{printf "%.0f ns", $1}')"
    fi
}

# Compare Criterion baselines
compare_criterion_baselines() {
    local baseline1="$1"
    local baseline2="$2"

    local base1_dir="$CRITERION_DIR/$baseline1"
    local base2_dir="$CRITERION_DIR/$baseline2"

    if [[ ! -d "$base1_dir" ]]; then
        log_error "Baseline not found: $baseline1"
        log_info "Available baselines:"
        ls -1 "$CRITERION_DIR" 2>/dev/null | grep -v "^$" | sed 's/^/  /'
        exit 1
    fi

    if [[ ! -d "$base2_dir" ]]; then
        log_error "Baseline not found: $baseline2"
        log_info "Available baselines:"
        ls -1 "$CRITERION_DIR" 2>/dev/null | grep -v "^$" | sed 's/^/  /'
        exit 1
    fi

    echo ""
    echo "============================================================"
    echo "CRITERION BASELINE COMPARISON"
    echo "============================================================"
    echo ""
    echo "Baseline 1: $baseline1"
    echo "Baseline 2: $baseline2"
    echo "Threshold:  ${THRESHOLD}%"
    echo ""

    # Use cargo bench with baseline comparison
    log_info "Running criterion comparison..."
    cargo bench -- --baseline "$baseline1" --compare "$baseline2" 2>&1 | \
        grep -E "(Benchmarking|time:|change:|Performance)" || true
}

main() {
    local baseline1=""
    local baseline2=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --latest)
                COMPARE_LATEST=true
                shift
                baseline1="$1"
                shift
                ;;
            --run)
                COMPARE_RUNS=true
                shift
                baseline1="$1"
                shift
                baseline2="$1"
                shift
                ;;
            --threshold)
                shift
                THRESHOLD="$1"
                shift
                ;;
            --json)
                OUTPUT_JSON=true
                shift
                ;;
            --help|-h)
                show_help
                ;;
            *)
                if [[ -z "$baseline1" ]]; then
                    baseline1="$1"
                elif [[ -z "$baseline2" ]]; then
                    baseline2="$1"
                else
                    log_error "Too many arguments"
                    show_help
                fi
                shift
                ;;
        esac
    done

    # Handle --latest mode
    if [[ "$COMPARE_LATEST" == "true" ]]; then
        local latest_run
        latest_run=$(find_latest_run)

        if [[ -z "$latest_run" ]]; then
            log_error "No benchmark runs found in $RESULTS_DIR"
            exit 1
        fi

        log_info "Latest run: $(basename "$latest_run")"
        log_info "Comparing against baseline: $baseline1"

        # Extract data from latest run
        local latest_data
        latest_data=$(extract_benchmark_data "$latest_run")

        # For Criterion baselines, we'd need different approach
        # For now, suggest using cargo bench directly
        log_info "For Criterion baseline comparison, run:"
        log_info "  cargo bench -- --baseline $baseline1"
        exit 0
    fi

    # Handle --run mode
    if [[ "$COMPARE_RUNS" == "true" ]]; then
        local run1_dir="$RESULTS_DIR/run_$baseline1"
        local run2_dir="$RESULTS_DIR/run_$baseline2"

        if [[ ! -d "$run1_dir" ]]; then
            log_error "Run not found: $baseline1"
            log_info "Available runs:"
            ls -1 "$RESULTS_DIR" 2>/dev/null | grep "^run_" | sed 's/^run_/  /'
            exit 1
        fi

        if [[ ! -d "$run2_dir" ]]; then
            log_error "Run not found: $baseline2"
            log_info "Available runs:"
            ls -1 "$RESULTS_DIR" 2>/dev/null | grep "^run_" | sed 's/^run_/  /'
            exit 1
        fi

        local data1 data2
        data1=$(extract_benchmark_data "$run1_dir")
        data2=$(extract_benchmark_data "$run2_dir")

        compare_benchmarks "$data1" "$data2" "$baseline1" "$baseline2"
        exit $?
    fi

    # Standard Criterion baseline comparison
    if [[ -z "$baseline1" ]] || [[ -z "$baseline2" ]]; then
        log_error "Two baselines required for comparison"
        log_info ""
        log_info "Usage: $0 <baseline1> <baseline2>"
        log_info ""
        log_info "Available Criterion baselines:"
        ls -1 "$CRITERION_DIR" 2>/dev/null | grep -v "^$" | sed 's/^/  /'
        log_info ""
        log_info "Available run IDs:"
        ls -1 "$RESULTS_DIR" 2>/dev/null | grep "^run_" | sed 's/^run_/  /' | head -10
        exit 1
    fi

    compare_criterion_baselines "$baseline1" "$baseline2"
}

main "$@"
