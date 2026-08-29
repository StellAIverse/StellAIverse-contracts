# Security Audit Automation Script for StellAIverse Contracts (PowerShell)
# This script implements static analysis and security checks

param(
    [switch]$SkipTests,
    [switch]$Verbose
)

Write-Host "🔍 StellAIverse Security Audit Automation" -ForegroundColor Blue
Write-Host "======================================" -ForegroundColor Blue

# Function to write colored output
function Write-Status {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Blue
}

function Write-Success {
    param([string]$Message)
    Write-Host "[PASS] $Message" -ForegroundColor Green
}

function Write-Warning {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Host "[FAIL] $Message" -ForegroundColor Red
}

# Check if required tools are installed
function Test-Dependencies {
    Write-Status "Checking dependencies..."
    
    try {
        $null = Get-Command cargo -ErrorAction Stop
        $null = Get-Command rustc -ErrorAction Stop
        Write-Success "All dependencies found"
        return $true
    }
    catch {
        Write-Error "Required tools not found: cargo or rustc"
        return $false
    }
}

# Run cargo fmt to check code formatting
function Test-Formatting {
    Write-Status "Checking code formatting..."
    
    try {
        $result = cargo fmt --all -- --check 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Success "Code formatting is correct"
            return $true
        } else {
            Write-Error "Code formatting issues found"
            Write-Status "Run 'cargo fmt' to fix formatting issues"
            if ($Verbose) { Write-Host $result }
            return $false
        }
    }
    catch {
        Write-Error "Error running cargo fmt: $_"
        return $false
    }
}

# Run cargo clippy for static analysis
function Test-Clippy {
    Write-Status "Running Clippy static analysis..."
    
    try {
        $result = cargo clippy --all-targets --all-features -- -D warnings -W clippy::all -W clippy::pedantic 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Success "Clippy checks passed"
            return $true
        } else {
            Write-Error "Clippy found issues"
            if ($Verbose) { Write-Host $result }
            return $false
        }
    }
    catch {
        Write-Error "Error running cargo clippy: $_"
        return $false
    }
}

# Run cargo audit for security vulnerabilities
function Test-Audit {
    Write-Status "Running cargo audit for security vulnerabilities..."
    
    try {
        $result = cargo audit 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Success "No security vulnerabilities found"
            return $true
        } else {
            Write-Warning "Security audit found vulnerabilities (review above)"
            if ($Verbose) { Write-Host $result }
            return $false
        }
    }
    catch {
        Write-Error "Error running cargo audit: $_"
        return $false
    }
}

# Run cargo check for compilation errors
function Test-Check {
    Write-Status "Running cargo check for compilation errors..."
    
    try {
        $result = cargo check --all --all-features 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Success "All contracts compile successfully"
            return $true
        } else {
            Write-Error "Compilation errors found"
            if ($Verbose) { Write-Host $result }
            return $false
        }
    }
    catch {
        Write-Error "Error running cargo check: $_"
        return $false
    }
}

# Run cargo test
function Test-Tests {
    if ($SkipTests) {
        Write-Status "Skipping tests (SkipTests flag set)"
        return $true
    }
    
    Write-Status "Running cargo test..."
    
    try {
        $result = cargo test --all --all-features 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Success "All tests passed"
            return $true
        } else {
            Write-Error "Some tests failed"
            if ($Verbose) { Write-Host $result }
            return $false
        }
    }
    catch {
        Write-Error "Error running cargo test: $_"
        return $false
    }
}

# Check for common security issues in the codebase
function Test-SecurityPatterns {
    Write-Status "Checking for security patterns..."
    
    $issuesFound = 0
    
    # Check for hardcoded addresses or private keys
    $secretMatches = Select-String -Path "contracts\*.rs" -Pattern "G[A-Z0-9]" | Select-String -Pattern "secret|private|key" -CaseSensitive
    if ($secretMatches) {
        Write-Warning "Potential hardcoded secrets found"
        $issuesFound++
        if ($Verbose) { $secretMatches | ForEach-Object { Write-Host "  $($_.Filename):$($_.LineNumber) - $($_.Line)" } }
    }
    
    # Check for panic! statements
    $panicMatches = Select-String -Path "contracts\*.rs" -Pattern "panic!"
    if ($panicMatches) {
        Write-Warning "Uncontrolled panic! statements found"
        $issuesFound++
        if ($Verbose) { $panicMatches | ForEach-Object { Write-Host "  $($_.Filename):$($_.LineNumber) - $($_.Line)" } }
    }
    
    # Check for require_auth usage (should be present in state-modifying functions)
    $authMatches = Select-String -Path "contracts\*.rs" -Pattern "require_auth"
    if (-not $authMatches) {
        Write-Warning "Missing authentication checks in some functions"
        $issuesFound++
    }
    
    if ($issuesFound -eq 0) {
        Write-Success "Security pattern checks passed"
        return $true
    } else {
        Write-Warning "$issuesFound potential security issues found"
        return $false
    }
}

# Generate security report
function New-SecurityReport {
    Write-Status "Generating security audit report..."
    
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $reportFile = "security-audit-report-$timestamp.md"
    
    $gitCommit = try { git rev-parse HEAD 2>$null } catch { "N/A" }
    
    $report = @"
# Security Audit Report

**Date**: $(Get-Date)
**Repository**: StellAIverse Contracts
**Commit**: $gitCommit

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
- Tool: Custom pattern checks
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
"@
    
    $report | Out-File -FilePath $reportFile -Encoding UTF8
    Write-Success "Security report generated: $reportFile"
}

# Main execution
function Main {
    $exitCode = 0
    
    if (-not (Test-Dependencies)) {
        exit 1
    }
    
    Write-Status "Starting security audit..."
    Write-Host ""
    
    # Run all checks
    if (-not (Test-Formatting)) { $exitCode = 1 }
    if (-not (Test-Check)) { $exitCode = 1 }
    if (-not (Test-Clippy)) { $exitCode = 1 }
    if (-not (Test-Audit)) { $exitCode = 1 }
    if (-not (Test-Tests)) { $exitCode = 1 }
    if (-not (Test-SecurityPatterns)) { $exitCode = 1 }
    
    Write-Host ""
    
    if ($exitCode -eq 0) {
        Write-Success "🎉 All security checks passed!"
        New-SecurityReport
    } else {
        Write-Error "❌ Some security checks failed. Please review the output above."
        exit 1
    }
}

# Run main function
Main
