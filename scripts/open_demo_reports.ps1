# Open saved demo DWGs, run HC_REPORT (+ PDF), verify Charlotte IDF persisted in drawing.
$ErrorActionPreference = "Stop"
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$env:HYDROCOMPLETE_PRO = "1"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }

function Latest-Report($ext) {
    Start-Sleep -Milliseconds 1500
    Get-ChildItem $ReportDir -Filter "report-tab-*.$ext" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

function Test-OpenedDwg($label, $dwgPath, $must) {
    Write-Host ""
    Write-Host "=== $label ==="
    $openJson = '{"op":"open","path":"' + $dwgPath + '"}'
    $requests = @(
        $openJson
        '{"op":"run","cmd":"HC_REPORT"}'
        '{"op":"run","cmd":"HC_REPORT_PDF"}'
    )
    $output = ($requests -join "`n") | & $Ocs --serve 2>&1
    foreach ($line in @($output)) {
        if ($line -match '"ok":false') { throw "${label} failed: $line" }
    }
    $html = Latest-Report "html"
    $pdf  = Latest-Report "pdf"
    if (-not $html) { throw "${label}: no HTML report" }
    if (-not $pdf -or $pdf.Length -lt 500) { throw "${label}: no PDF report" }
    $c = Get-Content $html.FullName -Raw -Encoding UTF8
    foreach ($s in $must) {
        if ($c -notlike "*$s*") { throw "${label} missing: $s" }
        Write-Host "  OK: $s"
    }
    if ($c -match 'system total = <strong>([^<]+)') {
        $q = $matches[1].Trim()
        if ($q -eq '0.00 cfs' -or $q -eq '0.0 cfs') { throw "${label}: design Q is zero" }
        Write-Host "  OK: Q total $q"
    }
    Write-Host "  HTML: $($html.FullName)"
    Write-Host "  PDF:  $($pdf.FullName)"
    return @($html.FullName, $pdf.FullName)
}

$paths = @()
$paths += Test-OpenedDwg "LandXML demo DWG" "C:/Users/michael.flynn/Documents/HydroComplete/hydrocomplete-demo-fixed.dwg" @(
    "a=81.2", "Network/P1", "Network/P2", "0.0133", "Manning Pipe Capacity"
)
Start-Sleep -Seconds 2
$paths += Test-OpenedDwg "Manual pipe demo DWG" "C:/Users/michael.flynn/Documents/HydroComplete/hydrocomplete-manual-pipe-demo.dwg" @(
    "a=81.2", "Network/P1*</td><td>1.25", "Network/P2*</td><td>1.50"
)

Write-Host ""
Write-Host "Opening reports..."
foreach ($p in $paths) { Start-Process $p }
Write-Host "OPEN DEMO REPORTS PASSED"