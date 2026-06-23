# Install a debug plugin DLL for local OCS automation (HYDROCOMPLETE_PRO bypass is debug-only).
param(
    [string]$Root = (Split-Path $PSScriptRoot -Parent)
)
$ErrorActionPreference = "Stop"
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"

Write-Host "Building debug plugin (dev Pro bypass)..."
Push-Location $Root
$prevEap = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
& cargo build -p opencad-hydrocomplete-plugin 2>&1 | Out-Host
$buildExit = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } else { 1 }
$ErrorActionPreference = $prevEap
if ($buildExit -ne 0) { throw "cargo build failed" }
Pop-Location

New-Item -ItemType Directory -Force -Path $PluginDir | Out-Null
Copy-Item -Force (Join-Path $Root "target\debug\opencad_hydrocomplete_plugin.dll") `
    (Join-Path $PluginDir "opencad.hydrocomplete-windows-x86_64.dll")
Copy-Item -Force (Join-Path $Root "plugin.toml") (Join-Path $PluginDir "plugin.toml")
Write-Host "Installed debug plugin DLL + plugin.toml"