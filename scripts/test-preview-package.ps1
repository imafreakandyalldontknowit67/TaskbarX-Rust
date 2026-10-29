#Requires -Version 5.1
<#
.SYNOPSIS
Runs fixture-only portable-preview packaging tests on PowerShell 5.1+.
.DESCRIPTION
Copies the production packager and unchanged helpers into a unique temporary
fixture repository. All .exe files contain labeled inert bytes and are never run.
Retains fixtures (including failed outputs); never builds or probes the desktop.
.EXAMPLE
powershell.exe -NoProfile -File .\scripts\test-preview-package.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildHelpers.psm1') -Force
Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$id = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N')
$scratch = Join-Path ([IO.Path]::GetTempPath()) "TaskbarX-Rust-preview-tests [fixtures] $id"
$fixtureScripts = Join-Path $scratch 'fixture repository [space]/scripts'
[void][IO.Directory]::CreateDirectory($fixtureScripts)
foreach ($name in @('package-preview.ps1', 'BuildHelpers.psm1')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $fixtureScripts $name)
}
$packager = Join-Path $fixtureScripts 'package-preview.ps1'
$script:passed = 0
$required = @('taskbarx.exe', 'taskbarx-configurator.exe', 'README.md', 'LICENSE',
    'THIRD_PARTY_NOTICES.md', 'settings.sample.json', 'docs/USER-GUIDE.md', 'docs/settings.sample.json')
$supplements = @('docs/assets/art [fixture].svg', 'docs/assets/nested/preview image.png',
    'docs/licenses/component/LICENSE-APACHE', 'docs/licenses/MIT.txt')

function Assert-Test {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw "Preview package test failed: $Message" }
    $script:passed++
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Pattern)
    $caught = $null
    try { & $Action | Out-Null } catch { $caught = $_.Exception.Message }
    Assert-Test ($null -ne $caught -and $caught -match $Pattern) "Expected '$Pattern'; got '$caught'"
}

function Write-FixtureFile {
    param([string]$Directory, [string]$Relative, [string]$Text)
    $path = Join-Path $Directory $Relative
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($path))
    Write-Utf8File -Path $path -Text $Text
}

function New-Fixture {
    param([string]$Name, [switch]$WithSupplements)
    $directory = Join-Path $scratch "sources/$Name [candidate]"
    [void][IO.Directory]::CreateDirectory($directory)
    foreach ($relative in $required) {
        Write-FixtureFile $directory $relative "INERT FIXTURE ONLY - NOT AN APPLICATION OR RELEASE: $relative`r`n"
    }
    Write-FixtureFile $directory 'Open Settings.cmd' '@echo INERT FIXTURE ONLY - app\taskbarx-configurator.exe'
    # All excluded files are also checksummed, just like a broad local candidate.
    foreach ($relative in @('evidence/verification/result.json', 'logs/run.log', 'diagnostics.json',
        'build-metadata.json', 'dependency-inventory.json', 'reference/upstream.exe', 'settings.json',
        'docs/VALIDATION.md', 'docs/PARITY.md', 'docs/INTEGRATION.md', 'docs/TESTING.md', 'docs/SHELL.md',
        'docs/unselected.md', 'docs/settings.json')) {
        Write-FixtureFile $directory $relative "EXCLUDED FIXTURE ONLY: C:\Private\operator\$relative"
    }
    if ($WithSupplements) {
        foreach ($relative in $supplements) { Write-FixtureFile $directory $relative "INERT DOCUMENT/ASSET FIXTURE ONLY: $relative" }
        foreach ($relative in @('docs/assets/reference/upstream.ico', 'docs/assets/upstream.ico', 'docs/assets/extra.dll',
            'docs/assets/installer.msi', 'docs/assets/archive.zip', 'docs/assets/run.cmd', 'docs/assets/secret/settings.json',
            'docs/assets/evidence/result.json', 'docs/assets/logs/private.txt', 'docs/assets/diagnostics.json',
            'docs/assets/build-metadata.json', 'docs/assets/dependency-inventory.json', 'docs/assets/output.stdout.txt',
            'docs/assets/VALIDATION.md', 'docs/licenses/SHELL.md', 'docs/licenses/nested/TESTING-local.md')) {
            Write-FixtureFile $directory $relative "EXCLUDED FIXTURE ONLY: $relative"
        }
    }
    $null = Write-PayloadChecksums -Directory $directory
    return $directory
}

