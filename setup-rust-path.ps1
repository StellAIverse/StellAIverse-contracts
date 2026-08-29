# PowerShell script to add Rust/Cargo to PATH for current session
# Run this in your Cursor terminal: . .\setup-rust-path.ps1

$cargoBin = "$env:USERPROFILE\.cargo\bin"

if (Test-Path $cargoBin) {
    if ($env:PATH -notlike "*$cargoBin*") {
        $env:PATH = "$cargoBin;$env:PATH"
        Write-Host "✓ Added Rust/Cargo to PATH for this session" -ForegroundColor Green
        Write-Host "  Location: $cargoBin" -ForegroundColor Gray
    } else {
        Write-Host "✓ Rust/Cargo already in PATH" -ForegroundColor Green
    }
    
    Write-Host "`nVerifying installation..." -ForegroundColor Cyan
    & "$cargoBin\rustup.exe" --version
    & "$cargoBin\cargo.exe" --version
    & "$cargoBin\rustc.exe" --version
} else {
    Write-Host "✗ Rust not found at $cargoBin" -ForegroundColor Red
    Write-Host "  Please install Rust first: https://rustup.rs/" -ForegroundColor Yellow
}
