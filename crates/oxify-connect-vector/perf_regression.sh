#!/usr/bin/env bash
# Performance regression testing script for oxify-connect-vector
# This script compares benchmark results between two git refs (e.g., main vs current branch)

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
BASELINE_REF="${1:-main}"
CURRENT_REF="${2:-HEAD}"
THRESHOLD="${3:-10}" # Performance degradation threshold in percentage

echo -e "${GREEN}=== oxify-connect-vector Performance Regression Testing ===${NC}"
echo "Baseline: $BASELINE_REF"
echo "Current:  $CURRENT_REF"
echo "Threshold: ${THRESHOLD}%"
echo ""

# Check if critcmp is installed
if ! command -v critcmp &> /dev/null; then
    echo -e "${YELLOW}critcmp not found. Installing...${NC}"
    cargo install critcmp
fi

# Save current state
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
echo -e "${GREEN}Current branch: $CURRENT_BRANCH${NC}"

# Create temporary directory for benchmark results
BENCH_DIR=$(mktemp -d)
echo "Using temporary directory: $BENCH_DIR"

# Function to run all benchmarks
run_benchmarks() {
    local baseline_name=$1
    echo -e "${GREEN}Running benchmarks for baseline: $baseline_name${NC}"

    cargo bench --bench vector_bench -- --save-baseline "$baseline_name" --noplot
    cargo bench --bench hybrid_bench -- --save-baseline "$baseline_name" --noplot
    cargo bench --bench cache_bench -- --save-baseline "$baseline_name" --noplot
    cargo bench --bench colbert_bench -- --save-baseline "$baseline_name" --noplot
}

# Run benchmarks on baseline
echo -e "${GREEN}=== Step 1: Checkout baseline ($BASELINE_REF) ===${NC}"
git checkout "$BASELINE_REF" --quiet
run_benchmarks "baseline"

# Run benchmarks on current
echo -e "${GREEN}=== Step 2: Checkout current ($CURRENT_REF) ===${NC}"
git checkout "$CURRENT_REF" --quiet
run_benchmarks "current"

# Restore original branch
echo -e "${GREEN}=== Step 3: Restore original branch ===${NC}"
git checkout "$CURRENT_BRANCH" --quiet

# Compare results
echo -e "${GREEN}=== Step 4: Compare results ===${NC}"
echo ""

# Run comparison
COMPARISON_OUTPUT=$(critcmp baseline current)

# Print comparison
echo "$COMPARISON_OUTPUT"
echo ""

# Check for regressions
echo -e "${GREEN}=== Step 5: Check for regressions ===${NC}"

# Extract performance changes and check against threshold
REGRESSIONS=$(echo "$COMPARISON_OUTPUT" | grep -oE '\+[0-9]+\.[0-9]+%' | sed 's/+//;s/%//' || true)

FOUND_REGRESSION=false
for change in $REGRESSIONS; do
    if (( $(echo "$change > $THRESHOLD" | bc -l) )); then
        echo -e "${RED}⚠️  Performance regression detected: +${change}%${NC}"
        FOUND_REGRESSION=true
    fi
done

if [ "$FOUND_REGRESSION" = false ]; then
    echo -e "${GREEN}✅ No significant performance regression detected${NC}"
    exit 0
else
    echo -e "${RED}❌ Performance regression detected (threshold: ${THRESHOLD}%)${NC}"
    echo ""
    echo "Review the comparison output above for details."
    echo "If this is expected, you can:"
    echo "  1. Update the threshold: $0 $BASELINE_REF $CURRENT_REF <new_threshold>"
    echo "  2. Document the regression in your PR/commit message"
    exit 1
fi
