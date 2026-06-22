# Build HydroComplete plugin, install to OCS, create Charlotte demo, export HTML report.
$ErrorActionPreference = "Stop"

$Root = Split-Path $PSScriptRoot -Parent
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"
$OutDwg = Join-Path $ReportDir "hydrocomplete-demo-fixed.dwg"

if (-not (Test-Path $Ocs)) { throw "OpenCADStudio not found: $Ocs" }
if (-not (Test-Path $LandXml)) { throw "LandXML sample not found: $LandXml" }

& (Join-Path $PSScriptRoot "install_dev_plugin.ps1") -Root $Root

Start-Sleep -Seconds 2
$demoPath = ($OutDwg -replace '\\', '/')
$landXmlJson = $LandXml -replace '\\', '/'

$requests = @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $landXmlJson`"}"
    '{"op":"query","type":"Circle"}'
    '{"op":"run","cmd":"HC_EDIT 2B area 2.0 c 0.75"}'
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    "{`"op`":`"save`",`"path`":`"$demoPath`"}"
)

Write-Host "Running OCS automation (LandXML + Charlotte IDF)..."
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
if ($html -notlike '*Manning Pipe Capacity*') { throw "Report missing Manning section" }
if ($html -notlike '*a=81.2*') { throw "Report should use Charlotte NC IDF (a=81.2)" }
if ($html -like '*Network/P1*</td><td>1.25</td><td>0.0000*') { throw "P1 still has zero bed slope" }
if ($html -notmatch 'Q<sub>full</sub> \(cfs\)</th>[\s\S]*<td>\d+\.\d+</td>') {
    throw "Report Manning Qfull still zero"
}

Write-Host ""
Write-Host "=== Demo ready ==="
Write-Host "DWG:    $OutDwg"
Write-Host "Report: $($newReport.FullName)"
Write-Host "DEMO BUILD PASSED"