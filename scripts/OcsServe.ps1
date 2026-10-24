# Shared runner for `OpenCADStudio --serve` automation. Dot-source it:
#     . (Join-Path $PSScriptRoot 'OcsServe.ps1')
#     $lines = Invoke-OcsBatch $Ocs @($requests)
#
# Why not `@($requests) | & $Ocs --serve 2>&1`: once a plugin loads, OCS starts
# an out-of-process plugin runner, and on GitHub's windows-latest (pwsh 7) that
# pipeline never finished: the nightly sat 22 min in its first call (run
# 40103256297) while the same requests through Process.Start returned in 0.7 s
# (run 40112300490). This helper uses Process.Start, writes the requests as
# UTF-8 without a BOM (PowerShell 5.1 added one, and OCS rejected the first
# request as invalid JSON), and kills the process tree on a timeout so a hang
# fails in minutes with a message.
#
# Returns stderr lines first, then stdout lines, so the last line is still the
# last response (callers read it as the final state).

function Invoke-OcsBatch {
    param(
        [Parameter(Mandatory)] [string] $Ocs,
        [Parameter(Mandatory)] [string[]] $Requests,
        [int] $TimeoutSec = 300
    )
    $psi = New-Object System.Diagnostics.ProcessStartInfo $Ocs, '--serve'
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $p = [System.Diagnostics.Process]::Start($psi)
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    $text = ($Requests -join "`n") + "`n"
    $bytes = (New-Object System.Text.UTF8Encoding $false).GetBytes($text)
    $p.StandardInput.BaseStream.Write($bytes, 0, $bytes.Length)
    $p.StandardInput.BaseStream.Flush()
    $p.StandardInput.Close()
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        # The plugin runner is a child of the same exe; take it down too.
        & taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null
        throw "OpenCADStudio --serve did not exit within $TimeoutSec s"
    }
    $p.WaitForExit()
    [void]$out.Wait(10000)
    [void]$err.Wait(10000)
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($s in @($err.Result, $out.Result)) {
        if ($s) { foreach ($l in ($s -split "`r?`n")) { if ($l -ne '') { $lines.Add($l) } } }
    }
    # Unrolled on purpose: callers pipe this into ForEach-Object line by line.
    return $lines.ToArray()
}
