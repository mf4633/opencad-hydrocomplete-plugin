# HydroComplete OpenCAD end-to-end smoke test via OCS --serve automation.
$ErrorActionPreference = "Stop"

$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$OutDwg = Join-Path $env:TEMP "hc_smoke_test.dwg"

if (-not (Test-Path $Ocs)) { throw "OpenCADStudio not found: $Ocs" }
if (-not (Test-Path $LandXml)) { throw "LandXML sample not found: $LandXml" }

$beforeReports = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)

$requests = @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_ABOUT"}'
    '{"op":"run","cmd":"HC_PARAMS RP 10"}'
    ('{{"op":"run","cmd":"HC_LANDXML_IMPORT {0}"}}' -f $LandXml)
    '{"op":"entities"}'
    '{"op":"query","type":"Circle"}'
    '{"op":"query","type":"Line"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_VALIDATE"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_PIPES"}'
    '{"op":"run","cmd":"HC_CAPACITY"}'
    '{"op":"run","cmd":"HC_HGL"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    '{"op":"run","cmd":"HC_MULTIRP"}'
    ('{{"op":"save","path":"{0}"}}' -f ($OutDwg -replace '\\', '/'))
    '{"op":"entities"}'
)

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

$lines = @($output)
$fail = $false
foreach ($line in $lines) {
    if ($line -match '"ok":false') { $fail = $true }
}

# Parse key steps
$importLine = $lines | Where-Object { $_ -match 'HC_LANDXML_IMPORT' } | Select-Object -First 1
$analyzeLine = $lines | Where-Object { $_ -match 'HC_ANALYZE' } | Select-Object -First 1
$reportLine = $lines | Where-Object { $_ -match 'HC_REPORT' } | Select-Object -First 1
$finalLine = $lines | Select-Object -Last 1

Write-Host ""
Write-Host "=== Summary ==="
if ($importLine) { Write-Host "LandXML import: $importLine" }
if ($analyzeLine) { Write-Host "Analyze:        $analyzeLine" }
if ($reportLine) { Write-Host "Report:         $reportLine" }
if ($finalLine) { Write-Host "Final entities: $finalLine" }

$afterReports = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)
$newReport = Compare-Object $beforeReports $afterReports -PassThru | Where-Object { $_.SideIndicator -eq '=>' } | Select-Object -First 1
if (-not $newReport) {
    $newReport = $afterReports | Sort-Object LastWriteTime -Descending | Select-Object -First 1
}

if ($newReport -and (Test-Path $newReport.FullName)) {
    Write-Host "HTML report:    $($newReport.FullName) ($($newReport.Length) bytes)"
    $html = Get-Content $newReport.FullName -Raw
    foreach ($needle in @("HydroComplete", "Manning", "katex", "HGL")) {
        if ($html -notmatch [regex]::Escape($needle)) { throw "Report missing: $needle" }
    }
    Write-Host "Report content: OK (HydroComplete, Manning, KaTeX, HGL)"
    Start-Process $newReport.FullName
} else {
    throw "No HTML report found in $ReportDir"
}

if (-not (Test-Path $OutDwg)) { throw "DWG save failed: $OutDwg" }
Write-Host "Saved DWG:      $OutDwg ($((Get-Item $OutDwg).Length) bytes)"

if ($fail) { throw "One or more automation steps returned ok:false" }
Write-Host "SMOKE TEST PASSED"