function Get-FixtureSnapshot {
    param([string]$Directory)
    # Include directory names, file bytes, and last-write timestamps; not access times.
    return (@(Get-ChildItem -LiteralPath $Directory -Recurse -Force | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($Directory.Length + 1).Replace('\', '/')
        if ($_.PSIsContainer) { "DIRECTORY $relative" }
        else { '{0}|{1}|{2}|{3}' -f $relative, (Get-Sha256 -Path $_.FullName), $_.LastWriteTimeUtc.Ticks, $_.Attributes }
    }) -join "`n")
}

function Invoke-FixturePackage {
    param([string]$Source)
    $before = Get-FixtureSnapshot $Source
    try { & $packager -SourcePackageDirectory $Source }
    finally { Assert-Test ((Get-FixtureSnapshot $Source) -ceq $before) 'Source fixture unchanged, including on failure' }
}

function Remove-ManifestEntry {
    param([string]$Source, [string]$Relative)
    $manifest = Join-Path $Source 'SHA256SUMS.txt'
    $lines = @([IO.File]::ReadAllLines($manifest) | Where-Object { $_.Substring(66) -cne $Relative })
    Write-Utf8File -Path $manifest -Text (($lines -join "`n") + "`n")
}

function Assert-Manifest {
    param([string]$Path, [string]$Directory, [string[]]$ExpectedNames)
    $lines = [IO.File]::ReadAllLines($Path)
    Assert-Test ($lines.Count -eq $ExpectedNames.Count) "Exact manifest count: $Path"
    $seen = @{}
    foreach ($line in $lines) {
        Assert-Test ($line -cmatch '^([0-9a-f]{64})  (.+)$') 'Checksum syntax'
        $hash = $Matches[1]; $relative = $Matches[2]
        Assert-Test (-not $seen.ContainsKey($relative) -and $relative -cin $ExpectedNames) "Expected unique checksum path: $relative"
        $seen[$relative] = $true
        Assert-Test ((Get-Sha256 -Path (Join-Path $Directory $relative)) -ceq $hash) "Checksum bytes: $relative"
    }
}

