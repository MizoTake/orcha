param(
    [switch]$SkipCheck
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if (-not $SkipCheck) {
    Write-Host ">> cargo check"
    & cargo check
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Write-Host ">> cargo test --lib"
& cargo test --lib
exit $LASTEXITCODE
