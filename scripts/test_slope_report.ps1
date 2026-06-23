# Verify zero/adverse slope report labels via OCS --serve + HC_REPORT.
$ErrorActionPreference = "Stop"
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"

function Invoke-Ocs($cmds) {
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

function Get-CircleHandles($lines) {
    $q = @($lines | Where-Object { $_ -match '"entities":\[' } | Select-Object -Last 1)
    if (-not $q) { throw 'Circle query missing from OCS output' }
    $json = $q | ConvertFrom-Json
    return @{
        Inlet = ($json.entities | Where-Object { $_.radius -eq 3.0 }).handle
        Junction = ($json.entities | Where-Object { $_.radius -eq 4.0 }).handle
        Outfall = ($json.entities | Where-Object { $_.radius -eq 6.0 }).handle
    }
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
$flatSetup = Invoke-Ocs @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0 100 106 1.0 0.7"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0 100 106"}'
    '{"op":"query","type":"Circle"}'
)
$flatH = Get-CircleHandles $flatSetup
if (-not $flatH.Inlet -or -not $flatH.Outfall) { throw 'Flat inverts: could not resolve structure handles' }

Test-Report "Flat inverts" @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0 100 106 1.0 0.7"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0 100 106"}'
    "{`"op`":`"run`",`"cmd`":`"HC_PIPE_ARGS $($flatH.Inlet) $($flatH.Outfall) d18 n13`"}"
    "{`"op`":`"run`",`"cmd`":`"HC_EDIT $($flatH.Outfall) invert 100`"}"
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

# Adverse: raise downstream invert above upstream, then connect with serve-safe pipe args
$advSetup = Invoke-Ocs @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0 100 106 1.0 0.7"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0 100 106"}'
    '{"op":"query","type":"Circle"}'
)
$advH = Get-CircleHandles $advSetup
if (-not $advH.Inlet -or -not $advH.Outfall) { throw 'Adverse slope: could not resolve structure handles' }

Test-Report "Adverse slope" @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0 100 106 1.0 0.7"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0 100 106"}'
    "{`"op`":`"run`",`"cmd`":`"HC_PIPE_ARGS $($advH.Inlet) $($advH.Outfall) d18 n13`"}"
    "{`"op`":`"run`",`"cmd`":`"HC_EDIT $($advH.Outfall) invert 102`"}"
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

$pipeSetup = Invoke-Ocs @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0"}'
    '{"op":"run","cmd":"HC_JUNCTION 50,0"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0"}'
    '{"op":"query","type":"Circle"}'
)
$q = @($pipeSetup | Where-Object { $_ -match '"entities":\[' } | Select-Object -Last 1 | ConvertFrom-Json)
$inletH = ($q.entities | Where-Object { $_.radius -eq 3.0 }).handle
$junctH = ($q.entities | Where-Object { $_.radius -eq 4.0 }).handle
$outfallH = ($q.entities | Where-Object { $_.radius -eq 6.0 }).handle
if (-not $inletH -or -not $junctH -or -not $outfallH) { throw 'HC_PIPE_ARGS serve: could not resolve structure handles' }

Test-Report "HC_PIPE_ARGS serve" @(
    '{"op":"new"}'
    '{"op":"run","cmd":"HC_INLET 0,0"}'
    '{"op":"run","cmd":"HC_JUNCTION 50,0"}'
    '{"op":"run","cmd":"HC_OUTFALL 100,0"}'
    "{`"op`":`"run`",`"cmd`":`"HC_PIPE_ARGS $inletH $junctH d15 n13`"}"
    "{`"op`":`"run`",`"cmd`":`"HC_PIPE_ARGS $junctH $outfallH d18 n13`"}"
    "{`"op`":`"run`",`"cmd`":`"HC_EDIT $inletH area 2.0 c 0.75`"}"
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