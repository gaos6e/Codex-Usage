# -*- coding: utf-8 -*-
[CmdletBinding()]
param(
  [string]$Executable = (Join-Path $env:LOCALAPPDATA 'Chronolume\chronolume.exe'),
  [string]$OutputPath = (Join-Path $PSScriptRoot '..\docs\images\chronolume-dashboard.png')
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
  throw "Installed executable is missing: $Executable"
}
if (@(Get-Process -Name 'chronolume' -ErrorAction SilentlyContinue).Count -gt 0) {
  throw 'Close existing Chronolume processes before capturing the dashboard.'
}

Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class WindowShowNative
{
    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr handle, int command);
}
'@

$process = Start-Process -FilePath $Executable -PassThru
try {
  $deadline = [DateTime]::UtcNow.AddSeconds(15)
  do {
    Start-Sleep -Milliseconds 50
    $process.Refresh()
    if ($process.HasExited) {
      throw 'Chronolume exited before the dashboard opened.'
    }
  } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)

  if ($process.MainWindowHandle -eq 0) {
    throw 'Chronolume did not open a window within 15 seconds.'
  }

  [WindowShowNative]::ShowWindow($process.MainWindowHandle, 3) | Out-Null
  Start-Sleep -Milliseconds 750
  & (Join-Path $PSScriptRoot 'capture-window.ps1') -ProcessId $process.Id -OutputPath $OutputPath
} finally {
  if (-not $process.HasExited) {
    $process.CloseMainWindow() | Out-Null
    if (-not $process.WaitForExit(3000)) {
      Stop-Process -Id $process.Id -Force
      Wait-Process -Id $process.Id -ErrorAction SilentlyContinue
    }
  }
}
