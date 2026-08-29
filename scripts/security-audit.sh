#!/bin/bash

# Security Audit Automation Script for StellAIverse Contracts
# This script implements static analysis and security checks

set -e

echo "🔍 StellAIverse Security Audit Automation"
echo "======================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[FAIL]${NC} $1"
}

# Check if required tools are installed
check_dependencies() {
    print_status "Checking dependencies..."
    
    if ! command -v cargo &> /dev/null; then
        print_error "cargo is not installed"
        exit 1
    fi
    
    if ! command -v rustc &> /dev/null; then
        print_error "rustc is not installed"
        exit 1
    fi
    
    print_success "All dependencies found"
}

# Run cargo fmt to check code formatting
check_formatting() {
    print_status "Checking code formatting..."
    
    if cargo fmt --all -- --check; then
        print_success "Code formatting is correct"
    else
        print_error "Code formatting issues found"
        print_status "Run 'cargo fmt' to fix formatting issues"
        return 1
    fi
}

# Run cargo clippy for static analysis
run_clippy() {
    print_status "Running Clippy static analysis..."
    
    # Run clippy with strict checks
    if cargo clippy --all-targets --all-features -- -D warnings -W clippy::all -W clippy::pedantic; then
        print_success "Clippy checks passed"
    else
        print_error "Clippy found issues"
        return 1
    fi
}

# Run cargo audit for security vulnerabilities
run_audit() {
    print_status "Running cargo audit for security vulnerabilities..."
    
    if cargo audit; then
        print_success "No security vulnerabilities found"
    else
        print_warning "Security audit found vulnerabilities (review above)"
        return 1
    fi
}

# Run cargo check for compilation errors
run_check() {
    print_status "Running cargo check for compilation errors..."
    
    if cargo check --all --all-features; then
        print_success "All contracts compile successfully"
    else
        print_error "Compilation errors found"
        return 1
    fi
}

# Run cargo test
run_tests() {
    print_status "Running cargo test..."
    
    if cargo test --all --all-features; then
        print_success "All tests passed"
    else
        print_error "Some tests failed"
        return 1
    fi
}

# Check for common security issues in the codebase
check_security_patterns() {
    print_status "Checking for security patterns..."
    
    local issues_found=0
    
    # Check for hardcoded addresses or private keys
    if grep -r "G[A-Z0-9]" contracts/ --include="*.rs" | grep -i "secret\|private\|key" > /dev/null 2>&1; then
        print_warning "Potential hardcoded secrets found"
        issues_found=$((issues_found + 1))
    fi
    
    # Check for panic! statements that should be handled gracefully
    if grep -r "panic!" contracts/ --include="*.rs" | wc -l | grep -v "^0$" > /dev/null 2>&1; then
        print_warning "Uncontrolled panic! statements found"
        issues_found=$((issues_found + 1))
    fi
    
    # Check for unchecked arithmetic
    if grep -r "\.checked_" contracts/ --include="*.rs" | wc -l | grep "^0$" > /dev/null 2>&1; then
        print_warning "Unchecked arithmetic operations found"
        issues_found=$((issues_found + 1))
    fi
    
    # Check for require_auth usage
    if grep -r "require_auth" contracts/ --include="*.rs" | wc -l | grep "^0$" > /dev/null 2>&1; then
        print_warning "Missing authentication checks in some functions"
        issues_found=$((issues_found + 1))
    fi
    
    if [ $issues_found -eq 0 ]; then
        print_success "Security pattern checks passed"
    else
        print_warning "$issues_found potential security issues found"
        return 1
    fi
}

# Generate security report
generate_report() {
    print_status "Generating security audit report..."
    
    local report_file="security-audit-report-$(date +%Y%m%d-%H%M%S).md"
    
    cat > "$report_file" << EOF
# Security Audit Report

**Date**: $(date)
**Repository**: StellAIverse Contracts
**Commit**: $(git rev-parse HEAD 2>/dev/null || echo "N/A")

## Summary

This report contains the results of automated security analysis and static checks.

## Checks Performed

### 1. Code Formatting
- Tool: cargo fmt
- Status: ✅ PASSED

### 2. Static Analysis
- Tool: cargo clippy
- Status: ✅ PASSED

### 3. Security Vulnerability Scan
- Tool: cargo audit
- Status: ✅ PASSED

### 4. Compilation Check
- Tool: cargo check
- Status: ✅ PASSED

### 5. Unit Tests
- Tool: cargo test
- Status: ✅ PASSED

### 6. Security Pattern Analysis
- Tool: Custom grep-based checks
- Status: ✅ PASSED

## Recommendations

1. **Regular Audits**: Run this script weekly and before every deployment
2. **Manual Review**: Complement automated checks with manual security reviews
3. **Third-party Audit**: Consider engaging external security auditors
4. **Fuzz Testing**: Implement property-based fuzz testing for critical functions

## Next Steps

- Review any warnings or issues found
- Update code to address security concerns
- Re-run audit to verify fixes

---

*This report was generated automatically by the security audit automation script.*
EOF

    print_success "Security report generated: $report_file"
}

# Main execution
main() {
    local exit_code=0
    
    check_dependencies
    
    print_status "Starting security audit..."
    echo ""
    
    # Run all checks
    check_formatting || exit_code=1
    run_check || exit_code=1
    run_clippy || exit_code=1
    run_audit || exit_code=1
    run_tests || exit_code=1
    check_security_patterns || exit_code=1
    
    echo ""
    
    if [ $exit_code -eq 0 ]; then
        print_success "🎉 All security checks passed!"
        generate_report
    else
        print_error "❌ Some security checks failed. Please review the output above."
        exit 1
    fi
}

# Run main function
main "$@"
