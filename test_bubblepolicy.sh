# Convenience script for testing bubblepolicy tool
# Run with: bash test_bubblepolicy.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Directories
TEST_DIR="/tmp/bubblepolicy_test"
BUILD_DIR="$(cd "$(dirname "$0")" && pwd)"

echo -e "${BLUE}=== bubblepolicy Test Script ===${NC}"
echo ""

# Function to print colored messages
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Build the project
build_project() {
    print_info "Building bubblepolicy..."
    if ! cargo build --release 2>&1 | grep -q "Finished"; then
        print_error "Build failed!"
        exit 1
    fi
    print_success "Build successful"
}

# Test trace command
test_trace() {
    print_info "Testing trace command..."
    mkdir -p "$TEST_DIR"

    # Test with a simple command
    print_info "Tracing: echo 'hello'"
    if ! "$BUILD_DIR/target/release/bubblepolicy" trace --output "$TEST_DIR/trace1.json" -- echo "hello" 2>/dev/null; then
        print_error "Trace failed!"
        return 1
    fi

    # Check if trace file was created
    if [[ ! -f "$TEST_DIR/trace1.json" ]]; then
        print_error "Trace file not created!"
        return 1
    fi

    local file_count=$(grep -c '"path"' "$TEST_DIR/trace1.json" 2>/dev/null || echo "0")
    print_success "Trace successful: $file_count files accessed"
}

# Test review command
test_review() {
    print_info "Testing review command..."

    if [[ ! -f "$TEST_DIR/trace1.json" ]]; then
        print_error "No trace file found. Run test_trace first."
        return 1
    fi

    # Test non-interactive review (generate policy)
    print_info "Generating policy from trace..."
    if ! "$BUILD_DIR/target/release/bubblepolicy" review --generate-policy --output "$TEST_DIR/policy1.json" "$TEST_DIR/trace1.json" 2>/dev/null; then
        print_error "Review failed!"
        return 1
    fi

    if [[ ! -f "$TEST_DIR/policy1.json" ]]; then
        print_error "Policy file not created!"
        return 1
    fi

    local entry_count=$(grep -c '"path"' "$TEST_DIR/policy1.json" 2>/dev/null || echo "0")
    print_success "Review successful: $entry_count policy entries"
}

# Test create command
test_create() {
    print_info "Testing create command..."

    if [[ ! -f "$TEST_DIR/policy1.json" ]]; then
        print_error "No policy file found. Run test_review first."
        return 1
    fi

    # Test creating a wrapper script
    print_info "Creating wrapper script..."
    if ! "$BUILD_DIR/target/release/bubblepolicy" create --policy "$TEST_DIR/policy1.json" /bin/echo --output "$TEST_DIR/echo_wrapper.sh" 2>/dev/null; then
        print_error "Create failed!"
        return 1
    fi

    if [[ ! -f "$TEST_DIR/echo_wrapper.sh" ]]; then
        print_error "Wrapper file not created!"
        return 1
    fi

    # Make executable and test
    chmod +x "$TEST_DIR/echo_wrapper.sh"

    local bind_count=$(grep -c "ro-bind" "$TEST_DIR/echo_wrapper.sh" 2>/dev/null || echo "0")
    print_success "Create successful: $bind_count bind mounts generated"
    print_info "Wrapper script: $TEST_DIR/echo_wrapper.sh"
}

# Test directory grouping
test_directory_grouping() {
    print_info "Testing directory grouping optimization..."

    # Create a trace with multiple files in same directory
    print_info "Creating trace with multiple files..."
    "$BUILD_DIR/target/release/bubblepolicy" trace --output "$TEST_DIR/trace_multi.json" -- ls -la /etc 2>/dev/null

    if [[ ! -f "$TEST_DIR/trace_multi.json" ]]; then
        print_error "Multi-file trace failed!"
        return 1
    fi

    # Generate policy and wrapper
    "$BUILD_DIR/target/release/bubblepolicy" review --generate-policy --output "$TEST_DIR/policy_multi.json" "$TEST_DIR/trace_multi.json" 2>/dev/null
    "$BUILD_DIR/target/release/bubblepolicy" create --policy "$TEST_DIR/policy_multi.json" /bin/ls --output "$TEST_DIR/ls_wrapper.sh" 2>/dev/null

    # Count bind mounts
    local bind_count=$(grep -c "ro-bind" "$TEST_DIR/ls_wrapper.sh" 2>/dev/null || echo "0")
    local entry_count=$(grep -c '"path"' "$TEST_DIR/policy_multi.json" 2>/dev/null || echo "0")

    print_success "Directory grouping test complete"
    print_info "Policy entries: $entry_count"
    print_info "Bind mounts: $bind_count (grouped from $entry_count files)"
}

# Test multi-file merge in review
test_multi_file_merge() {
    print_info "Testing multi-file merge in review..."

    # Create multiple trace files
    print_info "Creating multiple trace files..."
    "$BUILD_DIR/target/release/bubblepolicy" trace --output "$TEST_DIR/trace_a.json" -- echo "test a" 2>/dev/null
    "$BUILD_DIR/target/release/bubblepolicy" trace --output "$TEST_DIR/trace_b.json" -- echo "test b" 2>/dev/null

    # Merge them with review
    print_info "Merging trace files..."
    "$BUILD_DIR/target/release/bubblepolicy" review --generate-policy --output "$TEST_DIR/merged_policy.json" "$TEST_DIR/trace_a.json" "$TEST_DIR/trace_b.json" 2>/dev/null

    if [[ ! -f "$TEST_DIR/merged_policy.json" ]]; then
        print_error "Multi-file merge failed!"
        return 1
    fi

    local entry_count=$(grep -c '"path"' "$TEST_DIR/merged_policy.json" 2>/dev/null || echo "0")
    print_success "Multi-file merge successful: $entry_count merged entries"
}

# Clean up
cleanup() {
    print_info "Cleaning up test files..."
    rm -rf "$TEST_DIR"
    print_success "Cleanup complete"
}

# Show usage
show_usage() {
    echo "Usage: $0 [test|clean|build|all]"
    echo ""
    echo "Commands:"
    echo "  build    - Build the project"
    echo "  test     - Run all tests"
    echo "  clean    - Clean test files"
    echo "  all      - Build and run all tests"
    echo ""
    echo "Individual tests:"
    echo "  trace    - Test trace command"
    echo "  review   - Test review command"
    echo "  create   - Test create command"
    echo "  grouping - Test directory grouping"
    echo "  merge    - Test multi-file merge"
}

# Main execution
main() {
    local command="${1:-all}"

    case $command in
        build)
            build_project
            ;;
        trace)
            build_project
            test_trace
            ;;
        review)
            build_project
            test_review
            ;;
        create)
            build_project
            test_create
            ;;
        grouping)
            build_project
            test_directory_grouping
            ;;
        merge)
            build_project
            test_multi_file_merge
            ;;
        test)
            build_project
            test_trace
            test_review
            test_create
            test_directory_grouping
            test_multi_file_merge
            ;;
        clean)
            cleanup
            ;;
        all)
            build_project
            test_trace
            test_review
            test_create
            test_directory_grouping
            test_multi_file_merge
            ;;
        -h|--help|help)
            show_usage
            ;;
        *)
            print_error "Unknown command: $command"
            show_usage
            exit 1
            ;;
    esac
}

# Run main function with all arguments
main "$@"
