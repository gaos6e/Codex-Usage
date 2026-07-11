# -*- coding: utf-8 -*-
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$tauriConfig = Get-Content -Raw -LiteralPath (Join-Path $repoRoot 'src-tauri\tauri.conf.json') | ConvertFrom-Json
$version = $tauriConfig.version
$releaseExe = Join-Path $repoRoot 'src-tauri\target\release\chronolume.exe'
$portableDir = Join-Path $repoRoot 'src-tauri\target\release\bundle\portable'
$archivePath = Join-Path $portableDir "Chronolume-$version-windows-x64-portable.zip"
$readme = Join-Path $repoRoot 'README.md'
$license = Join-Path $repoRoot 'LICENSE'
$thirdPartyLicenses = Join-Path $repoRoot 'THIRD_PARTY_LICENSES.txt'

& (Join-Path $PSScriptRoot 'generate-third-party-licenses.ps1') -OutputPath $thirdPartyLicenses | Out-Null
if ($LASTEXITCODE -ne 0) {
  throw 'Third-party license generation failed.'
}

foreach ($path in @($releaseExe, $readme, $license, $thirdPartyLicenses)) {
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
    throw "Required portable file is missing: $path"
  }
}

New-Item -ItemType Directory -Path $portableDir -Force | Out-Null
if (Test-Path -LiteralPath $archivePath) {
  Remove-Item -LiteralPath $archivePath -Force
}

Compress-Archive -LiteralPath @($releaseExe, $readme, $license, $thirdPartyLicenses) -DestinationPath $archivePath -CompressionLevel Optimal

$archive = Get-Item -LiteralPath $archivePath
$hash = Get-FileHash -LiteralPath $archivePath -Algorithm SHA256
[pscustomobject]@{
  Path = $archive.FullName
  Bytes = $archive.Length
  Sha256 = $hash.Hash
} | ConvertTo-Json
