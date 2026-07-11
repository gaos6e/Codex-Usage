# -*- coding: utf-8 -*-
[CmdletBinding()]
param(
  [string]$OutputPath,
  [string]$SmokeReportPath
)

$ErrorActionPreference = 'Stop'

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$defaultOutput = [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'artifacts\windows'))
if (-not $OutputPath) {
  $OutputPath = $defaultOutput
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
if (-not $SmokeReportPath) {
  $SmokeReportPath = Join-Path $repoRoot 'artifacts\windows-smoke.json'
}
$SmokeReportPath = [System.IO.Path]::GetFullPath($SmokeReportPath)

# This helper deletes and recreates its staging directory, so keep that mutation inside the
# repository's dedicated artifacts root even when a caller passes an explicit path.
$artifactsRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'artifacts'))
$artifactsPrefix = $artifactsRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $OutputPath.StartsWith($artifactsPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "Refusing to stage outside the repository artifacts directory: $OutputPath"
}

$tauriConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\tauri.conf.json') -Raw -Encoding utf8 | ConvertFrom-Json
$version = [string]$tauriConfig.version
$releaseRoot = Join-Path $repoRoot 'src-tauri\target\release'
$sources = @(
  (Join-Path $releaseRoot 'chronolume.exe'),
  (Join-Path $releaseRoot "bundle\nsis\Chronolume_${version}_x64-setup.exe"),
  (Join-Path $releaseRoot "bundle\portable\Chronolume-$version-windows-x64-portable.zip")
)
foreach ($source in $sources) {
  if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw "Required Windows release artifact is missing: $source"
  }
}

if (-not (Test-Path -LiteralPath $SmokeReportPath -PathType Leaf)) {
  throw "Windows smoke report is missing: $SmokeReportPath"
}
$smoke = Get-Content -LiteralPath $SmokeReportPath -Raw -Encoding utf8 | ConvertFrom-Json
if ($smoke.InstallerExitCode -ne 0 -or $smoke.Installed.WindowReadyMs -le 0 -or $smoke.Portable.WindowReadyMs -le 0) {
  throw 'Windows smoke report does not prove successful installed and portable launches.'
}
$expectedInstalledExecutables = @('chronolume.exe', 'uninstall.exe') | Sort-Object
$actualInstalledExecutables = @($smoke.InstalledBundleExecutables | Sort-Object)
if (($actualInstalledExecutables -join "`n") -ne ($expectedInstalledExecutables -join "`n")) {
  throw "Windows smoke report contains an unexpected installed executable set: $($actualInstalledExecutables -join ', ')"
}

$portableEntries = @()
$archive = [System.IO.Compression.ZipFile]::OpenRead($sources[2])
try {
  $portableEntries = @($archive.Entries | Where-Object { $_.FullName -and -not $_.FullName.EndsWith('/') } | ForEach-Object { $_.FullName.Replace('\', '/') } | Sort-Object)
} finally {
  $archive.Dispose()
}
$expectedEntries = @('LICENSE', 'README.md', 'THIRD_PARTY_LICENSES.txt', 'chronolume.exe') | Sort-Object
if (($portableEntries -join "`n") -ne ($expectedEntries -join "`n")) {
  throw "Portable archive entries differ from the allowed release set: $($portableEntries -join ', ')"
}

if (Test-Path -LiteralPath $OutputPath) {
  Remove-Item -LiteralPath $OutputPath -Recurse -Force
}
New-Item -ItemType Directory -Path $OutputPath -Force | Out-Null
foreach ($source in $sources) {
  Copy-Item -LiteralPath $source -Destination $OutputPath
}
Copy-Item -LiteralPath $SmokeReportPath -Destination (Join-Path $OutputPath 'smoke-windows.json')

$hashLines = @()
foreach ($source in $sources) {
  $name = [System.IO.Path]::GetFileName($source)
  $staged = Join-Path $OutputPath $name
  $hash = Get-FileHash -LiteralPath $staged -Algorithm SHA256
  $hashLines += "$($hash.Hash)  $name"
}
[System.IO.File]::WriteAllLines(
  (Join-Path $OutputPath 'SHA256SUMS-windows.txt'),
  $hashLines,
  [System.Text.UTF8Encoding]::new($false)
)
[System.IO.File]::WriteAllText(
  (Join-Path $OutputPath 'verification-windows.txt'),
  "Version: $version`nNSIS build: passed`nNSIS silent install smoke: passed`nInstalled executable audit: passed`nInstalled window smoke: passed ($($smoke.Installed.WindowReadyMs) ms)`nPortable archive entry audit: passed`nPortable window smoke: passed ($($smoke.Portable.WindowReadyMs) ms)`n",
  [System.Text.UTF8Encoding]::new($false)
)

[pscustomobject]@{
  Path = $OutputPath
  Version = $version
  Files = @(Get-ChildItem -LiteralPath $OutputPath -File | Select-Object -ExpandProperty Name)
} | ConvertTo-Json -Depth 3
