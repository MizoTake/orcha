param(
    [switch]$Release
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$cargoArgs = @("build")
if ($Release) {
    $cargoArgs += "--release"
}

Write-Host ">> cargo $($cargoArgs -join ' ')"
& cargo @cargoArgs
exit $LASTEXITCODE
