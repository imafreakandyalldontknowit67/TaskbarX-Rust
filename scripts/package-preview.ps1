#Requires -Version 5.1
<#
.SYNOPSIS
Repackages an explicitly supplied, checksummed local candidate without running it.
.DESCRIPTION
Reads payload inputs only from SourcePackageDirectory. Every source-manifest entry,
including excluded evidence, must validate. Checksums establish consistency, not
signing, licensing clearance, or build provenance. Review assets before supplying
the candidate; icons and additional executable material are not distributed.

Creates dist/TaskbarX-Rust/<timestamp-GUID>/ containing payload/, the two standalone
EXEs, TaskbarX-Rust-v0.1.0-windows-x64.zip, its .sha256, and external SHA256SUMS.txt.
The ZIP has payload files at its root, including its own SHA256SUMS.txt. Failed
outputs are retained. Nothing is built, launched, installed, or published.
.EXAMPLE
powershell.exe -NoProfile -File .\scripts\package-preview.ps1 -SourcePackageDirectory 'C:\Candidates\reviewed [preview]'
.OUTPUTS
An object with OutputDirectory, PackageDirectory, ZipPath, ChecksumsPath,
PayloadChecksumsPath, and ZipChecksumPath. No object is returned on failure.
#>
[CmdletBinding()]
param([Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$SourcePackageDirectory)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildHelpers.psm1') -Force

function ConvertTo-SafeRelativePath {
    param([string]$Path)
    # Canonicalize separators for duplicate detection, but reject Windows aliases,
    # device names, ADS, traversal, and names unsafe to extract on Windows.
    if ([string]::IsNullOrWhiteSpace($Path) -or [IO.Path]::IsPathRooted($Path)) {
        throw "Unsafe relative path: $Path"
    }
    $relative = $Path.Replace('\', '/')
    foreach ($part in $relative.Split('/')) {
        if (-not $part -or $part -in @('.', '..') -or $part -match '[\x00-\x1f\x7f<>:"|?*]' -or
            $part -match '[. ]$' -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(?:\.|$)') {
            throw "Unsafe relative path: $Path"
        }
    }
    return $relative
}

function Assert-NormalAncestors {
    param([string]$Path)
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Reparse points are not permitted: $current"
            }
        }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}

function Get-SourceFile {
    param([string]$Directory, [string]$Relative)
    $safe = ConvertTo-SafeRelativePath $Relative
    $path = [IO.Path]::GetFullPath((Join-Path $Directory $safe))
    if (-not $path.StartsWith($Directory.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Source path escapes its directory: $Relative"
    }
    Assert-NormalAncestors $path
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing source file: $Relative" }
    return $path
}

function Read-SourceManifest {
    param([string]$Directory)
    $path = Get-SourceFile $Directory 'SHA256SUMS.txt'
    $hashes = @{}
    $lineNumber = 0
    foreach ($line in [IO.File]::ReadAllLines($path)) {
        $lineNumber++
        if ($line -notmatch '^([0-9a-fA-F]{64})  (.+)$') { throw "Malformed source manifest entry at line $lineNumber." }
        $hash = $Matches[1].ToLowerInvariant()
        $relative = ConvertTo-SafeRelativePath $Matches[2]
        if ($hashes.ContainsKey($relative)) { throw "Duplicate source manifest entry: $relative" }
        if ($relative -ieq 'SHA256SUMS.txt') { throw 'Source manifest must not list itself.' }
        $hashes[$relative] = $hash
    }
    if ($hashes.Count -eq 0) { throw 'Source manifest is empty.' }
    # Validate the entire candidate, not just the distribution allowlist.
    foreach ($relative in $hashes.Keys) {
        $file = Get-SourceFile $Directory $relative
        if ((Get-Sha256 -Path $file) -ne $hashes[$relative]) { throw "Source checksum mismatch: $relative" }
    }
    return $hashes
}

function Test-ExcludedSupplement {
    param([string]$Relative)
    foreach ($part in $Relative.Split('/')) {
        if ($part -match '^(?i:evidence|logs?|diagnostics?|reference|target|app|build|build[-_]metadata|cargo[-_]metadata|dependency[-_]inventor(?:y|ies)|\.git)(?:[._-]|$)' -or
            $part -match '^(?i:VALIDATION|PARITY|INTEGRATION|TESTING|SHELL)(?:[._-]|$)' -or
            $part -ieq 'settings.json' -or $part -in @('Cargo.toml', 'Cargo.lock')) { return $true }
    }
    # Supplemental documentation is not a vehicle for upstream icons, executables,
    # scripts, credentials, compiled objects, or opaque archives of those files.
    return $Relative -match '(?i)\.(ico|cur|icns|exe|dll|sys|com|scr|msi|msp|msix|msixbundle|appx|appxbundle|ocx|cpl|pdb|obj|lib|a|so|dylib|bin|jar|wasm|ps1|psm1|psd1|cmd|bat|vbs|vbe|js|jse|wsf|wsh|hta|lnk|url|pfx|p12|pem|key|cer|crt|zip|7z|rar|tar|gz|cab|iso|log)$|(?i)\.(stdout|stderr)\.txt$'
}

function Get-SupplementFiles {
    param([string]$Directory, [string]$Relative)
    $path = Join-Path $Directory $Relative
    Assert-NormalAncestors $path
    if (-not (Test-Path -LiteralPath $path)) { return }
    if (-not (Test-Path -LiteralPath $path -PathType Container)) { throw "Expected supplemental directory: $Relative" }
    # Deliberately no recursive copy or traversal of the rest of docs/the candidate.
    foreach ($item in @(Get-ChildItem -LiteralPath $path -Force)) {
        $child = ConvertTo-SafeRelativePath ($Relative + '/' + $item.Name)
        Assert-NormalAncestors $item.FullName
        if (Test-ExcludedSupplement $child) { continue }
        if ($item.PSIsContainer) { Get-SupplementFiles $Directory $child }
        else { $child }
    }
}

function Copy-VerifiedFile {
    param([string]$Source, [string]$Destination, [string]$ExpectedHash)
    Assert-NormalAncestors $Source
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Destination))
    # Fresh ordinary files, without inheriting hidden/reparse attributes. CreateNew
    # refuses overwrites; FileShare.Read prevents writes during this individual copy.
    $inputStream = [IO.File]::Open($Source, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $outputStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose() }
    } finally { $inputStream.Dispose() }
    if ((Get-Sha256 -Path $Destination) -ne $ExpectedHash) { throw "Copied checksum mismatch: $Destination" }
}

