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
    $lines = [System.Collections.Generic.List[string]]::new()
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        @($cmds) | & $Ocs --serve 2>&1 | ForEach-Object { $lines.Add([string]$_) }
    } finally {
        $ErrorActionPreference = $prev
        $Error.Clear()
    }
    return $lines.ToArray()
}

function Get-StructCount($lines) {
    $q = @($lines | Where-Object { $_ -match '"entities":\[' } | Select-Object -Last 1)
    if (-not $q) { return 0 }
    return (@($q | ConvertFrom-Json).entities).Count
}

$openJson = '{"op":"open","path":"' + $Dwg + '"}'
$reopenJson = '{"op":"open","path":"' + $OutDwg + '"}'
$saveJson = '{"op":"save","path":"' + $OutDwg + '"}'

Write-Host "=== HC XDATA save/reopen (24-145) ==="

$phase1 = Invoke-Ocs @(
    $openJson
    '{"op":"run","cmd":"HC_CIVIL_IMPORT force"}'
    '{"op":"query","type":"Circle","layer":"HC-STRUCT"}'
    $saveJson
)
foreach ($line in @($phase1)) {
    if ($line -match '"ok":false') { throw "Phase 1 failed: $line" }
}
$structCount = Get-StructCount $phase1
if ($structCount -lt 1) { throw "Phase 1: expected HC-STRUCT circles after import, got $structCount" }
Write-Host "Before save: $structCount HC-STRUCT circle(s)"

Start-Sleep -Seconds 1

$phase2 = Invoke-Ocs @(
    $reopenJson
    '{"op":"query","type":"Circle","layer":"HC-STRUCT"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_VALIDATE"}'
)
foreach ($line in @($phase2)) {
    if ($line -match '"ok":false') { throw "Phase 2 failed: $line" }
}
$structCount2 = Get-StructCount $phase2
Write-Host "After reopen:  $structCount2 HC-STRUCT circle(s)"

if ($structCount2 -lt $structCount) {
    throw "XDATA lost after save/reopen: $structCount2 structures (was $structCount)"
}

Write-Host "Saved: $OutDwg"
Write-Host "XDATA SAVE/REOPEN TEST PASSED"