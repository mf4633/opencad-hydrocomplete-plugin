# Verify zero/adverse slope report labels via OCS --serve + HC_REPORT.
$ErrorActionPreference = "Stop"
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"

function Invoke-Ocs($cmds) {
    ($cmds -join "`n") | & $Ocs --serve 2>&1
}

function Test-Report($name, $cmds, $mustMatch, $mustNotMatch) {
    Write-Host ""
    Write-Host "=== $name ==="
    $before = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)
    $out = Invoke-Ocs $cmds
    $out | ForEach-Object { if ($_ -match 'HC_|entities|"ok":false') { Write-Host $_ } }
    if ($out -match '"ok":false') { throw "$name : automation returned ok:false" }
    Start-Sleep -Milliseconds 1100
    $after = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)
    $maxBefore = ($before | Measure-Object -Property LastWriteTime -Maximum).Maximum
    $report = $after | Where-Object { -not $maxBefore -or $_.LastWriteTime -gt $maxBefore } |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $report) {
        $report = $after | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }
    if (-not $report) { throw "$name : no HTML report written" }
    $html = Get-Content $report.FullName -Raw -Encoding UTF8
    foreach ($s in $mustMatch) {
        if ($s -match '^/.*/$') {
            if ($html -notmatch $s) { throw "$name : report missing pattern '$s'" }
        } elseif ($html -notlike "*$s*") {
            throw "$name : report missing '$s'"
        }
        Write-Host "  OK matches: $s"
    }
    foreach ($s in $mustNotMatch) {
        if ($s -match '^/.*/$') {
            if ($html -match $s) { throw "$name : report should NOT match '$s'" }
        } elseif ($html -like "*$s*") {
            throw "$name : report should NOT contain '$s'"
        }
        Write-Host "  OK absent:  $s"
    }
    Write-Host "  Report: $($report.FullName)"
}

# Flat inverts: assumed 0.001 Manning slope; may legitimately surcharge at high Q
Test-Report "Flat inverts" @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0 100 106 1.0 0.7"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0 100 106"}'
    '{"op":"run","cmd":"HC_PIPE 2B 2C 1.5 0.013"}'
    '{"op":"run","cmd":"HC_REPORT"}'
) @(
    "0.0010*"
    "minimum assumed slope"
    'Q_{\text{full}} = 3.3'
) @(
    ">ZERO SLOPE"
    ">ADVERSE SLOPE"
    '<tr class="capacity-na">'
)

# Adverse: use HC_EDIT because --serve splits coordinate args on placement cmds
Test-Report "Adverse slope" @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0 100 106 1.0 0.7"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0 100 106"}'
    '{"op":"run","cmd":"HC_EDIT 44 invert 102"}'
    '{"op":"run","cmd":"HC_PIPE 2B 2C 1.5 0.013"}'
    '{"op":"run","cmd":"HC_REPORT"}'
) @(
    ">ADVERSE SLOPE"
    '<tr class="capacity-na">'
    "-0.0200"
) @(
    '<tr class="surcharged">'
)

Test-Report "LandXML healthy" @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $LandXml`"}"
    '{"op":"run","cmd":"HC_EDIT 43 area 1.0 c 0.7 tc 12"}'
    '{"op":"run","cmd":"HC_REPORT"}'
) @(
    "Network/P1"
    "Network/P2"
    "0.0133"
) @(
    ">ZERO SLOPE"
    ">ADVERSE SLOPE"
    '<tr class="capacity-na">'
)

Test-Report "HC_PIPE_ARGS serve" @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0"}'
    '{"op":"run","cmd":"HC_JUNCTION 50,0"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS 2B 2C d15 n13"}'
    '{"op":"run","cmd":"HC_PIPE_ARGS 2C 2D d18 n13"}'
    '{"op":"run","cmd":"HC_EDIT 2B area 2.0 c 0.75"}'
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_REPORT"}'
) @(
    "Network/P1"
    "Network/P1*</td><td>1.25"
    "Network/P2*</td><td>1.50"
    "a=81.2"
) @(
    "Network/P1*</td><td>1.50</td><td>0.0010*"
)

Write-Host ""
Write-Host "ALL SLOPE REPORT TESTS PASSED"