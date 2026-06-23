# GitHub Actions / CI entry: Rust unit tests + Playwright front-end (no OCS or Civil 3D desktop).
param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = 'Stop'

$dagCheckout = Join-Path $Root '_dag\www'
if (Test-Path (Join-Path $dagCheckout 'index.html')) {
    $env:HC_DAG_WWW = $dagCheckout
    Write-Host "DAG fixture: $dagCheckout"
}

& (Join-Path $PSScriptRoot 'run_e2e_suite.ps1') `
    -Root $Root `
    -SkipBuild `
    -SkipOcs `
    -Skip24145 `
    -SkipCivil3d `
    -SkipCivil3dGui