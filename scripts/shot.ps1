# Screenshots a C3Engine app window (works even behind other windows, via PrintWindow).
# Usage: powershell -File scripts/shot.ps1 -App client -Out shot.png -Wait 8 -- --map-id 1015 --cell 326,310
param(
    [ValidateSet("client", "studio")] [string]$App = "client",
    [Parameter(Mandatory)] [string]$Out,
    [int]$Wait = 8,
    # More than one shot: Out gets -1, -2... before its extension, Every ms apart.
    [int]$Burst = 1,
    [int]$Every = 100,
    [Parameter(ValueFromRemainingArguments)] [string[]]$AppArgs
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public class PW {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    public struct RECT { public int L, T, R, B; }
}
'@
$exe = "target\release\c3-$App.exe"
$p = if ($AppArgs) { Start-Process $exe -ArgumentList $AppArgs -PassThru } else { Start-Process $exe -PassThru }
try {
    Start-Sleep $Wait
    $p.Refresh()
    $h = $p.MainWindowHandle
    $r = New-Object PW+RECT
    [PW]::GetWindowRect($h, [ref]$r) | Out-Null
    for ($i = 1; $i -le $Burst; $i++) {
        $b = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
        $g = [System.Drawing.Graphics]::FromImage($b)
        $dc = $g.GetHdc()
        # 2 = PW_RENDERFULLCONTENT (captures GPU-rendered content).
        [PW]::PrintWindow($h, $dc, 2) | Out-Null
        $g.ReleaseHdc($dc)
        $file = if ($Burst -gt 1) { [IO.Path]::ChangeExtension($Out, $null).TrimEnd('.') + "-$i" + [IO.Path]::GetExtension($Out) } else { $Out }
        $b.Save($file)
        $g.Dispose(); $b.Dispose()
        if ($i -lt $Burst) { Start-Sleep -Milliseconds $Every }
    }
} finally {
    if (-not $p.HasExited) { $p.Kill() }
}
