# HC XDATA DWG save/reopen: HC_CIVIL_IMPORT → save → reopen → HC_NETWORK must find structures.
$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$Dwg = "C:/Users/michael.flynn/Downloads/24-145 X-DRAINAGE.dwg"
$OutDwg = (Join-Path $env:TEMP "hc-xdata-roundtrip.dwg") -replace '\\', '/'
$env:HYDROCOMPLETE_PRO = "1"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }
if (-not (Test-Path ($Dwg -replace '/', '\'))) { throw "DWG not found: $Dwg" }

& (Join-Path $PSScriptRoot "install_dev_plugin.ps1") -Root $Root
Start-Sleep -Seconds 2

function Invoke-Ocs([string[]]$cmds) {
    ($cmds -join "`n") | & $Ocs --serve 2>&1
}

$openJson = '{"op":"open","path":"' + $Dwg + '"}'
$reopenJson = '{"op":"open","path":"' + $OutDwg + '"}'
$saveJson = '{"op":"save","path":"' + $OutDwg + '"}'

Write-Host "=== HC XDATA save/reopen (24-145) ==="

$phase1 = Invoke-Ocs @(
    $openJson
    '{"op":"run","cmd":"HC_CIVIL_IMPORT force"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    $saveJson
)
foreach ($line in @($phase1)) {
    if ($line -match '"ok":false') { throw "Phase 1 failed: $line" }
}
$net1 = @($phase1 | Where-Object { $_ -match 'HC_NETWORK' } | Select-Object -Last 1)
if ($net1 -notmatch 'structure') { throw "Phase 1: expected structures in HC_NETWORK: $net1" }
Write-Host "Before save: $net1"

Start-Sleep -Seconds 1

$phase2 = Invoke-Ocs @(
    $reopenJson
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_VALIDATE"}'
)
foreach ($line in @($phase2)) {
    if ($line -match '"ok":false') { throw "Phase 2 failed: $line" }
}
$net2 = @($phase2 | Where-Object { $_ -match 'HC_NETWORK' } | Select-Object -Last 1)
$val2 = @($phase2 | Where-Object { $_ -match 'HC_VALIDATE' } | Select-Object -Last 1)
Write-Host "After reopen:  $net2"
Write-Host "Validate:      $val2"

if ($net2 -match 'No storm-sewer structures' -or $net2 -match '0 structure') {
    throw "XDATA lost after save/reopen: $net2"
}
if ($val2 -match 'No storm-sewer structures') {
    throw "Validate failed after reopen: $val2"
}

Write-Host "Saved: $OutDwg"
Write-Host "XDATA SAVE/REOPEN TEST PASSED"