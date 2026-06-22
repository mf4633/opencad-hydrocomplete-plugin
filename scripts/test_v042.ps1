# v0.4.2 integration test: HC_PIPE_ARGS, HC_REPORT, HC_REPORT_PDF, ribbon commands.
$ErrorActionPreference = "Stop"
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }

$env:HYDROCOMPLETE_PRO = "1"

function Invoke-Ocs($cmds) {
    ($cmds -join "`n") | & $Ocs --serve 2>&1
}

function Assert-NoFail($out, $label) {
    foreach ($line in @($out)) {
        if ($line -match '"ok":false') { throw "$label failed: $line" }
    }
    Write-Host "  OK $label"
}

function Latest-Report($ext) {
    Get-ChildItem $ReportDir -Filter "report-tab-*.$ext" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
}

Write-Host "=== v0.4.2 serve integration ==="

# 1) HC_PIPE_ARGS vs broken HC_PIPE decimal args
Write-Host ""
Write-Host "--- HC_PIPE_ARGS (serve-safe) ---"
$before = (Get-Date)
$out = Invoke-Ocs @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0"}'
    '{"op":"run","cmd":"HC_JUNCTION 50,0"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS 2B 2C d15 n13"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS 2C 2D d18 n13"}'
    '{"op":"run","cmd":"HC_EDIT 2B area 2.0 c 0.75"}'
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_REPORT"}'
)
Assert-NoFail $out "HC_PIPE_ARGS pipeline"
Start-Sleep -Milliseconds 800
$html = Get-Content (Latest-Report "html").FullName -Raw
if ($html -notlike '*Network/P1*</td><td>1.25*') { throw "P1 should be 1.25 ft" }
if ($html -notlike '*Network/P2*</td><td>1.50*') { throw "P2 should be 1.50 ft" }
if ($html -notlike '*a=81.2*') { throw "Charlotte IDF missing" }
Write-Host "  OK report P1=1.25 P2=1.50 Charlotte a=81.2"

# 2) HC_REPORT_PDF (Pro)
Write-Host ""
Write-Host "--- HC_REPORT_PDF ---"
$pdfBefore = @(Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -EA SilentlyContinue)
$out = Invoke-Ocs @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $LandXml`"}"
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_REPORT_PDF"}'
)
Assert-NoFail $out "HC_REPORT_PDF"
Start-Sleep -Milliseconds 500
$pdf = Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -EA SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $pdf -or $pdf.Length -lt 500) { throw "PDF not written or too small" }
Write-Host "  OK PDF $($pdf.Name) ($($pdf.Length) bytes)"

# 3) Ribbon command stubs (usage lines)
Write-Host ""
Write-Host "--- Ribbon / command stubs ---"
$out = Invoke-Ocs @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS"}'
    '{"op":"run","cmd":"HC_PIPE"}'
    '{"op":"run","cmd":"HC_ABOUT"}'
)
Assert-NoFail $out "command stubs"

# 4) LandXML demo parity
Write-Host ""
Write-Host "--- LandXML report ---"
$out = Invoke-Ocs @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $LandXml`"}"
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_REPORT"}'
)
Assert-NoFail $out "LandXML report"
Start-Sleep -Milliseconds 800
$lx = Get-Content (Latest-Report "html").FullName -Raw
if ($lx -notmatch '0\.0133') { throw "LandXML slope 0.0133 missing" }
if ($lx -notmatch '7\.4[0-9]') { throw "LandXML P1 Qfull ~7.48 missing" }
Write-Host "  OK LandXML slope 0.0133 Qfull ~7.48"

Write-Host ""
Write-Host "V0.4.2 INTEGRATION TEST PASSED"