function Assert-Package {
    param($Result, [string]$Source, [string[]]$ExpectedNames)
    Assert-Test ($Result.OutputDirectory.StartsWith((Join-Path $scratch 'fixture repository [space]/dist/TaskbarX-Rust/'))) 'Output stays in temporary fixture repository'
    Assert-Test ([IO.Path]::GetFileName($Result.ZipPath) -ceq 'TaskbarX-Rust-v0.1.0-windows-x64.zip') 'Stable archive basename'
    $files = @(Get-ChildItem -LiteralPath $Result.PackageDirectory -File -Recurse -Force | ForEach-Object {
        $_.FullName.Substring($Result.PackageDirectory.Length + 1).Replace('\', '/')
    })
    Assert-Test (($files | Sort-Object) -join "`n" -ceq (($ExpectedNames | Sort-Object) -join "`n")) 'Exact allowlisted payload, with no evidence/private/unreviewed files'
    $launcher = [IO.File]::ReadAllText((Join-Path $Result.PackageDirectory 'Open Settings.cmd'))
    Assert-Test ($launcher -ceq "@echo off`r`nstart `"TaskbarX Rust Settings`" `"%~dp0taskbarx-configurator.exe`"`r`n") 'Generated, correctly quoted sibling launcher (never executed)'
    Assert-Test ($launcher -notmatch 'app[/\\]') 'Launcher does not target app/'
    foreach ($relative in $ExpectedNames | Where-Object { $_ -notin @('Open Settings.cmd', 'SHA256SUMS.txt') }) {
        Assert-Test ((Get-Sha256 -Path (Join-Path $Source $relative)) -eq (Get-Sha256 -Path (Join-Path $Result.PackageDirectory $relative))) "Source bytes preserved: $relative"
    }
    foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe')) {
        Assert-Test ((Get-Sha256 -Path (Join-Path $Source $name)) -eq (Get-Sha256 -Path (Join-Path $Result.OutputDirectory $name))) "Standalone inert EXE bytes: $name"
    }
    Assert-Manifest $Result.PayloadChecksumsPath $Result.PackageDirectory @($ExpectedNames | Where-Object { $_ -ne 'SHA256SUMS.txt' })
    Assert-Manifest $Result.ChecksumsPath $Result.OutputDirectory @('taskbarx.exe', 'taskbarx-configurator.exe', 'TaskbarX-Rust-v0.1.0-windows-x64.zip')
    Assert-Manifest $Result.ZipChecksumPath $Result.OutputDirectory @('TaskbarX-Rust-v0.1.0-windows-x64.zip')
    $outputFiles = @(Get-ChildItem -LiteralPath $Result.OutputDirectory -File -Force | Select-Object -ExpandProperty Name)
    Assert-Test (($outputFiles | Sort-Object) -join ',' -ceq ((@('taskbarx.exe', 'taskbarx-configurator.exe', 'SHA256SUMS.txt',
        'TaskbarX-Rust-v0.1.0-windows-x64.zip', 'TaskbarX-Rust-v0.1.0-windows-x64.zip.sha256') | Sort-Object) -join ',')) 'Exact standalone release asset set'
    # Independently read every ZIP member, including the embedded manifest. Do not
    # merely count files or compare the hash of the compressed container.
    $archive = [IO.Compression.ZipFile]::OpenRead($Result.ZipPath)
    try {
        Assert-Test ($archive.Entries.Count -eq $ExpectedNames.Count) 'ZIP file count'
        $seen = @{}
        foreach ($entry in $archive.Entries) {
            Assert-Test ($entry.FullName -cin $ExpectedNames -and -not $seen.ContainsKey($entry.FullName)) "ZIP root path unique and expected: $($entry.FullName)"
            $seen[$entry.FullName] = $true
            $stream = $entry.Open(); $sha = [Security.Cryptography.SHA256]::Create()
            try { $hash = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
            finally { $sha.Dispose(); $stream.Dispose() }
            Assert-Test ($hash -eq (Get-Sha256 -Path (Join-Path $Result.PackageDirectory $entry.FullName))) "ZIP uncompressed bytes: $($entry.FullName)"
        }
    } finally { $archive.Dispose() }
}

try {
    foreach ($file in @($packager, $PSCommandPath)) {
        $tokens = $null; $errors = $null
        $null = [Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors)
        Assert-Test ($errors.Count -eq 0) "PowerShell syntax: $file"
    }
    Assert-Test ((Get-Sha256 -Path (Join-Path $fixtureScripts 'BuildHelpers.psm1')) -eq (Get-Sha256 -Path (Join-Path $PSScriptRoot 'BuildHelpers.psm1'))) 'Fixture helper module is unchanged'
    $plain = New-Fixture 'optional absent'
    $expected = $required + @('Open Settings.cmd', 'SHA256SUMS.txt')
    # Current directory is intentionally unrelated to both scripts and candidate.
    Push-Location -LiteralPath $scratch
    try { $first = Invoke-FixturePackage $plain } finally { Pop-Location }
    Assert-Package $first $plain $expected
    $firstSnapshot = Get-FixtureSnapshot $first.OutputDirectory
    $second = Invoke-FixturePackage $plain
    Assert-Test ($first.OutputDirectory -ne $second.OutputDirectory) 'Repeated invocation allocates a distinct directory'
    Assert-Test ((Get-FixtureSnapshot $first.OutputDirectory) -ceq $firstSnapshot) 'Earlier package not overwritten'
    Assert-Package $second $plain $expected
    $rich = New-Fixture 'optional present' -WithSupplements
    # Hidden source assets must still be copied and checksummed as ordinary files.
    $hidden = Join-Path $rich $supplements[0]
    [IO.File]::SetAttributes($hidden, [IO.FileAttributes]::Hidden)
    $richResult = Invoke-FixturePackage $rich
    Assert-Package $richResult $rich ($expected + $supplements)

    $missingManifest = New-Fixture 'missing manifest'
    Remove-Item -LiteralPath (Join-Path $missingManifest 'SHA256SUMS.txt')
    Assert-Throws { Invoke-FixturePackage $missingManifest } 'Missing source file: SHA256SUMS.txt'
    foreach ($relative in $required) {
        $missing = New-Fixture ('missing required ' + $relative.Replace('/', '-'))
        Remove-Item -LiteralPath (Join-Path $missing $relative)
        Remove-ManifestEntry $missing $relative
        Assert-Throws { Invoke-FixturePackage $missing } 'Missing source file:'
    }
    $mismatch = New-Fixture 'checksum mismatch'
    Write-FixtureFile $mismatch 'taskbarx.exe' 'CHANGED INERT FIXTURE - NEVER EXECUTE'
    Assert-Throws { Invoke-FixturePackage $mismatch } 'Source checksum mismatch: taskbarx.exe'
    foreach ($relative in @('README.md', $supplements[0])) {
        $unlisted = New-Fixture ('unlisted ' + $relative.Replace('/', '-')) -WithSupplements
        Remove-ManifestEntry $unlisted $relative
        Assert-Throws { Invoke-FixturePackage $unlisted } 'Copied file absent from source manifest:'
    }
    foreach ($mode in @('missing', 'corrupted')) {
        $excluded = New-Fixture "excluded file $mode"
        $file = Join-Path $excluded 'evidence/verification/result.json'
        if ($mode -eq 'missing') { Remove-Item -LiteralPath $file }
        else { Write-Utf8File -Path $file -Text 'CORRUPTED EXCLUDED FIXTURE' }
        Assert-Throws { Invoke-FixturePackage $excluded } '(Missing source file|Source checksum mismatch): evidence/verification/result.json'
    }
    $invalid = New-Fixture 'invalid manifest entries'
    $manifest = Join-Path $invalid 'SHA256SUMS.txt'
    $original = [IO.File]::ReadAllText($manifest)
    $hash = Get-Sha256 -Path (Join-Path $invalid 'README.md')
    foreach ($unsafe in @('../outside.txt', 'docs/../../outside.txt', '/rooted.txt', '\rooted.txt', 'C:\outside.txt',
        'C:relative.txt', '\\server\share\file', 'docs//bad.txt', 'docs/./bad.txt', 'docs/name:stream', 'docs/trailing. ', 'docs/NUL.txt')) {
        Write-Utf8File -Path $manifest -Text ($original + "$hash  $unsafe`n")
        Assert-Throws { Invoke-FixturePackage $invalid } 'Unsafe relative path:'
    }
    foreach ($duplicate in @('README.md', 'readme.MD', 'docs\settings.sample.json')) {
        Write-Utf8File -Path $manifest -Text ($original + "$hash  $duplicate`n")
        Assert-Throws { Invoke-FixturePackage $invalid } 'Duplicate source manifest entry:'
    }
    foreach ($bad in @('', 'not a checksum', "$hash README.md", "$hash`tREADME.md", ('g' * 64 + '  README.md'))) {
        Write-Utf8File -Path $manifest -Text ($original + $bad + "`n")
        Assert-Throws { Invoke-FixturePackage $invalid } 'Malformed source manifest entry'
    }
    Write-Utf8File -Path $manifest -Text ''
    Assert-Throws { Invoke-FixturePackage $invalid } 'Source manifest is empty'

    # Real temporary junctions need no symlink privilege and never target anything
    # outside these fixtures. No application, shell shortcut, or desktop is used.
    $outside = Join-Path $scratch 'outside candidate [fixture]'
    Write-FixtureFile $outside 'private.txt' 'OUTSIDE SOURCE TREE - INERT FIXTURE'
    $outsideBefore = Get-FixtureSnapshot $outside
    $linked = New-Fixture 'reparse escape'
    $link = Join-Path $linked 'docs/assets'
    # PS 5.1 resolves -Target as a wildcard path (unlike the literal file APIs).
    $null = New-Item -ItemType Junction -Path $link -Target ([Management.Automation.WildcardPattern]::Escape($outside))
    Assert-Throws { & $packager -SourcePackageDirectory $linked } 'Reparse points are not permitted'
    $linkedManifest = Join-Path $linked 'SHA256SUMS.txt'
    $linkedText = [IO.File]::ReadAllText($linkedManifest) + (Get-Sha256 -Path (Join-Path $outside 'private.txt')) + "  docs/assets/private.txt`n"
    Write-Utf8File -Path $linkedManifest -Text $linkedText
    Assert-Throws { & $packager -SourcePackageDirectory $linked } 'Reparse points are not permitted'
    Assert-Test ([IO.File]::ReadAllText($linkedManifest) -ceq $linkedText) 'Reparse rejection leaves source manifest unchanged'
    $sourceLink = Join-Path $scratch 'source junction'
    $null = New-Item -ItemType Junction -Path $sourceLink -Target ([Management.Automation.WildcardPattern]::Escape($plain))
    Assert-Throws { & $packager -SourcePackageDirectory $sourceLink } 'Reparse points are not permitted'
    Assert-Test ((Get-FixtureSnapshot $outside) -ceq $outsideBefore) 'Reparse target unchanged'
    Assert-Throws { & $packager -SourcePackageDirectory (Split-Path -Parent $fixtureScripts) } 'Output must not be inside'

    # Load only internal function definitions for focused corruption tests. Normal
    # packaging above invokes the complete, unchanged entry point end-to-end. This
    # avoids adding test hooks/output overrides to production or editing helpers.
    $tokens = $null; $errors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile($packager, [ref]$tokens, [ref]$errors)
    foreach ($definition in $ast.FindAll({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] }, $false)) {
        . ([scriptblock]::Create($definition.Extent.Text))
    }
    $copy = Join-Path $scratch 'copy changed input.exe'
    Assert-Throws { Copy-VerifiedFile (Join-Path $plain 'taskbarx.exe') $copy ('0' * 64) } 'Copied checksum mismatch'
    Assert-Test (Test-Path -LiteralPath $copy -PathType Leaf) 'Failed copy retained'
    $copyBefore = Get-Sha256 -Path $copy
    Assert-Throws { Copy-VerifiedFile (Join-Path $plain 'taskbarx.exe') $copy $copyBefore } 'already exists'
    Assert-Test ((Get-Sha256 -Path $copy) -eq $copyBefore) 'Copy refuses overwrite'
    Assert-Throws { Write-NewTextFile $first.ChecksumsPath 'must not replace' } 'already exists'
    Assert-Throws { New-PreviewArchive $first.PackageDirectory $first.ZipPath $expected } 'overwrite archive checksum'

    $zipExpected = [ordered]@{}
    foreach ($relative in $expected) { $zipExpected[$relative] = Get-Sha256 -Path (Join-Path $first.PackageDirectory $relative) }
    foreach ($mode in @('missing', 'extra', 'duplicate', 'case-alias', 'unsafe', 'backslash', 'mismatch', 'manifest-mismatch', 'symlink')) {
        $badZip = Join-Path $scratch "corrupt $mode.zip"
        $stream = [IO.File]::Open($badZip, [IO.FileMode]::CreateNew)
        try {
            $archive = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create, $true)
            try {
                $names = @($expected)
                if ($mode -eq 'missing') { $names = @($names | Where-Object { $_ -ne 'README.md' }) }
                if ($mode -eq 'extra') { $names += 'extra.txt' }
                if ($mode -eq 'duplicate') { $names += 'README.md' }
                if ($mode -eq 'case-alias') { $names += 'readme.MD' }
                if ($mode -eq 'unsafe') { $names += '../escape.txt' }
                if ($mode -eq 'backslash') { $names += 'docs\unselected.txt' }
                foreach ($relative in $names) {
                    $entry = $archive.CreateEntry($relative)
                    if ($mode -eq 'symlink' -and $relative -eq 'README.md') { $entry.ExternalAttributes = -1610612736 }
                    $entryStream = $entry.Open()
                    try {
                        $file = Join-Path $first.PackageDirectory $relative
                        $bytes = if ($relative -cin $expected) { [IO.File]::ReadAllBytes($file) } else { [byte[]]@(1, 2, 3) }
                        if (($mode -eq 'mismatch' -and $relative -eq 'taskbarx.exe') -or
                            ($mode -eq 'manifest-mismatch' -and $relative -eq 'SHA256SUMS.txt')) { $bytes = [byte[]]@(4, 5, 6) }
                        $entryStream.Write($bytes, 0, $bytes.Length)
                    } finally { $entryStream.Dispose() }
                }
            } finally { $archive.Dispose() }
        } finally { $stream.Dispose() }
        Assert-Throws { Assert-PreviewArchive $badZip $zipExpected } '(Missing ZIP entry|Unexpected ZIP entry|Duplicate ZIP entry|Unsafe relative path|Unsafe ZIP entry|ZIP checksum mismatch)'
        Assert-Test (Test-Path -LiteralPath $badZip -PathType Leaf) "Failed ZIP retained: $mode"
    }
    # Without its sidecar, an already-existing ZIP must still fail CreateNew.
    Assert-Throws { New-PreviewArchive $first.PackageDirectory $badZip $expected } 'already exists'
    Assert-Test ((Get-FixtureSnapshot $first.OutputDirectory) -ceq $firstSnapshot) 'All overwrite rejections preserve the earlier output'

    Write-Host "PASS: $script:passed preview-package assertions. INERT FIXTURES ONLY; no build/application/desktop actions. Retained: $scratch"
} catch {
    Write-Host "FAIL after $script:passed assertions. INERT fixtures retained: $scratch"
    throw
}
