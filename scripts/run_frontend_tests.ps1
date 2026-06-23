# Generate HTML fixtures and run Playwright front-end tests (KaTeX reports + DAG editor).
param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent),
    [string]$LiveReport,
    [switch]$SkipFixture,
    [switch]$Headed
)
$ErrorActionPreference = 'Stop'
$frontend = Join-Path $Root 'tests\frontend'
$fixture = Join-Path $frontend 'fixtures\sample-report.html'

if (-not $SkipFixture) {
    Write-Host 'Generating report fixture from hydrocomplete sample network...'
    Push-Location $Root
    $env:HC_WRITE_FRONTEND_FIXTURE = '1'
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & cargo test -p hydrocomplete write_frontend_report_fixture -- --nocapture 2>&1 | Out-Host
    $fixtureExit = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } else { 0 }
    $ErrorActionPreference = $prevEap
    if ($fixtureExit -ne 0) { throw 'Fixture generation failed' }
    Pop-Location
    if (-not (Test-Path $fixture)) { throw "Fixture not written: $fixture" }
    Write-Host "Fixture: $fixture ($((Get-Item $fixture).Length) bytes)"
}

if (-not $LiveReport) {
    $docsReport = Get-ChildItem (Join-Path $env:USERPROFILE 'Documents\HydroComplete') `
        -Filter 'report-tab-*.html' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($docsReport) {
        $LiveReport = $docsReport.FullName
        Write-Host "Also validating live report: $LiveReport"
    }
}

Write-Host 'Installing Playwright (npm)...'
Push-Location $frontend
$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
if (-not (Test-Path 'node_modules')) {
    & npm install 2>&1 | Out-Host
}
$playwrightCli = Join-Path $frontend 'node_modules\.bin\playwright.cmd'
if (Test-Path $playwrightCli) {
    & $playwrightCli install chromium 2>&1 | Out-Host
} else {
    & npm exec -- playwright install chromium 2>&1 | Out-Host
}

$env:HC_REPORT_HTML = $LiveReport
$playwrightArgs = @('test')
if ($Headed) { $playwrightArgs += '--headed' }

Write-Host 'Running Playwright front-end tests...'
if (-not (Test-Path $playwrightCli)) { throw "Playwright CLI missing: $playwrightCli (run npm install in tests/frontend)" }
& $playwrightCli @playwrightArgs 2>&1 | Out-Host
$exit = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } else { 0 }
$ErrorActionPreference = $prevEap
Pop-Location

if ($exit -ne 0) { throw "Playwright tests failed (exit $exit)" }
Write-Host 'FRONTEND TESTS PASSED'