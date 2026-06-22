# HC_CIVIL_IMPORT on 24-145 X-DRAINAGE.dwg - Civil 3D I-SEWER-NETWORK bridge test.
$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"
$Dwg = "C:/Users/michael.flynn/Downloads/24-145 X-DRAINAGE.dwg"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$OutDwg = (Join-Path $ReportDir "24-145-civil-import.dwg") -replace '\\', '/'
$env:HYDROCOMPLETE_PRO = "1"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }
if (-not (Test-Path ($Dwg -replace '/', '\'))) { throw "DWG not found: $Dwg" }

& (Join-Path $PSScriptRoot "install_dev_plugin.ps1") -Root $Root
Start-Sleep -Seconds 2

function Invoke-Ocs([string[]]$cmds) {
    ($cmds -join "`n") | & $Ocs --serve 2>&1
}

Write-Host ""
Write-Host "=== 24-145 X-DRAINAGE - HC_CIVIL_IMPORT ==="
Write-Host "DWG: $Dwg"
Write-Host ""

$openJson = '{"op":"open","path":"' + $Dwg + '"}'
$saveJson = '{"op":"save","path":"' + $OutDwg + '"}'

Write-Host "--- Civil geometry probe ---"
$probe = Invoke-Ocs @(
    $openJson
    '{"op":"query","type":"Line","layer":"I-SEWER-NETWORK"}'
    '{"op":"query","type":"Block Reference","layer":"I-SEWER-NETWORK"}'
)
$probe | ForEach-Object { if ($_ -match 'count|total|"ok":false') { Write-Host $_ } }
foreach ($line in @($probe)) {
    if ($line -match '"ok":false') { throw "Probe failed: $line" }
}

Write-Host ""
Write-Host "--- HC_CIVIL_IMPORT ---"
$out = Invoke-Ocs @(
    $openJson
    '{"op":"run","cmd":"HC_CIVIL_IMPORT I-SEWER-NETWORK d15 n13 area 1.5 c 0.78 tc 15"}'
    '{"op":"run","cmd":"HC_PRIMARY_INLET"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_VALIDATE"}'
    '{"op":"run","cmd":"HC_PARAMS PRESET asheville-nc 10"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    '{"op":"run","cmd":"HC_REPORT_PDF"}'
    $saveJson
)
$out | ForEach-Object {
    if ($_ -match 'HC_|Civil|entities|"ok":false|pipe\(s\)|structure') { Write-Host $_ }
}
foreach ($line in @($out)) {
    if ($line -match '"ok":false') { throw "Step failed: $line" }
}

Start-Sleep -Milliseconds 1500
$html = Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
$pdf = Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $html) { throw "No HTML report" }
$c = Get-Content $html.FullName -Raw -Encoding UTF8
if ($c -notlike '*Manning Pipe Capacity*') { throw "Report missing Manning section" }
if ($c -match 'system total = <strong>([^<]+)') {
    $q = $matches[1].Trim()
    Write-Host "Design Q: $q"
    if ($q -eq '0.00 cfs') { throw "Design Q is zero" }
}

Write-Host ""
Write-Host "Saved:  $OutDwg"
Write-Host "Report: $($html.FullName)"
if ($pdf) { Write-Host "PDF:    $($pdf.FullName)" }

Start-Process $html.FullName
if ($pdf) { Start-Process $pdf.FullName }
$outPathWin = $OutDwg -replace '/', '\'
Start-Process -FilePath $Ocs -ArgumentList ('"' + $outPathWin + '"')

Write-Host ""
Write-Host "24-145 CIVIL IMPORT TEST PASSED"