function Write-NewTextFile {
    param([string]$Path, [string]$Text)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes($Text)
        $stream.Write($bytes, 0, $bytes.Length)
    } finally { $stream.Dispose() }
}

function Assert-PreviewArchive {
    param([string]$ZipPath, [System.Collections.IDictionary]$Expected)
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($ZipPath)
    try {
        $seen = @{}
        foreach ($entry in $archive.Entries) {
            $relative = ConvertTo-SafeRelativePath $entry.FullName
            if ($relative -cne $entry.FullName) { throw "Unsafe ZIP entry: $($entry.FullName)" }
            $unixType = ($entry.ExternalAttributes -shr 16) -band 0xf000
            if ($unixType -notin @(0, 0x8000) -or ($entry.ExternalAttributes -band 0x410)) {
                throw "Unsafe ZIP entry attributes: $relative"
            }
            if ($seen.ContainsKey($relative)) { throw "Duplicate ZIP entry: $relative" }
            $seen[$relative] = $true
            if ($relative -cnotin @($Expected.Keys)) { throw "Unexpected ZIP entry: $relative" }
            $stream = $entry.Open()
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
            finally { $sha.Dispose(); $stream.Dispose() }
            if ($hash -ne $Expected[$relative]) { throw "ZIP checksum mismatch: $relative" }
        }
        foreach ($relative in $Expected.Keys) {
            if (-not $seen.ContainsKey($relative)) { throw "Missing ZIP entry: $relative" }
        }
    } finally { $archive.Dispose() }
}

function New-PreviewArchive {
    param([string]$Directory, [string]$ZipPath, [string[]]$RelativePaths)
    # Framework ZipFile.CreateFromDirectory (used by New-LocalArchive) can write
    # backslashes on PS 5.1. Create explicit slash-separated entries instead, so
    # the same safe, exact ZIP paths are produced on both supported PS runtimes.
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    if (Test-Path -LiteralPath "$ZipPath.sha256") { throw "Refusing to overwrite archive checksum: $ZipPath.sha256" }
    $stream = [IO.File]::Open($ZipPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $archive = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            foreach ($relative in $RelativePaths) {
                $entry = $archive.CreateEntry($relative)
                $inputStream = [IO.File]::OpenRead((Join-Path $Directory $relative))
                try {
                    $entryStream = $entry.Open()
                    try { $inputStream.CopyTo($entryStream) } finally { $entryStream.Dispose() }
                } finally { $inputStream.Dispose() }
            }
        } finally { $archive.Dispose() }
    } finally { $stream.Dispose() }
    Write-NewTextFile "$ZipPath.sha256" ("{0}  {1}`n" -f (Get-Sha256 -Path $ZipPath), [IO.Path]::GetFileName($ZipPath))
}

