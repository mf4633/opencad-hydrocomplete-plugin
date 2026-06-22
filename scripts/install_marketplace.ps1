# Install HydroComplete via OCS marketplace flow (registry + GitHub Release).
param(
    [string]$Repo = "mf4633/opencad-hydrocomplete-plugin",
    [string]$Tag = "v0.4.5"
)
$ErrorActionPreference = "Stop"

$RegistryUrl = "https://raw.githubusercontent.com/HakanSeven12/OpenCADStudio/main/plugins/registry.json"
$PluginsRoot = Join-Path $env:APPDATA "OpenCADStudio\plugins"
$LibName = "opencad.hydrocomplete-windows-x86_64.dll"
$PluginId = "opencad.hydrocomplete"

Write-Host "=== OCS Marketplace install: HydroComplete ==="

$registry = Invoke-RestMethod -Uri $RegistryUrl
$entry = $registry | Where-Object { $_.repo -eq $Repo }
if (-not $entry) {
    throw "HydroComplete not in curated registry. Check https://github.com/HakanSeven12/OpenCADStudio/blob/main/plugins/registry.json"
}
Write-Host ('Registry: {0}' -f $entry.name)
Write-Host ('  {0}' -f $entry.description)

$staging = Join-Path $env:TEMP "hc-marketplace-$Tag"
if (Test-Path $staging) { Remove-Item $staging -Recurse -Force }
New-Item -ItemType Directory -Path $staging | Out-Null
gh release download $Tag --repo $Repo --dir $staging 2>&1 | Out-Host
if ($LASTEXITCODE -ne 0) { throw "gh release download failed" }

$tomlPath = Join-Path $staging "plugin.toml"
$libPath = Join-Path $staging $LibName
if (-not (Test-Path $tomlPath)) { throw "plugin.toml missing in release" }
if (-not (Test-Path $libPath)) { throw "$LibName missing in release" }

$tomlText = Get-Content $tomlPath -Raw
if ($tomlText -notlike "*$PluginId*") { throw "plugin.toml id mismatch" }
$version = "unknown"
if ($tomlText -match 'version = "([^"]+)"') { $version = $Matches[1] }
Write-Host ('Release {0} -> plugin version {1}' -f $Tag, $version)

$installDir = Join-Path $PluginsRoot $PluginId
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Get-ChildItem $installDir -Filter "*.dll" -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -ne $LibName } |
    ForEach-Object { Remove-Item $_.FullName -Force }

Copy-Item -Force $libPath (Join-Path $installDir $LibName)
Set-Content -Path (Join-Path $installDir "plugin.toml") -Value $tomlText -Encoding UTF8 -NoNewline

$bytes = (Get-Item (Join-Path $installDir $LibName)).Length
Write-Host ('Installed: {0}' -f $installDir)
Write-Host ('  {0} ({1} bytes)' -f $LibName, $bytes)
Write-Host ""
Write-Host "Restart OCS. Plugin Manager -> marketplace -> HydroComplete lists v0.4.3."
Write-Host "MARKETPLACE INSTALL OK"