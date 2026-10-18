# Local CI: formatting, lints (warnings are errors), and the full test suite.
# Usage: powershell -File scripts/check.ps1
$ErrorActionPreference = "Stop"
$env:Path += ";$env:USERPROFILE\.cargo\bin"
Set-Location (Split-Path $PSScriptRoot -Parent)

function Step($name, [scriptblock]$cmd) {
    Write-Host "==> $name" -ForegroundColor Cyan
    & $cmd
    if ($LASTEXITCODE -ne 0) { Write-Host "FAILED: $name" -ForegroundColor Red; exit $LASTEXITCODE }
}

Step "cargo fmt --check" { cargo fmt --all -- --check }
Step "cargo clippy (deny warnings)" { cargo clippy --release --workspace --all-targets -- -D warnings }
Step "cargo test" { cargo test --release --workspace }
Write-Host "All checks passed." -ForegroundColor Green
