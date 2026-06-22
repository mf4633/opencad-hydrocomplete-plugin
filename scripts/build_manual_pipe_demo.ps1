# Manual pipe demo for OCS --serve (HC_PIPE_ARGS with d## n## — see docs/OCS_SERVE.md).
$ErrorActionPreference = "Stop"

$Root = Split-Path $PSScriptRoot -Parent
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$OutDwg = Join-Path $ReportDir "hydrocomplete-manual-pipe-demo.dwg"

if (-not (Test-Path $Ocs)) { throw "OpenCADStudio not found: $Ocs" }

Write-Host "Building release plugin..."
Push-Location $Root
$ErrorActionPreference = 'Continue'
cargo build --release -p opencad-hydrocomplete-plugin 2>&1 | Out-Host
$buildExit = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($buildExit -ne 0) { throw "cargo build failed" }
Pop-Location

Copy-Item -Force (Join-Path $Root "target\release\opencad_hydrocomplete_plugin.dll") `
    (Join-Path $PluginDir "opencad.hydrocomplete-windows-x86_64.dll")
Write-Host "Installed plugin DLL"

Start-Sleep -Seconds 1
$demoPath = ($OutDwg -replace '\\', '/')

$requests = @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0"}'
    '{"op":"run","cmd":"HC_JUNCTION 50,0"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS 2B 2C d15 n13"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS 2C 2D d18 n13"}'
    '{"op":"run","cmd":"HC_EDIT 2B area 2.0 c 0.75"}'
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    "{`"op`":`"save`",`"path`":`"$demoPath`"}"
)

Write-Host "Running manual pipe automation (HC_PIPE_ARGS d15/d18)..."
$output = ($requests -join "`n") | & $Ocs --serve 2>&1
$output | ForEach-Object { Write-Host $_ }

foreach ($line in @($output)) {
    if ($line -match '"ok":false') { throw "Automation step failed: $line" }
}

Start-Sleep -Seconds 1
$newReport = Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $newReport) { throw "No HTML report written" }

$html = Get-Content $newReport.FullName -Raw
if ($html -notlike '*a=81.2*') { throw "Report should use Charlotte NC IDF (a=81.2)" }
if ($html -notlike '*Network/P1*</td><td>1.25*') { throw "P1 diameter should be 1.25 ft" }
if ($html -notlike '*Network/P2*</td><td>1.50*') { throw "P2 diameter should be 1.50 ft" }
if ($html -like '*Network/P1*</td><td>1.50</td><td>0.0010*') { throw "P1 still flat/default diameter" }

Write-Host ""
Write-Host "=== Manual pipe demo ready ==="
Write-Host "DWG:    $OutDwg"
Write-Host "Report: $($newReport.FullName)"
Write-Host "MANUAL PIPE DEMO PASSED"