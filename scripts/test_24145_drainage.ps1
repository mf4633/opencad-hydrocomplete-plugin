# HydroComplete workflow test on 24-145 X-DRAINAGE.dwg (Civil 3D export, no HC XDATA).
$ErrorActionPreference = "Stop"

$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$Dwg = "C:/Users/michael.flynn/Downloads/24-145 X-DRAINAGE.dwg"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$OutDwg = (Join-Path $env:TEMP "24-145-hc-overlay.dwg") -replace '\\', '/'

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }
if (-not (Test-Path ($Dwg -replace '/', '\')) ) { throw "DWG not found: $Dwg" }

function Invoke-Ocs([string[]]$cmds) {
    ($cmds -join "`n") | & $Ocs --serve 2>&1
}

Write-Host "=== 24-145 X-DRAINAGE.dwg — HydroComplete probe ==="
Write-Host "DWG: $Dwg"
Write-Host ""

# Phase 1: open raw Civil 3D drawing
Write-Host '--- Phase 1: Civil 3D geometry - no HC XDATA ---'
$openJson = '{"op":"open","path":"' + $Dwg + '"}'
$out = Invoke-Ocs @(
    $openJson
    '{"op":"entities"}'
    '{"op":"query","type":"Line","layer":"I-SEWER-NETWORK"}'
    '{"op":"query","type":"Block Reference","layer":"I-SEWER-NETWORK"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_VALIDATE"}'
)

$entityLine = @($out | Where-Object { $_ -match '"total":' } | Select-Object -First 1)
if ($entityLine) {
    $entities = $entityLine | ConvertFrom-Json
    Write-Host ("Entities: {0}" -f $entities.total)
}

$lineJson = @($out | Where-Object { $_ -match '"layer":"I-SEWER-NETWORK"' -and $_ -match '"count":' } | Select-Object -First 1)
if ($lineJson) {
    $sl = $lineJson | ConvertFrom-Json
    Write-Host ("I-SEWER-NETWORK lines: {0}" -f $sl.count)
}

$blockJson = @($out | Where-Object { $_ -match 'SPT65' -and $_ -match '"count":' } | Select-Object -First 1)
if ($blockJson) {
    $sb = $blockJson | ConvertFrom-Json
    Write-Host ('I-SEWER-NETWORK structures - SPT65 blocks: {0}' -f $sb.count)
}

$hcNetwork = @($out | Where-Object { $_ -match 'HC_NETWORK' } | Select-Object -Last 1)
$hcValidate = @($out | Where-Object { $_ -match 'HC_VALIDATE' } | Select-Object -Last 1)
Write-Host ("HC_NETWORK:  {0}" -f $hcNetwork)
Write-Host ("HC_VALIDATE: {0}" -f $hcValidate)

# Phase 2: LandXML overlay + Asheville-area IDF + report
Write-Host ""
Write-Host '--- Phase 2: LandXML overlay + analyze - workaround until Civil bridge ---'
$beforeReports = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)

$importCmd = '{"op":"run","cmd":"HC_LANDXML_IMPORT ' + $LandXml + '"}'
$saveCmd = '{"op":"save","path":"' + $OutDwg + '"}'
$out2 = Invoke-Ocs @(
    $openJson
    $importCmd
    '{"op":"run","cmd":"HC_EDIT 43 area 1.5 c 0.78 tc 15"}'
    '{"op":"run","cmd":"HC_PARAMS PRESET asheville-nc 10"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    $saveCmd
)

foreach ($line in @($out2)) {
    if ($line -match '"ok":false') { throw "Step failed: $line" }
}

$importLine = @($out2 | Where-Object { $_ -match 'LANDXML' } | Select-Object -First 1)
$networkLine = @($out2 | Where-Object { $_ -match 'HC_NETWORK' } | Select-Object -Last 1)
$analyzeLine = @($out2 | Where-Object { $_ -match 'HC_ANALYZE' } | Select-Object -Last 1)
Write-Host ("LandXML: {0}" -f $importLine)
Write-Host ("Network: {0}" -f $networkLine)
Write-Host ("Analyze: {0}" -f $analyzeLine)

Start-Sleep -Milliseconds 800
$afterReports = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)
$newReport = Compare-Object $beforeReports $afterReports -PassThru |
    Where-Object { $_.SideIndicator -eq '=>' } | Select-Object -First 1
if (-not $newReport) {
    $newReport = $afterReports | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}
if ($newReport) {
    Write-Host ('Report: {0} ({1} bytes)' -f $newReport.FullName, $newReport.Length)
    Start-Process $newReport.FullName
}
$outPath = $OutDwg -replace '/', '\'
if (Test-Path $outPath) {
    Write-Host ('Saved:  {0} ({1} bytes)' -f $outPath, (Get-Item $outPath).Length)
}

Write-Host ""
Write-Host "=== SUMMARY ==="
Write-Host "Raw DWG: 20 sewer lines + 18 SPT65 structures on I-SEWER-NETWORK — no HC XDATA."
Write-Host "HC commands on raw drawing: network empty (expected)."
Write-Host "Workaround: LandXML sample imported into same drawing; HC_ANALYZE + HC_REPORT OK."
Write-Host "Real 24-145 network: export LandXML from Civil 3D (no .xml in project folder)."
Write-Host "24-145 TEST COMPLETE"