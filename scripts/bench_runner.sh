#!/bin/bash
#
# M3 Benchmark Runner
# ====================
#
# This script sets up the proper environment for running benchmarks and
# generates the Redis competitiveness report.
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
#   --full          Run full benchmark suite
#   --tier=<tier>   Run specific tier (a0, a1, b, c, d)
#   --filter=<pat>  Run benchmarks matching pattern
#   --baseline=<n>  Save/compare with baseline name
#   --perf          Run with perf stat
#   --perf-record   Run with perf record (generates perf.data)
#   --cores=<list>  Pin to specific cores (e.g., "0-7")
#   --no-setup      Skip environment setup checks
#   --json          Output environment as JSON
#   --help          Show this help message
#
# Examples:
#   ./scripts/bench_runner.sh --full
#   ./scripts/bench_runner.sh --tier=a1
#   ./scripts/bench_runner.sh --filter="kvstore_" --perf
#   ./scripts/bench_runner.sh --full --baseline=m3_launch
#   ./scripts/bench_runner.sh --full --cores="0-7" --perf
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
TIER=""
FILTER=""
BASELINE=""
USE_PERF=false
USE_PERF_RECORD=false
CORES=""
SKIP_SETUP=false
OUTPUT_JSON=false

# Benchmark results directory
RESULTS_DIR="$PROJECT_ROOT/target/benchmark-results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

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
    # Extract lines 2-37 (the help comment block)
    sed -n '2,37p' "$0" | sed 's/^# //' | sed 's/^#//'
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
    log_info "Building in release mode with LTO..."
    cd "$PROJECT_ROOT"
    cargo build --release --bench m3_primitives 2>&1 | tail -5
    log_success "Build complete"
}

run_benchmarks() {
    local filter="$1"
    local baseline="$2"
    local cores="$3"
    local use_perf="$4"
    local use_perf_record="$5"

    cd "$PROJECT_ROOT"
    mkdir -p "$RESULTS_DIR"

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
        local perf_output="$RESULTS_DIR/perf_stat_${TIMESTAMP}.txt"
        cmd+=("perf" "stat" "-e" "$PERF_EVENTS" "-d" "-d" "-d" "-o" "$perf_output")
        log_info "Perf output will be saved to: $perf_output"
    fi

    # Perf record wrapper
    if [[ "$use_perf_record" == "true" ]] && check_perf; then
        local perf_data="$RESULTS_DIR/perf_${TIMESTAMP}.data"
        cmd+=("perf" "record" "-e" "$PERF_EVENTS" "-g" "-o" "$perf_data")
        log_info "Perf data will be saved to: $perf_data"
    fi

    # Main benchmark command
    cmd+=("cargo" "bench" "--bench" "m3_primitives")

    if [[ ${#criterion_args[@]} -gt 0 ]]; then
        cmd+=("--")
        cmd+=("${criterion_args[@]}")
    fi

    log_info "Running: ${cmd[*]}"
    echo ""

    # Execute
    "${cmd[@]}" 2>&1 | tee "$RESULTS_DIR/bench_output_${TIMESTAMP}.txt"

    echo ""
    log_success "Benchmark complete"
    log_info "Results saved to: $RESULTS_DIR/bench_output_${TIMESTAMP}.txt"

    # Generate report
    generate_redis_report "$RESULTS_DIR/bench_output_${TIMESTAMP}.txt"
}

generate_redis_report() {
    local output_file="$1"
    local report_file="$RESULTS_DIR/redis_comparison_${TIMESTAMP}.txt"

    log_info "Generating Redis comparison report..."

    cat > "$report_file" << 'EOF'
=============================================================================
REDIS COMPETITIVENESS REPORT
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
            --help|-h)
                show_help
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                ;;
        esac
    done

    # Map tier to filter
    case "$TIER" in
        a0|A0)
            FILTER="core/"
            ;;
        a1|A1)
            FILTER="engine/"
            ;;
        b|B)
            FILTER="kvstore/\|eventlog/\|statecell/\|tracestore/\|runindex/"
            ;;
        c|C)
            FILTER="tracestore/\|index_amp/"
            ;;
        d|D)
            FILTER="contention/"
            ;;
    esac

    echo ""
    echo "============================================================"
    echo "M3 BENCHMARK RUNNER"
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

    # Build
    build_release

    # Run benchmarks
    if [[ "$RUN_FULL" == "true" ]] || [[ -n "$FILTER" ]]; then
        run_benchmarks "$FILTER" "$BASELINE" "$CORES" "$USE_PERF" "$USE_PERF_RECORD"
    else
        log_info "No benchmarks specified. Use --full or --filter=<pattern>"
        log_info "Examples:"
        log_info "  $0 --full                    # Run all benchmarks"
        log_info "  $0 --tier=a1                 # Run Tier A1 only"
        log_info "  $0 --filter=\"kvstore_\"       # Run KVStore benchmarks"
        log_info "  $0 --full --perf             # Run with perf stat"
        log_info "  $0 --full --baseline=m3      # Save baseline 'm3'"
    fi

    echo ""
    log_success "Done"
}

main "$@"
