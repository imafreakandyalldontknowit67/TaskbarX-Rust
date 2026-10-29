#Requires -Version 5.1
<#
.SYNOPSIS
Packages two local PE candidates without building, downloading, or executing them.
.DESCRIPTION
Creates TaskbarX-Rust_<Version>_<Architecture>.zip and, for x64/x86 only, .rmskin.
OutputDirectory must be explicitly supplied and either nonexistent or empty
(including hidden entries). Files are created exclusively; partial archives are
retained on failure. There are no loose executable assets or staging directories.

PE/header checks establish only a declared machine type, NOT build provenance,
signing, license completeness, or runtime compatibility. Binary bytes are never
rewritten: embedded compiler paths, if any, require separate build-owner review.
External build metadata is not selected. No Rainmeter installation is required.
.EXAMPLE
powershell.exe -NoProfile -File .\scripts\package-distribution.ps1 -BinaryDirectory 'C:\Candidates\build [reviewed]' -Architecture x64 -OutputDirectory 'C:\Packages\candidate [1]'
.OUTPUTS
An object with OutputDirectory, ZipPath, RmskinPath (null for arm64), Version,
and Architecture. Only completed packaging returns an object.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$BinaryDirectory,
    [Parameter(Mandatory)][ValidateSet('x64', 'x86', 'arm64')][string]$Architecture,
    [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$OutputDirectory,
    [ValidateNotNullOrEmpty()][string]$Version = '0.1.2'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression

function Assert-DistributionPrivacy {
    param([string]$Path)
    $checker = Join-Path $PSScriptRoot 'check-distribution-privacy.ps1'
    if (-not (Test-Path -LiteralPath $checker -PathType Leaf)) {
        throw 'Distribution privacy checker is required.'
    }
    & $checker -Path $Path
    if ($LASTEXITCODE -ne 0) { throw 'Distribution privacy check failed; do not distribute these files.' }
}

function Assert-ArchiveName {
    param([string]$Name)
    if (-not $Name -or $Name.Contains('\') -or [IO.Path]::IsPathRooted($Name)) {
        throw "Unsafe archive name: $Name"
    }
    foreach ($part in $Name.Split('/')) {
        if (-not $part -or $part -in @('.', '..') -or $part -match '[\x00-\x1f\x7f<>:"|?*]' -or
            $part -match '[. ]$' -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(?:\.|$)') {
            throw "Unsafe archive name: $Name"
        }
    }
}

function Get-FilesystemPath {
    param([string]$Path)
    # Inspect the spelling BEFORE normalization so link/../ aliases cannot hide
    # traversal. Brackets and spaces are literal, not wildcard expressions.
    if ([string]::IsNullOrWhiteSpace($Path) -or $Path -match '[\x00-\x1f\x7f<>"|?*]' -or
        $Path -match '^\\\\[?.]\\' -or $Path -match '::' -or
        $Path -match '^[a-zA-Z]:[^\\/]' -or $Path -match '^[a-zA-Z]:$') {
        throw "Unsafe filesystem path: $Path"
    }
    $spelling = $Path.Replace('\', '/') -replace '^[a-zA-Z]:', ''
    foreach ($part in $spelling.Split('/')) {
        if ($part -eq '..' -or ($part -ne '.' -and $part -match '[. ]$') -or $part.Contains(':') -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(?:\.|$)') {
            throw "Unsafe filesystem path: $Path"
        }
    }
    $provider = $null; $drive = $null
    $full = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path, [ref]$provider, [ref]$drive)
    if ($provider.Name -ne 'FileSystem') { throw 'Paths must use the filesystem provider.' }
    $full = [IO.Path]::GetFullPath($full)
    if ($full.Length -gt [IO.Path]::GetPathRoot($full).Length) { $full = $full.TrimEnd('\', '/') }
    return $full
}

function Assert-NormalAncestors {
    param([string]$Path)
    $current = $Path
    while ($current) {
        # Get-Item also observes dangling junctions/symlinks. Do not use Resolve-Path
        # to follow links or recursively enumerate their targets.
        $item = $null
        try { $item = Get-Item -LiteralPath $current -Force -ErrorAction Stop }
        catch [Management.Automation.ItemNotFoundException] { }
        if ($null -ne $item) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Reparse points are not permitted: $current"
            }
            if ($current -ne $Path -and -not $item.PSIsContainer) {
                throw "Path ancestor is not a directory: $current"
            }
        }
        $current = [IO.Path]::GetDirectoryName($current)
    }
}

function Test-WithinPath {
    param([string]$Child, [string]$Parent)
    return $Child.Equals($Parent, [StringComparison]::OrdinalIgnoreCase) -or
        $Child.StartsWith($Parent.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)
}

function Assert-EmptyOutput {
    param([string]$Path, [string]$OwnedClaim = '')
    Assert-NormalAncestors $Path
    if (Test-Path -LiteralPath $Path) {
        if (-not (Test-Path -LiteralPath $Path -PathType Container)) { throw 'Output must be a directory.' }
        $other = @(Get-ChildItem -LiteralPath $Path -Force | Where-Object { $_.Name -cne $OwnedClaim })
        if ($other.Count -ne 0) { throw "Output directory must be empty; refusing to overwrite: $Path" }
    }
}

function Open-PayloadFile {
    param([string]$Path, [string]$Name)
    Assert-ArchiveName $Name
    Assert-NormalAncestors $Path
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing input file: $Path" }
    # Hold the same read-only handles from validation through BOTH archive writes.
    # FileShare.Read denies concurrent modification/deletion of selected files.
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    return [pscustomobject]@{ Name = $Name; Stream = $stream }
}

function Assert-PeMachine {
    param([IO.Stream]$Stream, [string]$Name, [uint16]$Expected)
    $reader = New-Object IO.BinaryReader($Stream, [Text.Encoding]::ASCII, $true)
    try {
        $Stream.Position = 0
        if ($Stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Invalid DOS MZ header: $Name" }
        $Stream.Position = 0x3c
        [long]$offset = $reader.ReadInt32()
        # Subtract first: malicious offsets must not overflow or allocate buffers.
        if ($offset -lt 64 -or $offset -gt ($Stream.Length - 24)) { throw "Invalid PE offset or truncated COFF header: $Name" }
        $Stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw "Invalid PE signature: $Name" }
        $coff = $reader.ReadBytes(20)
        if ($coff.Length -ne 20) { throw "Truncated COFF header: $Name" }
        $machine = [int]$coff[0] + 256 * [int]$coff[1]
        if ($machine -ne $Expected) { throw ("PE machine mismatch for {0}: expected 0x{1:X4}, found 0x{2:X4}" -f $Name, $Expected, $machine) }
        $sections = [int]$coff[2] + 256 * [int]$coff[3]
        $optionalSize = [int]$coff[16] + 256 * [int]$coff[17]
        if (($offset + 24 + $optionalSize + 40 * $sections) -gt $Stream.Length) {
            throw "Truncated declared PE headers: $Name"
        }
        # Deliberately not a Windows loader, signature verifier, or executable
        # provenance check. Inert fixture headers can satisfy these checks.
    } finally { $reader.Dispose(); $Stream.Position = 0 }
}

function Get-ReviewedNoticeNames {
    # Explicit inventory of the existing docs/licenses notice texts, not a scan
    # of arbitrary *.txt files or of the checkout/docs tree. New dependency notice
    # names must be reviewed and added here. Copy the present texts; this does NOT
    # certify that supplied binaries match Cargo.lock or that notices are complete.
    'docs/licenses/README.md'
    $dual = @('anstream-1.0.0', 'anstyle-1.0.14', 'anstyle-parse-1.0.0', 'anstyle-query-1.1.5',
        'anstyle-wincon-3.0.11', 'clap-4.6.7', 'clap_builder-4.6.7', 'clap_derive-4.6.7',
        'clap_lex-1.1.1', 'colorchoice-1.0.5', 'heck-0.5.0', 'is_terminal_polyfill-1.70.2',
        'itoa-1.0.18', 'once_cell_polyfill-1.70.2', 'proc-macro2-1.0.107', 'quote-1.0.47',
        'serde-1.0.229', 'serde_core-1.0.229', 'serde_derive-1.0.229', 'serde_json-1.0.154',
        'syn-2.0.119', 'syn-3.0.9', 'unicode-ident-1.0.27', 'utf8parse-0.2.2')
    foreach ($dependency in $dual) {
        "docs/licenses/$dependency/LICENSE-APACHE"
        "docs/licenses/$dependency/LICENSE-MIT"
    }
    foreach ($dependency in @('windows-0.61.3', 'windows-collections-0.2.0', 'windows-core-0.61.2',
        'windows-future-0.2.1', 'windows-implement-0.60.2', 'windows-interface-0.59.3',
        'windows-link-0.1.3', 'windows-link-0.2.1', 'windows-numerics-0.2.0',
        'windows-result-0.3.4', 'windows-strings-0.4.2', 'windows-sys-0.61.2', 'windows-threading-0.1.0')) {
        "docs/licenses/$dependency/license-apache-2.0"
        "docs/licenses/$dependency/license-mit"
    }
    'docs/licenses/memchr-2.8.3/COPYING'
    'docs/licenses/memchr-2.8.3/LICENSE-MIT'
    'docs/licenses/strsim-0.11.1/LICENSE'
    'docs/licenses/unicode-ident-1.0.27/LICENSE-UNICODE'
    'docs/licenses/zmij-1.0.23/LICENSE-MIT'
}

function Assert-NoticeText {
    param($Item)
    # A reviewed filename is not permission to include opaque binary contents.
    if ($Item.Stream.Length -eq 0 -or $Item.Stream.Length -gt 1MB) { throw "Invalid license text size: $($Item.Name)" }
    $reader = New-Object IO.StreamReader($Item.Stream, (New-Object Text.UTF8Encoding($false, $true)), $true, 4096, $true)
    try {
        $text = $reader.ReadToEnd()
        if ($text -match '[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]') { throw "Invalid license text content: $($Item.Name)" }
    } finally { $reader.Dispose(); $Item.Stream.Position = 0 }
}

function New-TextPayload {
    param([string]$Name, [string]$Text)
    Assert-ArchiveName $Name
    # Generated text is ASCII-compatible UTF-8 with CRLF (also understood by the
    # Windows INI reader); no user filesystem path is ever interpolated here.
    $bytes = (New-Object Text.UTF8Encoding($false)).GetBytes(($Text -replace '\r?\n', "`r`n"))
    return [pscustomobject]@{ Name = $Name; Bytes = $bytes }
}

function Write-DistributionArchive {
    param([string]$Path, [object[]]$Files, [object[]]$Texts, [string]$Prefix = '', [switch]$Rmskin)
    Assert-NormalAncestors $Path
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $archive = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            $seen = @{}
            foreach ($item in $Files + $Texts) {
                $name = $Prefix + $item.Name
                # RMSKIN.ini and Launcher.ini are archive-level generated text.
                if ($item.Name -eq 'RMSKIN.ini' -or $item.Name.StartsWith('Skins/')) { $name = $item.Name }
                Assert-ArchiveName $name
                if ($seen.ContainsKey($name)) { throw "Duplicate archive entry: $name" }
                $seen[$name] = $true
                $entry = $archive.CreateEntry($name, [IO.Compression.CompressionLevel]::Optimal)
                $entryStream = $entry.Open()
                try {
                    if ($null -ne $item.PSObject.Properties['Stream']) {
                        $item.Stream.Position = 0
                        $item.Stream.CopyTo($entryStream)
                    } else { $entryStream.Write($item.Bytes, 0, $item.Bytes.Length) }
                } finally { $entryStream.Dispose() }
            }
        } finally { $archive.Dispose() }
        if ($Rmskin) {
            # Independently implemented from the published format, not copied GPL
            # implementation code. Official sources inspected at stable release
            # v4.5.28.3936, commit cbd528813fa72fd48fb946c0e994367ed1aca415:
            # https://docs.rainmeter.net/manual/distributing-skins/
            # https://github.com/rainmeter/rainmeter/blob/cbd528813fa72fd48fb946c0e994367ed1aca415/Library/DialogInstall.h
            # https://github.com/rainmeter/rainmeter/blob/cbd528813fa72fd48fb946c0e994367ed1aca415/Library/DialogPackage.cpp
            # https://github.com/rainmeter/rainmeter/blob/cbd528813fa72fd48fb946c0e994367ed1aca415/Library/DialogInstall.cpp
            # PackageFooter / CreatePackage / ReadPackage establish 16 bytes:
            # 8-byte LE preceding ZIP length, 1-byte flags (normal package = 0),
            # then ASCII RMSKIN plus NUL. Flags are NOT an architecture/signing
            # mechanism. The ZIP must be closed before measuring its byte length.
            $stream.Position = $stream.Length
            $writer = New-Object IO.BinaryWriter($stream, [Text.Encoding]::ASCII, $true)
            try {
                $writer.Write([uint64]$stream.Length) # BinaryWriter is little-endian.
                $writer.Write([byte]0)
                $writer.Write([byte[]]@(0x52, 0x4d, 0x53, 0x4b, 0x49, 0x4e, 0))
                $writer.Flush()
            } finally { $writer.Dispose() }
        }
    } finally { $stream.Dispose() }
}

