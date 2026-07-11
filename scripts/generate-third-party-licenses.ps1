# -*- coding: utf-8 -*-
[CmdletBinding()]
param(
  [string[]]$CargoTarget,
  [string]$OutputPath
)

$ErrorActionPreference = 'Stop'

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$cargoManifest = Join-Path $repoRoot 'src-tauri/Cargo.toml'
$packageLockPath = Join-Path $repoRoot 'package-lock.json'
if (-not $OutputPath) {
  $OutputPath = Join-Path $repoRoot 'THIRD_PARTY_LICENSES.txt'
}
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)

if (-not $CargoTarget -or $CargoTarget.Count -eq 0) {
  $hostLine = rustc -vV | Where-Object { $_ -like 'host:*' } | Select-Object -First 1
  if (-not $hostLine) {
    throw 'Unable to determine the Rust host target.'
  }
  $CargoTarget = @($hostLine.Substring(5).Trim())
}
$CargoTarget = @($CargoTarget | Sort-Object -Unique)

function Get-LicenseFiles {
  param(
    [Parameter(Mandatory)][string]$PackageDirectory,
    [string]$ExplicitLicenseFile
  )

  $paths = @()
  if ($ExplicitLicenseFile -and (Test-Path -LiteralPath $ExplicitLicenseFile -PathType Leaf)) {
    $paths += [System.IO.Path]::GetFullPath($ExplicitLicenseFile)
  }
  if (Test-Path -LiteralPath $PackageDirectory -PathType Container) {
    $paths += @(Get-ChildItem -LiteralPath $PackageDirectory -File | Where-Object {
      $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)([._-].*)?$'
    } | ForEach-Object { $_.FullName })
  }
  @($paths | Sort-Object -Unique)
}

$records = @()
$packageLock = Get-Content -LiteralPath $packageLockPath -Raw -Encoding utf8 | ConvertFrom-Json -AsHashtable
foreach ($entry in $packageLock.packages.GetEnumerator()) {
  if ($entry.Key -notlike 'node_modules/*' -or $entry.Value.dev) {
    continue
  }
  $packageDirectory = Join-Path $repoRoot $entry.Key
  $manifestPath = Join-Path $packageDirectory 'package.json'
  if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Installed npm package is missing; run npm ci first: $manifestPath"
  }
  $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding utf8 | ConvertFrom-Json
  $records += [pscustomobject]@{
    Ecosystem = 'npm'
    Name = [string]$manifest.name
    Version = [string]$entry.Value.version
    License = [string]$entry.Value.license
    Directory = $packageDirectory
    LicenseFiles = @(Get-LicenseFiles -PackageDirectory $packageDirectory)
  }
}

$cargoPackages = @{}
foreach ($target in $CargoTarget) {
  $metadataText = cargo metadata --manifest-path $cargoManifest --format-version 1 --locked --filter-platform $target
  if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed for target $target"
  }
  $metadata = $metadataText | ConvertFrom-Json
  $resolvedIds = @{}
  foreach ($node in $metadata.resolve.nodes) {
    $resolvedIds[$node.id] = $true
  }
  foreach ($package in $metadata.packages) {
    if (-not $package.source -or -not $resolvedIds.ContainsKey($package.id)) {
      continue
    }
    $key = "$($package.name)@$($package.version)"
    $cargoPackages[$key] = $package
  }
}

foreach ($key in @($cargoPackages.Keys | Sort-Object)) {
  $package = $cargoPackages[$key]
  $packageDirectory = Split-Path -Parent $package.manifest_path
  $records += [pscustomobject]@{
    Ecosystem = 'cargo'
    Name = [string]$package.name
    Version = [string]$package.version
    License = [string]$package.license
    Directory = $packageDirectory
    LicenseFiles = @(Get-LicenseFiles -PackageDirectory $packageDirectory -ExplicitLicenseFile $package.license_file)
  }
}

$records = @($records | Sort-Object Ecosystem, Name, Version -Unique)
$missingMetadata = @($records | Where-Object { -not $_.License })
if ($missingMetadata.Count -gt 0) {
  throw "Dependencies without license metadata: $($missingMetadata.Name -join ', ')"
}

