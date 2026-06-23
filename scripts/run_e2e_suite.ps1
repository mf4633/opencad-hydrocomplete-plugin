# HydroComplete end-to-end test suite: Rust, OCS automation, 24-145, Playwright, Civil 3D.
param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent),
    [string]$Ocs = $(if ($env:HC_OCS_EXE) { $env:HC_OCS_EXE } else { 'C:\Users\michael.flynn\Downloads\OpenCADStudio-v0.6.0-windows-x86_64-portable.exe' }),
    [string]$Civil3dRoot = $(if ($env:HC_CIVIL3D_ROOT) { $env:HC_CIVIL3D_ROOT } else { (Join-Path (Split-Path $Root -Parent) 'hydrocomplete-civil3d') }),
    [switch]$SkipRust,
    [switch]$SkipBuild,
    [switch]$SkipOcs,
    [switch]$Skip24145,
    [switch]$SkipFrontend,
    [switch]$SkipCivil3d,
    [switch]$SkipCivil3dGui,
    [switch]$KeepExistingAcad
)

$ErrorActionPreference = 'Stop'
$started = Get-Date
$results = [System.Collections.Generic.List[object]]::new()

function Invoke-External {
    param([scriptblock]$Action)
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Action
        $ok = $?
        $code = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } elseif ($ok) { 0 } else { 1 }
        if ($code -ne 0) { throw "exit code $code" }
    }
    finally {
        $ErrorActionPreference = $prev
    }
}

function Invoke-E2EStep {
    param(
        [string]$Name,
        [scriptblock]$Action,
        [switch]$Optional
    )
    Write-Host ''
    Write-Host ('=' * 72)
    Write-Host "E2E: $Name"
    Write-Host ('=' * 72)
    $stepStart = Get-Date
    try {
        Invoke-External $Action
        $elapsed = (Get-Date) - $stepStart
        $results.Add([pscustomobject]@{ Step = $Name; Status = 'PASS'; Seconds = [math]::Round($elapsed.TotalSeconds, 1) })
        Write-Host "PASS: $Name ($([math]::Round($elapsed.TotalSeconds, 1))s)"
    }
    catch {
        $elapsed = (Get-Date) - $stepStart
        if ($Optional) {
            $results.Add([pscustomobject]@{ Step = $Name; Status = 'SKIP'; Seconds = [math]::Round($elapsed.TotalSeconds, 1); Note = $_.Exception.Message })
            Write-Host "SKIP: $Name — $($_.Exception.Message)"
        }
        else {
            $results.Add([pscustomobject]@{ Step = $Name; Status = 'FAIL'; Seconds = [math]::Round($elapsed.TotalSeconds, 1); Note = $_.Exception.Message })
            Write-Host "FAIL: $Name — $($_.Exception.Message)"
            throw
        }
    }
}

Write-Host 'HydroComplete E2E suite'
Write-Host "Root: $Root"
Write-Host "OCS:  $Ocs"

if (-not $SkipRust) {
    Invoke-E2EStep 'Rust workspace tests' {
        Push-Location $Root
        try {
            & cargo test --workspace --all-features 2>&1 | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
        }
        finally { Pop-Location }
    }
}

if (-not $SkipBuild) {
    Invoke-E2EStep 'Build + install dev OCS plugin' {
        & (Join-Path $PSScriptRoot 'install_dev_plugin.ps1') -Root $Root
    }
}

if (-not $SkipOcs) {
    if (-not (Test-Path $Ocs)) { throw "OCS executable not found: $Ocs" }

    Invoke-E2EStep 'OCS LandXML smoke test' {
        & (Join-Path $PSScriptRoot 'ocs_smoke_test.ps1')
    }

    Invoke-E2EStep 'OCS v0.4.2 integration (pipes, PDF, LandXML)' {
        & (Join-Path $PSScriptRoot 'test_v042.ps1')
    }

    Invoke-E2EStep 'OCS slope + report regression' {
        & (Join-Path $PSScriptRoot 'test_slope_report.ps1')
    }

    Invoke-E2EStep 'OCS Charlotte demo build' {
        & (Join-Path $PSScriptRoot 'build_hydro_demo.ps1')
    }

    Invoke-E2EStep 'OCS manual pipe demo build' {
        & (Join-Path $PSScriptRoot 'build_manual_pipe_demo.ps1')
    }
}

if (-not $Skip24145) {
    $dwg = 'C:\Users\michael.flynn\Downloads\24-145 X-DRAINAGE.dwg'
    if (-not (Test-Path $dwg)) {
        Invoke-E2EStep '24-145 full workflow' { throw 'DWG missing' } -Optional
    }
    else {
        Invoke-E2EStep '24-145 Civil import + hydrology + report/PDF' {
            & (Join-Path $PSScriptRoot 'run_24145_full_workflow.ps1') -Root $Root
        }
    }
}

if (-not $SkipFrontend) {
    Invoke-E2EStep 'Playwright front-end tests (KaTeX + DAG)' {
        & (Join-Path $PSScriptRoot 'run_frontend_tests.ps1') -Root $Root -SkipFixture
    }
}

if (-not $SkipCivil3d -and (Test-Path (Join-Path $Civil3dRoot 'HydroComplete.Civil3D.sln'))) {
    Invoke-E2EStep 'Civil 3D dotnet tests + manifest' {
        & (Join-Path $Civil3dRoot 'scripts\ci.ps1') -Configuration Release
    }

    if (-not $SkipCivil3dGui) {
        $acadExe = 'C:\Program Files\Autodesk\AutoCAD 2026\acad.exe'
        if (-not (Test-Path $acadExe)) {
            Write-Host 'NOTE: Civil 3D GUI smoke needs AutoCAD 2026 (self-hosted runner label: civil3d). GitHub-hosted runners skip this step.'
            Invoke-E2EStep 'Civil 3D GUI parity smoke (COM)' { throw 'AutoCAD 2026 not installed' } -Optional
        }
        else {
            Invoke-E2EStep 'Install Civil 3D bundle for GUI smoke' {
                & (Join-Path $Civil3dRoot 'install.ps1') -Configuration Release
            }
            Invoke-E2EStep 'Civil 3D GUI parity smoke (COM)' {
                $smokeArgs = @{}
                if ($KeepExistingAcad) { $smokeArgs['KeepExistingAcad'] = $true }
                & (Join-Path $Civil3dRoot 'scripts\smoke-civil3d-parity.ps1') @smokeArgs
            }
        }
    }
}
elseif (-not $SkipCivil3d) {
    Invoke-E2EStep 'Civil 3D dotnet tests' { throw "Solution not found at $Civil3dRoot" } -Optional
}

$total = (Get-Date) - $started
Write-Host ''
Write-Host ('=' * 72)
Write-Host 'E2E SUMMARY'
Write-Host ('=' * 72)
$results | Format-Table Step, Status, Seconds, Note -AutoSize
$passed = @($results | Where-Object Status -eq 'PASS').Count
$skipped = @($results | Where-Object Status -eq 'SKIP').Count
Write-Host "Passed: $passed  Skipped: $skipped  Total time: $([math]::Round($total.TotalMinutes, 1)) min"
Write-Host 'E2E SUITE PASSED'