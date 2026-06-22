# Run all HydroComplete OCS automation demos and report tests.
$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent

Write-Host "=== HydroComplete demo suite ==="
& (Join-Path $PSScriptRoot "build_hydro_demo.ps1")
Write-Host ""
& (Join-Path $PSScriptRoot "build_manual_pipe_demo.ps1")
Write-Host ""
& (Join-Path $PSScriptRoot "test_slope_report.ps1")
Write-Host ""
Write-Host "ALL DEMOS PASSED"