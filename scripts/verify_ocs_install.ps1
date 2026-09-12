# Verify HydroComplete is installed and loads in OCS --serve.
$ErrorActionPreference = "Stop"
$Ocs = if ($env:HC_OCS_EXE) { $env:HC_OCS_EXE } else { Join-Path $env:USERPROFILE "Downloads\OpenCADStudio-v2026.36-windows-x86_64-portable.exe" }
# PowerShell 5.1 prefixes a UTF-8 BOM when piping to a native exe, so OCS --serve
# rejected the first request ({"op":"new"}) as invalid JSON. Pipe without a BOM.
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding $false
$OutputEncoding = New-Object System.Text.UTF8Encoding $false
$PluginDir = Join-Path $env:APPDATA "OpenCADStudio\plugins\opencad.hydrocomplete"
$Dwg = "C:/Users/michael.flynn/Downloads/24-145 X-DRAINAGE.dwg"

if (-not (Test-Path $Ocs)) { throw "OCS not found: $Ocs" }
$toml = Join-Path $PluginDir "plugin.toml"
$dll = Join-Path $PluginDir "opencad.hydrocomplete-windows-x86_64.dll"
if (-not (Test-Path $toml)) { throw "plugin.toml missing" }
if (-not (Test-Path $dll)) { throw "plugin DLL missing" }

$ver = "unknown"
if ((Get-Content $toml -Raw) -match 'version = "([^"]+)"') { $ver = $Matches[1] }
Write-Host "Installed: HydroComplete v$ver ($((Get-Item $dll).Length) bytes)"

$lines = [System.Collections.Generic.List[string]]::new()
@(
    '{"op":"open","path":"' + $Dwg + '"}'
    '{"op":"run","cmd":"HC_CIVIL_IMPORT force"}'
    '{"op":"run","cmd":"HC_NETWORK"}'
) | & $Ocs --serve 2>&1 | ForEach-Object { $lines.Add([string]$_) }

foreach ($line in $lines) {
    if ($line -match '"ok":false') { throw "OCS step failed: $line" }
}
$import = $lines | Where-Object { $_ -match 'HC_CIVIL_IMPORT' } | Select-Object -Last 1
$network = $lines | Where-Object { $_ -match 'HC_NETWORK' } | Select-Object -Last 1
Write-Host $import
Write-Host $network

$log = Join-Path $env:APPDATA "HydroComplete\civil-import-last.txt"
if (Test-Path $log) { Write-Host (Get-Content $log -Raw).Trim() }

Write-Host "OCS VERIFY OK - restart GUI and open Plugin Manager to confirm v$ver"