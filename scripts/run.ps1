param(
    [Parameter(Position = 0)]
    [ValidateSet("init", "run", "status", "profile", "explain", "help")]
    [string]$Command = "status",

    [string]$OrchDir = ".orcha",

    [ValidateSet("local_only", "cheap_checkpoints", "quality_gate", "unblock_first")]
    [string]$ProfileName,

    [switch]$Release,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$cliArgs = @("--orcha-dir", $OrchDir, $Command)

if ($Command -eq "profile") {
    if ([string]::IsNullOrWhiteSpace($ProfileName)) {
        Write-Error "Profile command requires -ProfileName (local_only|cheap_checkpoints|quality_gate|unblock_first)."
        exit 2
    }
    $cliArgs += $ProfileName
}

if ($Args) {
    $cliArgs += $Args
}

$cargoArgs = @("run")
if ($Release) {
    $cargoArgs += "--release"
}
$cargoArgs += "--"
$cargoArgs += $cliArgs

Write-Host ">> cargo $($cargoArgs -join ' ')"
& cargo @cargoArgs
exit $LASTEXITCODE
