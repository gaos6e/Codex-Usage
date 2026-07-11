# -*- coding: utf-8 -*-
[CmdletBinding()]
param(
  [string]$InstallerPath,
  [string]$PortableArchive,
  [string]$ReportPath
)

$ErrorActionPreference = 'Stop'

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'src-tauri\tauri.conf.json') | ConvertFrom-Json
$version = $tauriConfig.version
if (-not $InstallerPath) {
  $InstallerPath = Join-Path $repoRoot "src-tauri\target\release\bundle\nsis\Chronolume_${version}_x64-setup.exe"
}
if (-not $PortableArchive) {
  $PortableArchive = Join-Path $repoRoot "src-tauri\target\release\bundle\portable\Chronolume-$version-windows-x64-portable.zip"
}

$InstallerPath = (Resolve-Path -LiteralPath $InstallerPath).Path
$PortableArchive = (Resolve-Path -LiteralPath $PortableArchive).Path
$portableRoot = [System.IO.Path]::GetFullPath((Split-Path -Parent $PortableArchive))
$smokeDir = [System.IO.Path]::GetFullPath((Join-Path $portableRoot 'smoke-expanded'))
$portablePrefix = $portableRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $smokeDir.StartsWith($portablePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to use smoke directory outside portable output: $smokeDir"
}

$existing = @(Get-Process -Name 'chronolume' -ErrorAction SilentlyContinue)
if ($existing.Count -gt 0) {
  throw 'Close existing Chronolume processes before running distribution smoke tests.'
}

function Test-AppWindow {
  param([Parameter(Mandatory)][string]$Executable)

  $watch = [System.Diagnostics.Stopwatch]::StartNew()
  $process = Start-Process -FilePath $Executable -PassThru
  try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
      Start-Sleep -Milliseconds 50
      $process.Refresh()
      if ($process.HasExited) {
        throw "Application exited before opening a window: $Executable"
      }
    } while ($process.MainWindowHandle -eq 0 -and [DateTime]::UtcNow -lt $deadline)

    if ($process.MainWindowHandle -eq 0) {
      throw "Application did not open a window within 15 seconds: $Executable"
    }

    $watch.Stop()
    [pscustomobject]@{
      Executable = $Executable
      WindowReadyMs = [Math]::Round($watch.Elapsed.TotalMilliseconds, 2)
      WorkingSetBytes = $process.WorkingSet64
      PrivateBytes = $process.PrivateMemorySize64
    }
  } finally {
    if (-not $process.HasExited) {
      $process.CloseMainWindow() | Out-Null
      if (-not $process.WaitForExit(3000)) {
        Stop-Process -Id $process.Id -Force
        Wait-Process -Id $process.Id -ErrorAction SilentlyContinue
      }
    }
  }
}

function Remove-SmokeDirectory {
  if (-not (Test-Path -LiteralPath $smokeDir)) {
    return
  }

  $resolvedSmoke = [System.IO.Path]::GetFullPath($smokeDir)
  if (-not $resolvedSmoke.StartsWith($portablePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove smoke directory outside portable output: $resolvedSmoke"
  }

  for ($attempt = 1; $attempt -le 30; $attempt++) {
    try {
      Remove-Item -LiteralPath $resolvedSmoke -Recurse -Force
      return
    } catch {
      if ($attempt -eq 30) {
        throw
      }
      Start-Sleep -Milliseconds 100
    }
  }
}

$installer = Start-Process -FilePath $InstallerPath -ArgumentList '/S' -PassThru -Wait
if ($installer.ExitCode -ne 0) {
  throw "NSIS installer exited with code $($installer.ExitCode)."
}

$installedExe = Join-Path $env:LOCALAPPDATA 'Chronolume\chronolume.exe'
if (-not (Test-Path -LiteralPath $installedExe -PathType Leaf)) {
  throw "Installed executable is missing: $installedExe"
}
$installedResult = Test-AppWindow -Executable $installedExe

Remove-SmokeDirectory
New-Item -ItemType Directory -Path $smokeDir | Out-Null

try {
  Expand-Archive -LiteralPath $PortableArchive -DestinationPath $smokeDir
  foreach ($name in @('chronolume.exe', 'README.md', 'LICENSE', 'THIRD_PARTY_LICENSES.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $smokeDir $name) -PathType Leaf)) {
      throw "Portable archive entry is missing: $name"
    }
  }
  $portableResult = Test-AppWindow -Executable (Join-Path $smokeDir 'chronolume.exe')
} finally {
  Remove-SmokeDirectory
}

$result = [pscustomobject]@{
  InstallerExitCode = $installer.ExitCode
  Installed = $installedResult
  Portable = $portableResult
}
$json = $result | ConvertTo-Json -Depth 4
if ($ReportPath) {
  $ReportPath = [System.IO.Path]::GetFullPath($ReportPath)
  $reportParent = Split-Path -Parent $ReportPath
  if (-not (Test-Path -LiteralPath $reportParent -PathType Container)) {
    New-Item -ItemType Directory -Path $reportParent -Force | Out-Null
  }
  [System.IO.File]::WriteAllText($ReportPath, $json, [System.Text.UTF8Encoding]::new($false))
}
$json
