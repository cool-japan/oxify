#!/bin/bash

# Security Test Suite Runner for Oxify Server
# Runs all OWASP Top 10 security tests

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
TARGET_URL=${TARGET_URL:-"http://localhost:8080"}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo -e "${GREEN}=====================================${NC}"
echo -e "${GREEN}Oxify Server Security Test Suite${NC}"
echo -e "${GREEN}=====================================${NC}"
echo ""
echo "Target URL: $TARGET_URL"
echo "Test Directory: $SCRIPT_DIR"
echo ""

# Check if k6 is installed
if ! command -v k6 &> /dev/null; then
    echo -e "${RED}❌ Error: k6 is not installed${NC}"
    echo "Please install k6: https://k6.io/docs/getting-started/installation"
    exit 1
fi

# Counter for results
PASSED=0
FAILED=0
TOTAL=0

# Function to run a test
run_test() {
    local test_name="$1"
    local test_file="$2"

    echo -e "${YELLOW}Running: $test_name${NC}"
    echo "----------------------------------------"

    TOTAL=$((TOTAL + 1))

    if k6 run -e TARGET_URL="$TARGET_URL" "$SCRIPT_DIR/$test_file"; then
        echo -e "${GREEN}✅ $test_name PASSED${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}❌ $test_name FAILED${NC}"
        FAILED=$((FAILED + 1))
    fi

    echo ""
}

# Run all security tests
run_test "SQL Injection Test" "sql-injection-test.js"
run_test "XSS Test" "xss-test.js"
run_test "Authentication & Authorization Test" "auth-test.js"
run_test "Security Headers Test" "headers-test.js"

# Print summary
echo -e "${GREEN}=====================================${NC}"
echo -e "${GREEN}Test Suite Summary${NC}"
echo -e "${GREEN}=====================================${NC}"
echo "Total Tests: $TOTAL"
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    echo -e "${GREEN}🎉 All security tests passed!${NC}"
    exit 0
else
    echo -e "${RED}⚠️  Some security tests failed!${NC}"
    echo "Please review the failures above and fix vulnerabilities."
    exit 1
fi
