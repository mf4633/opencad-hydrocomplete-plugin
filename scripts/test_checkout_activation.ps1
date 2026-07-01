# Post-checkout activation walkthrough — online validate + Pro PDF (no dev bypass).
param(
    [string]$Email = "checkout-walkthrough@hydrocomplete.test",
    [string]$LicenseKey = "hc_live_ocs_8d32ab957bd3"
)
$ErrorActionPreference = "Stop"

$Ocs = "C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe"
$LandXml = "C:/Users/michael.flynn/dev/opencad-hydrocomplete-plugin/crates/stormsewer/examples/sample_landxml.xml"
$LicensePath = Join-Path $env:APPDATA "HydroComplete/opencad-license.json"
$ReportDir = Join-Path $env:USERPROFILE "Documents/HydroComplete"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }

Write-Host "=== 1. Validate key on production API ==="
$body = @{
    licenseKey = $LicenseKey
    product    = "opencad"
    features   = @("reports", "export")
    email      = $Email
} | ConvertTo-Json
$validate = Invoke-RestMethod -Uri "https://hydrocomplete.com/api/licensing/validate" -Method Post -ContentType "application/json" -Body $body
if (-not $validate.valid) { throw "API validate failed: $($validate | ConvertTo-Json -Compress)" }
Write-Host "  API valid=true product=$($validate.license.product)"

Write-Host "=== 2. Clear prior license file ==="
if (Test-Path $LicensePath) {
    $bak = "$LicensePath.bak-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
    Copy-Item $LicensePath $bak
    Remove-Item $LicensePath -Force
    Write-Host "  Backed up to $bak"
}

Remove-Item Env:HYDROCOMPLETE_PRO -ErrorAction SilentlyContinue

function Invoke-OcsServe([string[]]$Lines) {
    $tmp = Join-Path $env:TEMP "hc-activate-$(Get-Random).jsonl"
    Set-Content -Path $tmp -Value ($Lines -join "`n") -Encoding UTF8
    try {
        Get-Content -Raw $tmp | & $Ocs --serve 2>&1
    } finally {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "=== 3. HC_ACTIVATE in OCS ==="
$activateCmd = "HC_ACTIVATE $Email $LicenseKey"
$actOut = Invoke-OcsServe @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"$activateCmd`"}"
    '{"op":"run","cmd":"HC_LICENSE"}'
)
$actOut | ForEach-Object { Write-Host $_ }
foreach ($line in @($actOut)) {
    if ($line -match '"ok":false') { throw "Activation step failed: $line" }
}

Start-Sleep -Milliseconds 500
if (-not (Test-Path $LicensePath)) { throw "License file not written: $LicensePath" }
$lic = Get-Content $LicensePath -Raw | ConvertFrom-Json
Write-Host "  License file: product=$($lic.product) mode=$($lic.validationMode) expires=$($lic.expires)"
if ($lic.product -ne "opencad") { throw "Wrong product in license file" }

Write-Host "=== 4. Pro workflow (LandXML + HC_REPORT_PDF) ==="
$pdfBefore = @(Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -ErrorAction SilentlyContinue)
$wfOut = Invoke-OcsServe @(
    '{"op":"new"}'
    "{`"op`":`"run`",`"cmd`":`"HC_LANDXML_IMPORT $LandXml`"}"
    '{"op":"run","cmd":"HC_PARAMS PRESET charlotte-nc 10"}'
    '{"op":"run","cmd":"HC_ANALYZE"}'
    '{"op":"run","cmd":"HC_REPORT_PDF"}'
)
$wfOut | ForEach-Object { Write-Host $_ }
foreach ($line in @($wfOut)) {
    if ($line -match '"ok":false') { throw "Pro workflow failed: $line" }
}

Start-Sleep -Milliseconds 800
$pdf = Get-ChildItem $ReportDir -Filter "report-tab-*.pdf" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $pdf -or $pdf.Length -lt 500) { throw "HC_REPORT_PDF did not produce a PDF" }
$bytes = [System.IO.File]::ReadAllBytes($pdf.FullName)
if ([System.Text.Encoding]::ASCII.GetString($bytes, 0, 4) -ne "%PDF") { throw "Not a valid PDF: $($pdf.FullName)" }

Write-Host "  PDF: $($pdf.FullName) ($($pdf.Length) bytes)"
Write-Host ""
Write-Host "CHECKOUT ACTIVATION WALKTHROUGH PASSED"
Write-Host "  Email: $Email"
Write-Host "  Key:   $LicenseKey"
Write-Host "  Activate in GUI: HC_ACTIVATE $Email $LicenseKey"