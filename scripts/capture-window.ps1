param(
    [Parameter(Mandatory = $true)]
    [int]$ProcessId,

    [Parameter(Mandatory = $true)]
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class WindowCaptureNative
{
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr handle, out Rect rect);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr handle, IntPtr deviceContext, uint flags);
}
'@

$process = Get-Process -Id $ProcessId -ErrorAction Stop
$process.Refresh()
$handle = $process.MainWindowHandle
if ($handle -eq [IntPtr]::Zero) {
    throw "Process $ProcessId does not have a main window"
}

$rect = New-Object WindowCaptureNative+Rect
if (-not [WindowCaptureNative]::GetWindowRect($handle, [ref]$rect)) {
    throw 'GetWindowRect failed'
}

$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) {
    throw "Invalid window bounds: ${width}x${height}"
}

$output = [System.IO.Path]::GetFullPath($OutputPath)
$parent = Split-Path -Parent $output
if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
    throw "Output directory does not exist: $parent"
}

$bitmap = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
try {
    $deviceContext = $graphics.GetHdc()
    try {
        if (-not [WindowCaptureNative]::PrintWindow($handle, $deviceContext, 2)) {
            throw 'PrintWindow failed'
        }
    }
    finally {
        $graphics.ReleaseHdc($deviceContext)
    }
    $bitmap.Save($output, [System.Drawing.Imaging.ImageFormat]::Png)
}
finally {
    $graphics.Dispose()
    $bitmap.Dispose()
}

Get-Item -LiteralPath $output | Select-Object FullName, Length, LastWriteTime
