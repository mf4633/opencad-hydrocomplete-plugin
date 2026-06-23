# GitHub Actions / CI entry: Rust unit tests + Playwright front-end (no OCS or Civil 3D desktop).
param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = 'Stop'

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

& (Join-Path $PSScriptRoot 'run_e2e_suite.ps1') `
    -Root $Root `
    -SkipBuild `
    -SkipOcs `
    -Skip24145 `
    -SkipCivil3d `
    -SkipCivil3dGui