# GitHub Actions nightly E2E entry point.
# Wraps run_e2e_suite.ps1 with CI-friendly defaults (OCS path, DAG fixture, Civil 3D root).
param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent),
    [string]$Ocs,
    [string]$Civil3dRoot,
    [ValidateSet('OcsFrontend', 'Civil3dGui', 'Full')]
    [string]$Mode = 'OcsFrontend'
)

$ErrorActionPreference = 'Stop'

if (-not $Ocs) {
    $Ocs = if ($env:HC_OCS_EXE) { $env:HC_OCS_EXE }
    else { Join-Path $env:RUNNER_TEMP 'OpenCADStudio-v0.6.0-windows-x86_64-portable.exe' }
}

if (-not $Civil3dRoot) {
    $Civil3dRoot = if ($env:HC_CIVIL3D_ROOT) { $env:HC_CIVIL3D_ROOT }
    else { Join-Path (Split-Path $Root -Parent) 'hydrocomplete-civil3d' }
}

$dagRoot = Join-Path $Root '_dag'
$dagWww = Join-Path $dagRoot 'www'
$dagPkgWasm = Join-Path $dagWww 'pkg\hydrocomplete_dag_bg.wasm'

if (Test-Path (Join-Path $dagRoot 'Cargo.toml')) {
    if (-not (Test-Path $dagPkgWasm)) {
        Write-Host 'Building hydrocomplete-dag WASM (www/pkg missing)...'
        $prevEap = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        Push-Location $dagRoot
        try {
            & rustup target add wasm32-unknown-unknown 2>&1 | Out-Host
            if (-not (Get-Command wasm-pack -ErrorAction SilentlyContinue)) {
                & cargo install wasm-pack --locked 2>&1 | Out-Host
            }
            & wasm-pack build --target web --out-dir www/pkg --release 2>&1 | Out-Host
            if ($LASTEXITCODE -ne 0) { throw 'wasm-pack build failed' }
        }
        finally {
            Pop-Location
            $ErrorActionPreference = $prevEap
        }
    }
}

if (Test-Path (Join-Path $dagWww 'index.html')) {
    $env:HC_DAG_WWW = $dagWww
    Write-Host "DAG fixture: $dagWww"
}

$common = @{
    Root        = $Root
    Ocs         = $Ocs
    Civil3dRoot = $Civil3dRoot
}

Write-Host "Nightly E2E mode: $Mode"
Write-Host "OCS:  $Ocs"
Write-Host "Civil3D root: $Civil3dRoot"

switch ($Mode) {
    'OcsFrontend' {
        # Scheduled nightly: Rust + OCS automation + Playwright (no Civil 3D).
        & (Join-Path $PSScriptRoot 'run_e2e_suite.ps1') @common -SkipCivil3d -SkipCivil3dGui
    }
    'Civil3dGui' {
        # Manual / self-hosted: Civil 3D dotnet + GUI parity smoke (AutoCAD 2026 required).
        & (Join-Path $PSScriptRoot 'run_e2e_suite.ps1') @common `
            -SkipRust -SkipBuild -SkipOcs -Skip24145 -SkipFrontend
    }
    'Full' {
        # Everything the suite supports; 24-145 DWG and Civil 3D GUI skip gracefully when absent.
        & (Join-Path $PSScriptRoot 'run_e2e_suite.ps1') @common
    }
}