# Build HydroComplete plugin, install to OCS, create Charlotte demo, export HTML report.
$ErrorActionPreference = "Stop"

$Root = Split-Path $PSScriptRoot -Parent
$Ocs = if ($env:HC_OCS_EXE) { $env:HC_OCS_EXE } else { Join-Path $env:USERPROFILE "Downloads\OpenCADStudio-v2026.36-windows-x86_64-portable.exe" }
# PowerShell 5.1 prefixes a UTF-8 BOM when piping to a native exe, so OCS --serve
# rejected the first request ({"op":"new"}) as invalid JSON. Pipe without a BOM.
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding $false
$OutputEncoding = New-Object System.Text.UTF8Encoding $false
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$LandXml = (Join-Path (Split-Path $PSScriptRoot -Parent) "crates/stormsewer/examples/sample_landxml.xml") -replace "\\", "/"
$OutDwg = Join-Path $ReportDir "hydrocomplete-demo-fixed.dwg"

if (-not (Test-Path $Ocs)) { throw "OpenCADStudio not found: $Ocs" }
if (-not (Test-Path $LandXml)) { throw "LandXML sample not found: $LandXml" }

& (Join-Path $PSScriptRoot "install_dev_plugin.ps1") -Root $Root

Start-Sleep -Seconds 2
$demoPath = ($OutDwg -replace '\\', '/')
$landXmlJson = $LandXml -replace '\\', '/'

# Host handle allocation changes between OCS releases (v0.6.0 gave the inlet 2B,
# v2026.36 gives 31), so read the inlet (radius 3.0) handle from a dry import.
$setup = [System.Collections.Generic.List[string]]::new()
$prev = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    @('{"op":"new"}', "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $landXmlJson`"}", '{"op":"query","type":"Circle"}') |
        & $Ocs --serve 2>&1 | ForEach-Object { $setup.Add([string]$_) }
} finally {
    $ErrorActionPreference = $prev
    $Error.Clear()
}
$q = @($setup | Where-Object { $_ -match '"entities":\[' } | Select-Object -Last 1 | ConvertFrom-Json)
$inletH = @($q.entities | Where-Object { $_.radius -eq 3.0 } | ForEach-Object { $_.handle }) | Select-Object -First 1
if (-not $inletH) { throw "Could not resolve LandXML inlet handle from OCS query" }

$requests = @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $landXmlJson`"}"
    '{"op":"query","type":"Circle"}'
    "{`"op`":`"run`",`"cmd`":`"HC_EDIT $inletH area 2.0 c 0.75`"}"
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    "{`"op`":`"save`",`"path`":`"$demoPath`"}"
)

Write-Host "Running OCS automation (LandXML + Charlotte IDF)..."
$output = [System.Collections.Generic.List[string]]::new()
$prev = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    @($requests) | & $Ocs --serve 2>&1 | ForEach-Object { $output.Add([string]$_) }
} finally {
    $ErrorActionPreference = $prev
    $Error.Clear()
}
$output = $output.ToArray()
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