# Conservative version grammar: no normalization/sanitizing, trailing dots,
# traversal, whitespace, INI syntax, shell metacharacters, or control characters.
if ($Version.Length -gt 48 -or $Version -cnotmatch '\A[0-9A-Za-z]+(?:[._+-][0-9A-Za-z]+)*\z') {
    throw 'Unsafe Version: use 1-48 ASCII letters/digits separated by single dots, underscores, pluses or hyphens.'
}
$Architecture = $Architecture.ToLowerInvariant()
$machines = @{ x64 = 0x8664; x86 = 0x014c; arm64 = 0xaa64 }
$binary = Get-FilesystemPath $BinaryDirectory
$output = Get-FilesystemPath $OutputDirectory
$root = Get-FilesystemPath ([IO.Path]::GetDirectoryName($PSScriptRoot))
Assert-NormalAncestors $binary
if (-not (Test-Path -LiteralPath $binary -PathType Container)) { throw 'BinaryDirectory must be an existing directory.' }
if ((Test-WithinPath $output $binary) -or (Test-WithinPath $binary $output)) {
    throw 'Binary and output directories must not overlap.'
}
Assert-EmptyOutput $output

$files = New-Object 'Collections.Generic.List[object]'
$claim = $null
try {
    foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe')) {
        $item = Open-PayloadFile (Join-Path $binary $name) $name
        $files.Add($item)
        Assert-PeMachine $item.Stream $name $machines[$Architecture]
        Assert-DistributionPrivacy (Join-Path $binary $name)
    }
    foreach ($name in @('LICENSE', 'THIRD_PARTY_NOTICES.md', 'docs/settings.sample.json', 'docs/licenses/README.md')) {
        $destination = $name
        if ($name -eq 'docs/settings.sample.json') { $destination = 'settings.sample.json' }
        $item = Open-PayloadFile (Join-Path $root $name) $destination
        $files.Add($item)
        if ($name -ne 'docs/settings.sample.json') { Assert-NoticeText $item }
    }
    $noticeCount = 0
    foreach ($name in Get-ReviewedNoticeNames) {
        if ($name -eq 'docs/licenses/README.md') { continue }
        $path = Join-Path $root $name
        Assert-NormalAncestors $path
        if (Test-Path -LiteralPath $path) {
            $item = Open-PayloadFile $path $name
            $files.Add($item)
            Assert-NoticeText $item
            $noticeCount++
        }
    }
    if ($noticeCount -eq 0) { throw 'Missing dependency license texts in docs/licenses.' }

    $texts = @(
        (New-TextPayload 'Open Settings.cmd' "@echo off`nsetlocal DisableDelayedExpansion`nstart `"TaskbarX Rust Settings`" `"%~dp0taskbarx-configurator.exe`"`n"),
        (New-TextPayload 'README.txt' @"
TaskbarX-Rust $Version - $Architecture unsigned distribution

Independent Rust rewrite of TaskbarX by Chris Andriessen; not an official
upstream release or endorsement. See LICENSE, THIRD_PARTY_NOTICES.md and
docs/licenses/ for attribution and dependency notice texts. No upstream artwork,
resources or .NET libraries are bundled.

PORTABLE USE
Extract the entire ZIP into a stable folder you control, not a temporary folder
or the archive viewer. Keep both EXEs and the accompanying files together.
Open Settings.cmd opens only its sibling taskbarx-configurator.exe. Opening
settings does not start the taskbar engine. Save only persists settings; use the
explicit Apply/start action to save, stop and start the engine. Closing settings
does not stop an independently running engine.

Default settings: %APPDATA%\TaskbarX-Rust\settings.json (roaming AppData).
settings.sample.json is only a reference; it is not an installed user profile.
Startup registration is separate: use Startup Enable/update explicitly after
choosing the stable folder. Save and Apply do not enable or update startup.
If you move the folder, explicitly update startup again, or Disable it.

RAINMETER (x64/x86 only; no ARM Rainmeter package)
Rainmeter is optional and is not required for the portable ZIP. Installation of
the .rmskin alone does not run either application. Manually load Launcher.ini in
TaskbarX-Rust-<arch>, then explicitly click its visible Open Settings target.
Loading or refreshing the skin does not execute application binaries. The click
opens only the bundled configurator, not the engine. Use Apply/start and the
separate Startup Enable/update action as described above. No automatic Load,
plugins, layouts, installation hooks or startup changes are supplied.

LIMITS / RELEASE REVIEW
These are unsigned packages, not signed or certified releases. Packaging
checks the declared PE machine type of BOTH files, not genuine build provenance,
runtime compatibility, complete license compliance, or safety. Choose the
architecture appropriate to your Windows system. Rainmeter's footer does not
enforce processor compatibility and provides no signing or authentication.
The runtime targets classic Windows taskbars; do not assume stock Windows 11
taskbar support, feature parity, or tested x86/ARM64 runtime behavior. Each real
build and Windows/taskbar combination requires independent validation.
Binary bytes are preserved, including any embedded compiler paths; the build
owner must review those separately. External build-path metadata is excluded.
Nothing in this package establishes permission or readiness to publish.
"@)
    )
    $skin = "Skins/TaskbarX-Rust-$Architecture"
    $skinTexts = @(
        (New-TextPayload 'RMSKIN.ini' @"
[rmskin]
Name=TaskbarX-Rust $Architecture unsigned package
Author=TaskbarX-Rust independent rewrite contributors
Version=$Version
MinimumRainmeter=4.5.0
MinimumWindows=10.0
"@),
        (New-TextPayload "$skin/Launcher.ini" @"
[Rainmeter]
Update=-1
AccurateText=1

[Metadata]
Name=TaskbarX-Rust $Architecture launcher
Author=TaskbarX-Rust independent rewrite contributors
Information=Unsigned $Architecture package. Click to open settings only. Apply/start and Startup Enable/update are separate explicit actions.
Version=$Version
License=MIT

[OpenSettings]
Meter=String
Text=Open TaskbarX-Rust Settings ($Architecture)
FontFace=Segoe UI
FontSize=12
FontColor=255,255,255,255
SolidColor=32,32,32,255
Padding=12,8,12,8
AntiAlias=1
MouseActionCursor=1
LeftMouseUpAction=["#@#TaskbarX-Rust\taskbarx-configurator.exe"]
"@)
    )

    # Validate every input BEFORE creating the output. Recheck after acquiring an
    # exclusive claim to detect competing packagers of different architectures.
    Assert-EmptyOutput $output
    [void][IO.Directory]::CreateDirectory($output)
    Assert-NormalAncestors $output
    $claimName = '.taskbarx-distribution-owner'
    $claimPath = Join-Path $output $claimName
    $claim = New-Object IO.FileStream($claimPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite,
        [IO.FileShare]::None, 4096, [IO.FileOptions]::DeleteOnClose)
    # DeleteOnClose cleans up only the exact handle we exclusively created. No
    # path-based or recursive cleanup, including on failure; archives are retained.
    Assert-EmptyOutput $output $claimName
    $basename = "TaskbarX-Rust_${Version}_$Architecture"
    $zip = Join-Path $output "$basename.zip"
    Write-DistributionArchive $zip $files.ToArray() $texts
    Assert-DistributionPrivacy $zip
    $rmskin = $null
    if ($Architecture -in @('x64', 'x86')) {
        $rmskin = Join-Path $output "$basename.rmskin"
        Write-DistributionArchive $rmskin $files.ToArray() ($texts + $skinTexts) "$skin/@Resources/TaskbarX-Rust/" -Rmskin
        Assert-DistributionPrivacy $rmskin
    }
    [pscustomobject]@{ OutputDirectory = $output; ZipPath = $zip; RmskinPath = $rmskin; Version = $Version; Architecture = $Architecture }
} finally {
    if ($null -ne $claim) { $claim.Dispose() }
    foreach ($item in $files) { $item.Stream.Dispose() }
}