$root = Get-RepositoryRoot -ScriptDirectory $PSScriptRoot
$resolved = Resolve-Path -LiteralPath $SourcePackageDirectory
if ($resolved.Provider.Name -ne 'FileSystem') { throw 'Source package must be a filesystem directory.' }
$source = [IO.Path]::GetFullPath($resolved.ProviderPath)
if ($source.Length -gt [IO.Path]::GetPathRoot($source).Length) { $source = $source.TrimEnd('\', '/') }
Assert-NormalAncestors $source
if (-not (Test-Path -LiteralPath $source -PathType Container)) { throw 'Source package must be a directory.' }
$outputParent = Join-Path $root 'dist/TaskbarX-Rust'
Assert-NormalAncestors $outputParent
if ($outputParent -ieq $source -or $outputParent.StartsWith($source.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Output must not be inside the supplied source package.'
}
$sourceHashes = Read-SourceManifest $source
$required = @('taskbarx.exe', 'taskbarx-configurator.exe', 'README.md', 'LICENSE',
    'THIRD_PARTY_NOTICES.md', 'settings.sample.json', 'docs/USER-GUIDE.md', 'docs/settings.sample.json')
$selected = [ordered]@{}
foreach ($relative in $required + @(Get-SupplementFiles $source 'docs/assets') + @(Get-SupplementFiles $source 'docs/licenses')) {
    $null = Get-SourceFile $source $relative
    if (-not $sourceHashes.ContainsKey($relative)) { throw "Copied file absent from source manifest: $relative" }
    $selected[$relative] = $sourceHashes[$relative]
}

$output = New-OutputDirectory -Root $root -Category TaskbarX-Rust
try {
    $payload = Join-Path $output 'payload'
    if (Test-Path -LiteralPath $payload) { throw "Refusing to overwrite payload: $payload" }
    [void][IO.Directory]::CreateDirectory($payload)
    foreach ($relative in $selected.Keys) {
        $file = Get-SourceFile $source $relative
        Copy-VerifiedFile $file (Join-Path $payload $relative) $selected[$relative]
    }
    $launcher = Join-Path $payload 'Open Settings.cmd'
    Write-NewTextFile $launcher "@echo off`r`nstart `"TaskbarX Rust Settings`" `"%~dp0taskbarx-configurator.exe`"`r`n"
    $selected['Open Settings.cmd'] = Get-Sha256 -Path $launcher
    $payloadChecksums = Write-PayloadChecksums -Directory $payload
    $stagedHashes = Read-SourceManifest $payload
    if ($stagedHashes.Count -ne $selected.Count) { throw 'Unexpected staged payload file set.' }
    foreach ($relative in $selected.Keys) {
        if (-not $stagedHashes.ContainsKey($relative) -or $stagedHashes[$relative] -ne $selected[$relative]) {
            throw "Staged payload checksum mismatch: $relative"
        }
    }
    $selected['SHA256SUMS.txt'] = Get-Sha256 -Path $payloadChecksums
    $zip = Join-Path $output 'TaskbarX-Rust-v0.1.0-windows-x64.zip'
    New-PreviewArchive -Directory $payload -ZipPath $zip -RelativePaths @($selected.Keys)
    Assert-PreviewArchive -ZipPath $zip -Expected $selected

    $binaries = [ordered]@{}
    foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe')) { $binaries[$name] = Join-Path $payload $name }
    Copy-ArtifactPayload -Artifacts $binaries -Directory $output
    foreach ($name in $binaries.Keys) {
        if ((Get-Sha256 -Path (Join-Path $output $name)) -ne $sourceHashes[$name]) { throw "Standalone checksum mismatch: $name" }
    }
    $checksums = Join-Path $output 'SHA256SUMS.txt'
    $lines = foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe', [IO.Path]::GetFileName($zip))) {
        '{0}  {1}' -f (Get-Sha256 -Path (Join-Path $output $name)), $name
    }
    Write-NewTextFile $checksums (($lines -join "`n") + "`n")
    [pscustomobject]@{
        OutputDirectory = $output; PackageDirectory = $payload; ZipPath = $zip
        ChecksumsPath = $checksums; PayloadChecksumsPath = $payloadChecksums; ZipChecksumPath = "$zip.sha256"
    }
} catch {
    throw "Portable preview packaging failed; output retained at '$output': $($_.Exception.Message)"
}
