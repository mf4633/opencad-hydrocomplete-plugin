# Full 24-145 workflow: HC_CIVIL_IMPORT (v0.4.5 labels) + worksheet hydrology + analyze/report.
$ErrorActionPreference = "Stop"
$Root = Split-Path $PSScriptRoot -Parent
$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"
$Dwg = "C:/Users/michael.flynn/Downloads/24-145 X-DRAINAGE.dwg"
$ReportDir = Join-Path $env:USERPROFILE "Documents\HydroComplete"
$OutDwg = (Join-Path $env:TEMP "24-145-hc-full.dwg") -replace '\\', '/'
$WorkDir = Join-Path $env:TEMP "hc-24145"
$env:HYDROCOMPLETE_PRO = "1"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }
if (-not (Test-Path ($Dwg -replace '/', '\'))) { throw "DWG not found: $Dwg" }
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null

& (Join-Path $PSScriptRoot "install_dev_plugin.ps1") -Root $Root
Write-Host "Installed plugin DLL"
Start-Sleep -Seconds 2

function Invoke-Ocs([string[]]$cmds) {
    $lines = [System.Collections.Generic.List[string]]::new()
    @($cmds) | & $Ocs --serve 2>&1 | ForEach-Object {
        $lines.Add([string]$_)
    }
    return $lines.ToArray()
}

$openJson = '{"op":"open","path":"' + $Dwg + '"}'
$saveJson = '{"op":"save","path":"' + $OutDwg + '"}'

Write-Host ""
Write-Host "=== 24-145 full workflow ==="

# Civil import + probe structures/labels
$probeOut = Invoke-Ocs @(
    $openJson
    '{"op":"run","cmd":"HC_CIVIL_IMPORT force"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"query","type":"Circle","layer":"HC-STRUCT"}'
    '{"op":"query","type":"MText","layer":"I-SEWER-NETWORK"}'
)
foreach ($line in @($probeOut)) {
    if ($line -match '"ok":false') { throw "Probe failed: $line" }
}

function Get-OcsJsonLine([object[]]$lines, [scriptblock]$Predicate) {
    foreach ($line in $lines) {
        if ($line -notmatch '^\{') { continue }
        try {
            $j = $line | ConvertFrom-Json
            if (& $Predicate $j) { return $j }
        } catch {}
    }
    return $null
}

$importLine = @($probeOut | Where-Object { $_ -match 'HC_CIVIL_IMPORT' } | Select-Object -Last 1)
Write-Host $importLine

$circlesJson = Get-OcsJsonLine $probeOut {
    param($j)
    $j.PSObject.Properties.Name -contains 'entities' -and
        $j.entities.Count -gt 0 -and
        $j.entities[0].layer -eq 'HC-STRUCT'
}
$mtextJson = Get-OcsJsonLine $probeOut {
    param($j)
    $j.PSObject.Properties.Name -contains 'entities' -and
        $j.entities.Count -gt 0 -and
        $j.entities[0].type -eq 'MText'
}
if (-not $circlesJson) { throw "No HC-STRUCT query result" }
($circlesJson | ConvertTo-Json -Depth 8 -Compress) | Out-File (Join-Path $WorkDir "structures.json") -Encoding utf8
if ($mtextJson) {
    ($mtextJson | ConvertTo-Json -Depth 8 -Compress) | Out-File (Join-Path $WorkDir "mtext.json") -Encoding utf8
}

$structJson = $circlesJson
$structList = @()
foreach ($ent in $structJson.entities) {
    $structList += @{
        handle = $ent.handle
        x      = $ent.center[0]
        y      = $ent.center[1]
        kind   = if ($ent.radius -le 3.5) { "inlet" } elseif ($ent.radius -ge 5.5) { "outfall" } else { "junction" }
    }
}
[System.IO.File]::WriteAllText(
    (Join-Path $WorkDir "struct-list.json"),
    ($structList | ConvertTo-Json -Depth 4),
    (New-Object System.Text.UTF8Encoding $false)
)

$mtextPath = Join-Path $WorkDir "mtext.json"
$pyArgs = @(
    (Join-Path $PSScriptRoot "extract_24145_hydrology.py"),
    (Join-Path $WorkDir "struct-list.json")
)
if (Test-Path $mtextPath) { $pyArgs += $mtextPath }
$hydroJson = python @pyArgs | ConvertFrom-Json
Write-Host ("Worksheet hydrology: {0} structures, {1} inlets, {2} HC_EDIT cmds" -f $hydroJson.structures, $hydroJson.inlets, $hydroJson.edits.Count)
$hydroJson | ConvertTo-Json -Depth 6 | Out-File (Join-Path $WorkDir "hydrology-plan.json") -Encoding utf8

function Get-NewReportFile {
    param(
        [System.IO.FileInfo[]]$After,
        [System.IO.FileInfo[]]$Before,
        [datetime]$Since,
        [string]$Label
    )
    $maxBefore = ($Before | Measure-Object -Property LastWriteTime -Maximum).Maximum
    $cutoff = if ($maxBefore) { $maxBefore } else { $Since }
    $newByCompare = @(Compare-Object $Before $After -PassThru |
        Where-Object { $_.SideIndicator -eq '=>' -and $_.LastWriteTime -gt $cutoff } |
        Sort-Object LastWriteTime -Descending)
    if ($newByCompare.Count -gt 0) { return $newByCompare[0] }
    $updated = @($After | Where-Object { $_.LastWriteTime -gt $cutoff } |
        Sort-Object LastWriteTime -Descending)
    if ($updated.Count -gt 0) { return $updated[0] }
    throw "$Label did not produce a new file in $ReportDir (cutoff: $cutoff)"
}

function Assert-PdfReport {
    param([System.IO.FileInfo]$Pdf)
    if (-not $Pdf -or -not (Test-Path -LiteralPath $Pdf.FullName)) {
        throw "HC_REPORT_PDF: PDF file missing"
    }
    if ($Pdf.Length -le 500) {
        throw "HC_REPORT_PDF: PDF too small ($($Pdf.Length) bytes): $($Pdf.FullName)"
    }
    $bytes = [System.IO.File]::ReadAllBytes($Pdf.FullName)
    if ($bytes.Length -lt 4) {
        throw "HC_REPORT_PDF: PDF unreadable: $($Pdf.FullName)"
    }
    $magic = [System.Text.Encoding]::ASCII.GetString($bytes, 0, 4)
    if ($magic -ne '%PDF') {
        throw "HC_REPORT_PDF: invalid PDF magic bytes (got '$magic'): $($Pdf.FullName)"
    }
}

$workflowStart = Get-Date
$beforeReports = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)
$beforePdf = @(Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -ErrorAction SilentlyContinue)
$cmds = @($openJson, '{"op":"run","cmd":"HC_CIVIL_IMPORT force"}')
foreach ($edit in $hydroJson.edits) {
    $cmds += '{"op":"run","cmd":"' + $edit + '"}'
}
$cmds += @(
    '{"op":"run","cmd":"HC_PARAMS PRESET asheville-nc 10"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
    '{"op":"run","cmd":"HC_VALIDATE"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT"}'
    '{"op":"run","cmd":"HC_REPORT_PDF"}'
    $saveJson
)

$out = Invoke-Ocs $cmds
foreach ($line in @($out)) {
    if ($line -match '"ok":false') { throw "Step failed: $line" }
    if ($line -match 'HC_|Civil|Design|pipe\(s\)|structure|Manning') { Write-Host $line }
}

Start-Sleep -Milliseconds 1200
$afterReports = @(Get-ChildItem $ReportDir -Filter "report-tab-*.html" -ErrorAction SilentlyContinue)
$newReport = Get-NewReportFile -After $afterReports -Before $beforeReports -Since $workflowStart -Label "HC_REPORT"

$c = Get-Content $newReport.FullName -Raw -Encoding UTF8
if ($c -match 'system total = <strong>([^<]+)') {
    Write-Host ("Design Q: {0}" -f $matches[1].Trim())
}
Write-Host "Report: $($newReport.FullName) ($($newReport.Length) bytes)"

$afterPdf = @(Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -ErrorAction SilentlyContinue)
$pdf = Get-NewReportFile -After $afterPdf -Before $beforePdf -Since $workflowStart -Label "HC_REPORT_PDF"
Assert-PdfReport -Pdf $pdf
Write-Host "PDF:    $($pdf.FullName) ($($pdf.Length) bytes)"

Start-Process $newReport.FullName
Start-Process $pdf.FullName
Write-Host "Saved:  $OutDwg"
Write-Host "Plan:   $(Join-Path $WorkDir 'hydrology-plan.json')"
Write-Host "24-145 FULL WORKFLOW COMPLETE"