$licenseTexts = @{}
$packagesWithoutText = @()
foreach ($record in $records) {
  if ($record.LicenseFiles.Count -eq 0) {
    $packagesWithoutText += $record
    continue
  }
  foreach ($licenseFile in $record.LicenseFiles) {
    $content = Get-Content -LiteralPath $licenseFile -Raw -Encoding utf8
    $content = ($content -replace "`r`n", "`n").Trim()
    if (-not $content) {
      continue
    }
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($content)
    $hash = [System.Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($bytes))
    if (-not $licenseTexts.ContainsKey($hash)) {
      $licenseTexts[$hash] = [pscustomobject]@{ Content = $content; Packages = @(); Files = @() }
    }
    $licenseTexts[$hash].Packages += "$($record.Ecosystem):$($record.Name)@$($record.Version)"
    $licenseTexts[$hash].Files += [System.IO.Path]::GetFileName($licenseFile)
  }
}

$builder = [System.Text.StringBuilder]::new()
[void]$builder.AppendLine('CHRONOLUME THIRD-PARTY LICENSES')
[void]$builder.AppendLine('================================')
[void]$builder.AppendLine()
[void]$builder.AppendLine('This file is generated from package-lock.json, Cargo.lock, installed package metadata,')
[void]$builder.AppendLine('and license files shipped in the resolved package archives. Do not edit it by hand.')
[void]$builder.AppendLine("Cargo target audit: $($CargoTarget -join ', ')")
[void]$builder.AppendLine("npm production packages: $(@($records | Where-Object Ecosystem -eq 'npm').Count)")
[void]$builder.AppendLine("Cargo resolved packages: $(@($records | Where-Object Ecosystem -eq 'cargo').Count)")
[void]$builder.AppendLine("package-lock.json SHA-256: $((Get-FileHash -LiteralPath $packageLockPath -Algorithm SHA256).Hash)")
[void]$builder.AppendLine("Cargo.lock SHA-256: $((Get-FileHash -LiteralPath (Join-Path $repoRoot 'src-tauri/Cargo.lock') -Algorithm SHA256).Hash)")
[void]$builder.AppendLine()
[void]$builder.AppendLine('PACKAGE INVENTORY')
[void]$builder.AppendLine('-----------------')
foreach ($record in $records) {
  [void]$builder.AppendLine("$($record.Ecosystem):$($record.Name)@$($record.Version) | $($record.License)")
}

if ($packagesWithoutText.Count -gt 0) {
  [void]$builder.AppendLine()
  [void]$builder.AppendLine('PACKAGES WHOSE ARCHIVES DO NOT CONTAIN A TOP-LEVEL LICENSE FILE')
  [void]$builder.AppendLine('--------------------------------------------------------------')
  [void]$builder.AppendLine('Their declared SPDX expressions remain recorded in the inventory above.')
  foreach ($record in $packagesWithoutText) {
    [void]$builder.AppendLine("$($record.Ecosystem):$($record.Name)@$($record.Version) | $($record.License)")
  }
}

[void]$builder.AppendLine()
[void]$builder.AppendLine('VERBATIM LICENSE AND NOTICE TEXTS')
[void]$builder.AppendLine('---------------------------------')
foreach ($hash in @($licenseTexts.Keys | Sort-Object)) {
  $entry = $licenseTexts[$hash]
  [void]$builder.AppendLine()
  [void]$builder.AppendLine("----- SHA-256 $hash -----")
  [void]$builder.AppendLine("Packages: $((@($entry.Packages | Sort-Object -Unique)) -join ', ')")
  [void]$builder.AppendLine("Source files: $((@($entry.Files | Sort-Object -Unique)) -join ', ')")
  [void]$builder.AppendLine()
  [void]$builder.AppendLine($entry.Content)
}

$parent = Split-Path -Parent $OutputPath
if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
  New-Item -ItemType Directory -Path $parent -Force | Out-Null
}
[System.IO.File]::WriteAllText($OutputPath, $builder.ToString(), [System.Text.UTF8Encoding]::new($false))

[pscustomobject]@{
  Path = $OutputPath
  Bytes = (Get-Item -LiteralPath $OutputPath).Length
  Packages = $records.Count
  UniqueLicenseTexts = $licenseTexts.Count
  PackagesWithoutTopLevelText = $packagesWithoutText.Count
  Sha256 = (Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256).Hash
} | ConvertTo-Json
