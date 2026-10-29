#Requires -Version 5.1
<#
.SYNOPSIS
Offline, dependency-free distribution tests using only inert fixture PE bytes.
.DESCRIPTION
Copies the packager and privacy checker into unique temporary fixture repositories.
Does not read real application binaries or execute any EXE, CMD, or Rainmeter
skin. No build, download, installation, desktop/startup action, or publication.
Fixtures are retained for inspection, including on failure; no recursive cleanup.
.EXAMPLE
powershell.exe -NoProfile -File .\scripts\test-distribution.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
$script:passed = 0
$script:skipped = New-Object 'Collections.Generic.List[string]'
$script:nextCase = 0
$production = Join-Path $PSScriptRoot 'package-distribution.ps1'
$scratch = [IO.Path]::GetFullPath((Join-Path ([IO.Path]::GetTempPath()) ('TaskbarX distribution [fixtures] ' + [Guid]::NewGuid().ToString('N'))))

function Assert-Test {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw "Distribution fixture assertion failed: $Message" }
    $script:passed++
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Pattern)
    $caught = $null
    try { & $Action | Out-Null } catch { $caught = $_.Exception.Message }
    Assert-Test ($null -ne $caught -and $caught -match $Pattern) "Expected '$Pattern', got '$caught'"
}

function Assert-FixturePath {
    param([string]$Path)
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($scratch + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Fixture operation escapes its unique temporary directory: $Path"
    }
}

function Get-Hash {
    param([byte[]]$Bytes)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($sha.ComputeHash($Bytes)).Replace('-', '') }
    finally { $sha.Dispose() }
}

function Write-Fixture {
    param([string]$Path, [string]$Text)
    Assert-FixturePath $Path
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path))
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}

function Write-FakePe {
    param([string]$Path, [int]$Machine)
    Assert-FixturePath $Path
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path))
    $bytes = New-Object byte[] 512
    $bytes[0] = 0x4d; $bytes[1] = 0x5a
    $bytes[0x3c] = 128
    $bytes[128] = 0x50; $bytes[129] = 0x45
    $bytes[132] = [byte]($Machine -band 255); $bytes[133] = [byte]($Machine -shr 8)
    # Deliberately inert, not a runnable PE: complete COFF header, no sections or
    # optional header. The production machine check is not a loader/provenance test.
    # Also prove that packaging does not strip embedded (fictional) compiler paths.
    $label = [Text.Encoding]::ASCII.GetBytes('INERT FIXTURE - NEVER EXECUTE: ' + [IO.Path]::GetFileName($Path) + ' C:\FixtureCompilerPath\source.rs')
    [Array]::Copy($label, 0, $bytes, 256, $label.Length)
    [IO.File]::WriteAllBytes($Path, $bytes)
}

function Get-SnapshotLines {
    param([string]$Directory, [string]$Prefix = '')
    foreach ($item in @(Get-ChildItem -LiteralPath $Directory -Force | Sort-Object Name)) {
        $name = $Prefix + $item.Name
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            # Do not recurse into any junction/symlink, even in test snapshots.
            "REPARSE|$name|$($item.Attributes)"
        } elseif ($item.PSIsContainer) {
            "DIRECTORY|$name"
            Get-SnapshotLines $item.FullName ($name + '/')
        } else {
            '{0}|{1}|{2}|{3}' -f $name, (Get-Hash ([IO.File]::ReadAllBytes($item.FullName))), $item.LastWriteTimeUtc.Ticks, $item.Attributes
        }
    }
}

function Get-Snapshot {
    param([string]$Directory)
    if (-not (Test-Path -LiteralPath $Directory)) { return '<absent>' }
    $item = Get-Item -LiteralPath $Directory -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { return "REPARSE ROOT|$($item.Attributes)" }
    if (Test-Path -LiteralPath $Directory -PathType Leaf) {
        return (Get-Hash ([IO.File]::ReadAllBytes($Directory)))
    }
    return (@(Get-SnapshotLines $Directory) -join "`n")
}

$noticeNames = @('docs/licenses/README.md', 'docs/licenses/anstream-1.0.0/LICENSE-APACHE',
    'docs/licenses/anstream-1.0.0/LICENSE-MIT', 'docs/licenses/memchr-2.8.3/COPYING',
    'docs/licenses/windows-0.61.3/license-apache-2.0', 'docs/licenses/windows-0.61.3/license-mit')
$portableNames = @('taskbarx.exe', 'taskbarx-configurator.exe', 'Open Settings.cmd', 'LICENSE',
    'THIRD_PARTY_NOTICES.md', 'settings.sample.json', 'README.txt') + $noticeNames

function New-FixtureRepository {
    param([string]$Label = 'case', [int]$Machine = 0x8664)
    $script:nextCase++
    $case = Join-Path $scratch ("inputs/{0:D3} {1} [literal]" -f $script:nextCase, $Label)
    $repo = Join-Path $case 'fixture checkout [repo]'
    $bin = Join-Path $case 'supplied binaries [not host arch]'
    $scriptPath = Join-Path $repo 'scripts/package-distribution.ps1'
    Assert-FixturePath $scriptPath
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($scriptPath))
    [IO.File]::Copy($production, $scriptPath, $false)
    [IO.File]::Copy((Join-Path $PSScriptRoot 'check-distribution-privacy.ps1'),
        (Join-Path ([IO.Path]::GetDirectoryName($scriptPath)) 'check-distribution-privacy.ps1'), $false)
    foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe')) { Write-FakePe (Join-Path $bin $name) $Machine }
    Write-Fixture (Join-Path $case 'sentinel.txt') 'UNCHANGED FIXTURE SENTINEL'
    foreach ($name in @('LICENSE', 'THIRD_PARTY_NOTICES.md') + $noticeNames) {
        Write-Fixture (Join-Path $repo $name) "INERT FIXTURE LEGAL TEXT ONLY: $name`r`nCopyright fixture author.`r`n"
    }
    Write-Fixture (Join-Path $repo 'docs/settings.sample.json') "{`"schema_version`":1,`"fixture`":true}`r`n"
    # These tempting files must NOT leak into either archive. Include unreviewed
    # files even under otherwise approved license folders, and private-path text.
    foreach ($name in @('README.md', 'USER-GUIDE.md', 'RELEASE-v0.1.0.md', 'DISTRIBUTION.md',
        'Open Settings.cmd', 'settings.json', 'Cargo.toml', 'src/main.rs', 'evidence/result.json',
        'diagnostics.json', 'logs/run.log', 'build-metadata.json', 'docs/USER-GUIDE.md',
        'docs/assets/artwork.png', 'docs/assets/upstream.ico', 'upstream/TaskbarX.exe',
        'app/extra.dll', 'docs/licenses/diagnostics.txt', 'docs/licenses/extra.dll',
        'docs/licenses/unreviewed/LICENSE', 'docs/licenses/anstream-1.0.0/extra.dll',
        'docs/licenses/anstream-1.0.0/src/LICENSE', 'docs/licenses/anstream-1.0.0/artwork.ico')) {
        Write-Fixture (Join-Path $repo $name) 'EXCLUDED PRIVATE METADATA C:\Private\compiler\source - not payload'
    }
    foreach ($name in @('diagnostics.json', 'taskbarx.pdb', 'settings.json', 'extra.exe', 'extra.dll', 'artwork.ico',
        'source/main.rs', 'evidence/stdout.txt', 'build-metadata.json')) {
        Write-Fixture (Join-Path $bin $name) 'EXCLUDED BINARY-DIRECTORY METADATA C:\Private\build'
    }
    [pscustomobject]@{ Case = $case; Root = $repo; Binary = $bin; Script = $scriptPath }
}

function New-OutputPath {
    return (Join-Path $scratch ('outputs/new [literal] ' + [Guid]::NewGuid().ToString('N')))
}

function Invoke-CheckedPackage {
    param($Fixture, [string]$Architecture = 'x64', [string]$Output = (New-OutputPath),
        [AllowEmptyString()][string]$Version, [string]$Binary = $Fixture.Binary, [string]$ScriptPath = $Fixture.Script)
    $before = Get-Snapshot $Fixture.Case
    $arguments = @{ BinaryDirectory = $Binary; Architecture = $Architecture; OutputDirectory = $Output }
    if ($PSBoundParameters.ContainsKey('Version')) { $arguments.Version = $Version }
    try { & $ScriptPath @arguments }
    finally { Assert-Test ((Get-Snapshot $Fixture.Case) -ceq $before) 'All fixture inputs, attributes, timestamps and sentinels unchanged' }
}

function Assert-PackageFailure {
    param($Fixture, [string]$Pattern, [hashtable]$Arguments = @{})
    $argsCopy = @{} + $Arguments
    if (-not $argsCopy.ContainsKey('Output')) { $argsCopy.Output = New-OutputPath }
    $before = Get-Snapshot $argsCopy.Output
    Assert-Throws { Invoke-CheckedPackage -Fixture $Fixture @argsCopy } $Pattern
    Assert-Test ((Get-Snapshot $argsCopy.Output) -ceq $before) 'Rejected packaging does not create archives or change existing output/sentinels'
}

function Read-ZipMembers {
    param([byte[]]$Bytes)
    $memory = New-Object IO.MemoryStream
    try {
        $memory.Write($Bytes, 0, $Bytes.Length); $memory.Position = 0
        $zip = New-Object IO.Compression.ZipArchive($memory, [IO.Compression.ZipArchiveMode]::Read, $true)
        try {
            $members = @{}
            foreach ($entry in $zip.Entries) {
                $name = $entry.FullName
                Assert-Test (-not $members.ContainsKey($name)) "No duplicate/case-alias ZIP entry: $name"
                Assert-Test ($name -notmatch '\\|(^/)|(^|/)\.\.?(/|$)|[\x00-\x1f:<>"|?*]|/$|//' ) "Safe slash-separated ZIP path: $name"
                Assert-Test (($entry.ExternalAttributes -band 0x410) -eq 0) "No directory/reparse attribute: $name"
                $stream = $entry.Open(); $buffer = New-Object IO.MemoryStream
                try { $stream.CopyTo($buffer); $members[$name] = $buffer.ToArray() }
                finally { $stream.Dispose(); $buffer.Dispose() }
            }
            return $members
        } finally { $zip.Dispose() }
    } finally { $memory.Dispose() }
}

function Read-TestArchive {
    param([string]$Path, [switch]$Rmskin)
    $bytes = [IO.File]::ReadAllBytes($Path)
    $zipLength = $bytes.Length
    if ($Rmskin) {
        # Independent byte-level inspection: no production functions, footer
        # reader, BinaryWriter, or BitConverter length encoding is reused here.
        if ($bytes.Length -lt 38) { throw 'Invalid RMSKIN footer: truncated package' }
        $zipLength -= 16
        [decimal]$declared = 0; [decimal]$multiplier = 1
        for ($i = 0; $i -lt 8; $i++) {
            $declared += $bytes[$zipLength + $i] * $multiplier
            $multiplier *= 256
        }
        if ($declared -ne $zipLength) { throw 'Invalid RMSKIN footer: ZIP length' }
        if ($bytes[$zipLength + 8] -ne 0) { throw 'Invalid RMSKIN footer: expected normal-package flags 0' }
        $signature = @(82, 77, 83, 75, 73, 78, 0)
        for ($i = 0; $i -lt 7; $i++) {
            if ($bytes[$zipLength + 9 + $i] -ne $signature[$i]) { throw 'Invalid RMSKIN footer: signature or NUL terminator' }
        }
        Assert-Test ($declared -gt 0) 'Nonempty preceding ZIP length, little-endian 64-bit footer'
    }
    Assert-Test ($zipLength -ge 22) 'ZIP includes complete EOCD'
    # Our writer emits no ZIP comments; the footer must immediately follow EOCD.
    Assert-Test (($bytes[$zipLength - 22], $bytes[$zipLength - 21], $bytes[$zipLength - 20], $bytes[$zipLength - 19] -join ',') -ceq '80,75,5,6') 'Readable ZIP EOCD at boundary, not a renamed ZIP or a footer included in ZIP length'
    $zipBytes = New-Object byte[] $zipLength
    [Array]::Copy($bytes, 0, $zipBytes, 0, $zipLength)
    $members = Read-ZipMembers $zipBytes
    if ($Rmskin) {
        # In addition to the bounded ZIP, ensure an ordinary framework ZIP reader
        # can read the actual file with trailing footer bytes.
        $whole = Read-ZipMembers $bytes
        Assert-Test ($whole.Count -eq $members.Count) 'Full RMSKIN file remains ZIP-readable'
        foreach ($name in $members.Keys) {
            Assert-Test ((Get-Hash $whole[$name]) -ceq (Get-Hash $members[$name])) "Footer does not alter ZIP bytes: $name"
        }
    }
    return $members
}

function ConvertFrom-FixtureIni {
    param([byte[]]$Bytes)
    $sections = @{}; $section = $null
    foreach ($line in ([Text.Encoding]::UTF8.GetString($Bytes) -split '\r?\n')) {
        if (-not $line.Trim() -or $line.StartsWith(';')) { continue }
        if ($line -cmatch '^\[([^\]]+)\]$') {
            $section = $Matches[1]
            Assert-Test (-not $sections.ContainsKey($section)) "Unique INI section $section"
            $sections[$section] = @{}
        } else {
            Assert-Test ($null -ne $section -and $line -cmatch '^([^=]+)=(.*)$') 'Valid INI key/value syntax'
            $key = $Matches[1]; $value = $Matches[2]
            Assert-Test (-not $sections[$section].ContainsKey($key)) "Unique INI key $key"
            $sections[$section][$key] = $value
        }
    }
    return $sections
}

function Assert-Names {
    param([string[]]$Actual, [string[]]$Expected, [string]$Message)
    Assert-Test (($Actual | Sort-Object) -join "`n" -ceq (($Expected | Sort-Object) -join "`n")) $Message
}

function Assert-PortablePayload {
    param($Members, $Fixture, [string]$Prefix, [string]$Version, [string]$Architecture)
    foreach ($name in $portableNames) {
        Assert-Test ($Members.ContainsKey($Prefix + $name)) "Complete payload: $Prefix$name"
        $bytes = $Members[$Prefix + $name]
        $source = $null
        if ($name -in @('taskbarx.exe', 'taskbarx-configurator.exe')) { $source = Join-Path $Fixture.Binary $name }
        elseif ($name -eq 'settings.sample.json') { $source = Join-Path $Fixture.Root 'docs/settings.sample.json' }
        elseif ($name -notin @('Open Settings.cmd', 'README.txt')) { $source = Join-Path $Fixture.Root $name }
        if ($null -ne $source) {
            $original = [IO.File]::ReadAllBytes($source)
            Assert-Test ([Convert]::ToBase64String($bytes) -ceq [Convert]::ToBase64String($original)) "Exact input bytes: $name"
            Assert-Test ((Get-Hash $bytes) -ceq (Get-Hash $original)) "SHA-256 preserved: $name"
        }
        Assert-Test ([Text.Encoding]::UTF8.GetString($bytes) -notmatch 'EXCLUDED (PRIVATE|BINARY)|C:\\Private\\') "No excluded external metadata: $name"
    }
    $cmd = [Text.Encoding]::UTF8.GetString($Members[$Prefix + 'Open Settings.cmd'])
    Assert-Test ($cmd -ceq "@echo off`r`nsetlocal DisableDelayedExpansion`r`nstart `"TaskbarX Rust Settings`" `"%~dp0taskbarx-configurator.exe`"`r`n") 'CMD targets only the correctly quoted sibling configurator, never app/ or the engine'
    $readme = [Text.Encoding]::UTF8.GetString($Members[$Prefix + 'README.txt'])
    foreach ($pattern in @([regex]::Escape($Version), [regex]::Escape($Architecture), 'stable folder',
        'Opening\s+settings does not start', 'Apply/start', 'Startup Enable/update', '%APPDATA%\\TaskbarX-Rust\\settings.json',
        'unsigned', 'Independent Rust rewrite', 'Chris Andriessen', 'Windows 11', 'Windows 10',
        'architecture', 'security checks')) {
        Assert-Test ($readme -match $pattern) "Bundled instructions cover: $pattern"
    }
    Assert-Test ($readme -notmatch [regex]::Escape($scratch)) 'Generated instructions contain no absolute fixture/build paths'
}

function Assert-Package {
    param($Result, $Fixture, [string]$Architecture, [string]$Version)
    $base = "TaskbarX-Rust_${Version}_$Architecture"
    Assert-Test ($Result.Architecture -ceq $Architecture -and $Result.Version -ceq $Version) 'Result architecture/version'
    Assert-Test ([IO.Path]::GetFileName($Result.ZipPath) -ceq "$base.zip") 'Default/custom ZIP filename'
    $assets = @("$base.zip")
    $zip = Read-TestArchive $Result.ZipPath
    Assert-Names @($zip.Keys) $portableNames 'Exact flat ZIP inventory: no extras or directory entries'
    Assert-PortablePayload $zip $Fixture '' $Version $Architecture
    if ($Architecture -eq 'arm64') {
        Assert-Test ($null -eq $Result.RmskinPath) 'ARM64 emits no RMSKIN (nor ARM32/AnyCPU packages)'
    } else {
        $assets += "$base.rmskin"
        Assert-Test ([IO.Path]::GetFileName($Result.RmskinPath) -ceq "$base.rmskin") 'RMSKIN filename identifies architecture'
        $rmskin = Read-TestArchive $Result.RmskinPath -Rmskin
        $skin = "Skins/TaskbarX-Rust-$Architecture"
        $prefix = "$skin/@Resources/TaskbarX-Rust/"
        $expected = @('RMSKIN.ini', "$skin/Launcher.ini") + @($portableNames | ForEach-Object { $prefix + $_ })
        Assert-Names @($rmskin.Keys) $expected 'Exact RMSKIN inventory: one launcher, complete portable payload, no layouts/plugins/hooks'
        Assert-PortablePayload $rmskin $Fixture $prefix $Version $Architecture
        foreach ($name in $portableNames) {
            Assert-Test ((Get-Hash $zip[$name]) -ceq (Get-Hash $rmskin[$prefix + $name])) "ZIP and RMSKIN payloads identical: $name"
        }
        $metadata = ConvertFrom-FixtureIni $rmskin['RMSKIN.ini']
        Assert-Names @($metadata.Keys) @('rmskin') 'Root [rmskin] metadata'
        Assert-Names @($metadata.rmskin.Keys) @('Name', 'Author', 'Version', 'MinimumRainmeter', 'MinimumWindows') 'Metadata has no Load, LoadType, layouts or installation hooks'
        Assert-Test ($metadata.rmskin.Name -ceq "TaskbarX-Rust $Architecture unsigned package") 'Metadata labels architecture and unsigned package'
        Assert-Test ($metadata.rmskin.Version -ceq $Version) 'Metadata preserves safe version'
        Assert-Test ($metadata.rmskin.Author -ceq 'TaskbarX-Rust independent rewrite contributors') 'Independent rewrite author'
        Assert-Test ($metadata.rmskin.MinimumRainmeter -ceq '4.5.0') 'Valid minimum Rainmeter version'
        Assert-Test ($metadata.rmskin.MinimumWindows -ceq '10.0') 'Windows 10 minimum'
        $launcher = ConvertFrom-FixtureIni $rmskin["$skin/Launcher.ini"]
        Assert-Names @($launcher.Keys) @('Rainmeter', 'Metadata', 'OpenSettings') 'Only approved launcher sections, no measures'
        Assert-Names @($launcher.Rainmeter.Keys) @('Update', 'AccurateText') 'No load, refresh, update, close actions'
        Assert-Names @($launcher.Metadata.Keys) @('Name', 'Author', 'Information', 'Version', 'License') 'Only descriptive metadata'
        Assert-Names @($launcher.OpenSettings.Keys) @('Meter', 'Text', 'FontFace', 'FontSize', 'FontColor', 'SolidColor',
            'Padding', 'AntiAlias', 'MouseActionCursor', 'LeftMouseUpAction') 'Click-only meter, no implicit execution mechanism'
        Assert-Test ($launcher.OpenSettings.Meter -ceq 'String' -and $launcher.OpenSettings.Text -match "Settings.*$Architecture") 'Visible labeled click target'
        Assert-Test ($launcher.OpenSettings.FontColor -ceq '255,255,255,255' -and [int]$launcher.OpenSettings.FontSize -gt 0) 'Click target is not transparent or zero-sized'
        $action = $launcher.OpenSettings.LeftMouseUpAction
        Assert-Test ($action -ceq '["#@#TaskbarX-Rust\taskbarx-configurator.exe"]') 'Only explicitly clicking launches the quoted #@# configurator path'
        $target = $action.Substring(5, $action.Length - 7).Replace('\', '/')
        Assert-Test ($rmskin.ContainsKey("$skin/@Resources/$target")) 'Relative launcher target resolves inside bundled resources'
        $iniText = [Text.Encoding]::UTF8.GetString($rmskin["$skin/Launcher.ini"])
        Assert-Test ($iniText -notmatch '(?i)taskbarx\.exe|OnRefresh|OnUpdate|OnClose|RunCommand|!Execute|!CommandMeasure|@Include|Measure=') 'No engine execution, lifecycle actions, plugins, or includes'
    }
    Assert-Names @(Get-ChildItem -LiteralPath $Result.OutputDirectory -Force | ForEach-Object { $_.Name }) $assets 'Only requested archives are output assets; no loose EXEs, metadata or scratch files'
}

function New-FixtureJunction {
    param([string]$Link, [string]$Target)
    Assert-FixturePath $Link; Assert-FixturePath $Target
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Link))
    $targetArgument = $Target
    if ($PSVersionTable.PSVersion.Major -lt 6) { $targetArgument = [Management.Automation.WildcardPattern]::Escape($Target) }
    try {
        $null = New-Item -ItemType Junction -Path $Link -Target $targetArgument -ErrorAction Stop
        return $true
    } catch {
        $script:skipped.Add("Junction fixture unavailable: $($_.Exception.Message)")
        return $false
    }
}

try {
    # Refuse preexisting/reparse temp ancestry before creating this run's scope.
    Assert-Test (-not (Test-Path -LiteralPath $scratch)) 'Unique scratch directory does not already exist'
    $ancestor = [IO.Path]::GetDirectoryName($scratch)
    while ($ancestor) {
        $item = Get-Item -LiteralPath $ancestor -Force
        Assert-Test (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) 'Temporary fixture ancestry is not a reparse point'
        $ancestor = [IO.Path]::GetDirectoryName($ancestor)
    }
    [void][IO.Directory]::CreateDirectory($scratch)
    foreach ($path in @($production, $PSCommandPath)) {
        $tokens = $null; $errors = $null
        $null = [Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$errors)
        Assert-Test ($errors.Count -eq 0) "PowerShell syntax: $path"
    }
    $successful = @{}
    foreach ($architecture in @('x64', 'x86', 'arm64')) {
        $machine = @{ x64 = 0x8664; x86 = 0x014c; arm64 = 0xaa64 }[$architecture]
        $fixture = New-FixtureRepository $architecture $machine
        [IO.File]::SetAttributes((Join-Path $fixture.Root $noticeNames[1]), [IO.FileAttributes]::Hidden)
        foreach ($version in @('0.1.2', '1.2.3-rc.4+offline_7')) {
            $output = New-OutputPath
            $arguments = @{ Fixture = $fixture; Architecture = $architecture; Output = $output }
            if ($version -ne '0.1.2') {
                $arguments.Version = $version
                [void][IO.Directory]::CreateDirectory($output) # Explicit existing empty output is supported.
            }
            # Resolve literal relative inputs against an unrelated caller location;
            # legal/sample files still MUST come from the copied script's checkout.
            Push-Location -LiteralPath $scratch
            try {
                $arguments.Binary = $fixture.Binary.Substring($scratch.Length + 1)
                $arguments.Output = $output.Substring($scratch.Length + 1)
                $result = Invoke-CheckedPackage @arguments
            } finally { Pop-Location }
            Assert-Package $result $fixture $architecture $version
            Assert-PackageFailure $fixture 'must be empty' @{ Architecture = $architecture; Output = $output }
            $successful[$architecture] = $result
        }
    }

    # Valid PE headers must not bypass the distribution privacy gate.
    $leaky = New-FixtureRepository 'embedded-profile-path'
    $leakyBinary = Join-Path $leaky.Binary 'taskbarx.exe'
    $fakeHome = 'C:' + [char]92 + 'Users' + [char]92 + 'fixture-person' + [char]92 + 'source.rs'
    [IO.File]::WriteAllBytes($leakyBinary, [byte[]](
        [IO.File]::ReadAllBytes($leakyBinary) + [Text.Encoding]::ASCII.GetBytes($fakeHome)))
    Assert-PackageFailure $leaky 'Distribution privacy check failed'

    # Corrupt only fixture archives and independently reject every footer defect.
    $validRmskin = [IO.File]::ReadAllBytes($successful.x64.RmskinPath)
    foreach ($mode in @('length-low', 'length-high', 'flags', 'signature', 'nul', 'truncated-one', 'missing-footer', 'short', 'trailing-byte')) {
        $bad = [byte[]]$validRmskin.Clone()
        $footer = $bad.Length - 16
        switch ($mode) {
            'length-low' { $bad[$footer] = $bad[$footer] -bxor 1 }
            'length-high' { $bad[$footer + 7] = 128 }
            'flags' { $bad[$footer + 8] = 1 }
            'signature' { $bad[$footer + 9] = 0 }
            'nul' { $bad[$bad.Length - 1] = 1 }
            'truncated-one' { $bad = [byte[]]$bad[0..($bad.Length - 2)] }
            'missing-footer' { $bad = [byte[]]$bad[0..($footer - 1)] }
            'short' { $bad = [byte[]]@(82, 77, 83, 75, 73, 78, 0) }
            'trailing-byte' { $bad = [byte[]]($bad + [byte]0) }
        }
        $path = Join-Path $scratch "corrupt-$mode.rmskin"
        [IO.File]::WriteAllBytes($path, $bad)
        Assert-Throws { Read-TestArchive $path -Rmskin } 'Invalid RMSKIN footer'
    }

    # Both binary positions, every requested architecture, mixed pairs AND a pair
    # which agrees with itself but not the explicitly requested architecture.
    foreach ($architecture in @('x64', 'x86', 'arm64')) {
        $machine = @{ x64 = 0x8664; x86 = 0x014c; arm64 = 0xaa64 }[$architecture]
        $wrong = if ($architecture -eq 'x86') { 0x8664 } else { 0x014c }
        foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe', 'both')) {
            $fixture = New-FixtureRepository 'machine mismatch' $machine
            $names = @($name)
            if ($name -eq 'both') { $names = @('taskbarx.exe', 'taskbarx-configurator.exe') }
            foreach ($file in $names) { Write-FakePe (Join-Path $fixture.Binary $file) $wrong }
            $pattern = 'PE machine mismatch for ' + [regex]::Escape($names[0])
            Assert-PackageFailure $fixture $pattern @{ Architecture = $architecture }
        }
    }
    foreach ($name in @('taskbarx.exe', 'taskbarx-configurator.exe')) {
        foreach ($mode in @('empty', 'short-dos', 'bad-mz', 'negative-offset', 'small-offset', 'huge-offset',
            'beyond-eof', 'bad-pe', 'six-byte-pe', 'short-coff', 'optional-truncated', 'sections-truncated', 'unknown-machine')) {
            $fixture = New-FixtureRepository "$name $mode"
            $path = Join-Path $fixture.Binary $name
            $bytes = [IO.File]::ReadAllBytes($path)
            switch ($mode) {
                'empty' { $bytes = New-Object byte[] 0 }
                'short-dos' { $bytes = [byte[]]$bytes[0..62] }
                'bad-mz' { $bytes[0] = 0 }
                'negative-offset' { $bytes[63] = 128 }
                'small-offset' { $bytes[60] = 32 }
                'huge-offset' { $bytes[60] = 255; $bytes[61] = 255; $bytes[62] = 255; $bytes[63] = 127 }
                'beyond-eof' { $bytes[61] = 2 }
                'bad-pe' { $bytes[130] = 1 }
                'six-byte-pe' { $bytes = [byte[]]$bytes[0..133] }
                'short-coff' { $bytes = [byte[]]$bytes[0..150] }
                'optional-truncated' { $bytes[148] = 255; $bytes[149] = 255 }
                'sections-truncated' { $bytes[134] = 255; $bytes[135] = 255 }
                'unknown-machine' { $bytes[132] = 0; $bytes[133] = 0 }
            }
            [IO.File]::WriteAllBytes($path, $bytes)
            Assert-PackageFailure $fixture '(Invalid DOS|Invalid PE|truncated COFF|Truncated declared PE|PE machine mismatch)'
        }
    }
    foreach ($missing in @('taskbarx.exe', 'taskbarx-configurator.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'docs/settings.sample.json', 'docs/licenses/README.md')) {
        $fixture = New-FixtureRepository 'missing input'
        $parent = $fixture.Root
        if ($missing -like '*.exe') { $parent = $fixture.Binary }
        $path = Join-Path $parent $missing
        Assert-FixturePath $path
        Remove-Item -LiteralPath $path -Force
        Assert-PackageFailure $fixture 'Missing input file'
    }
    $fixture = New-FixtureRepository 'missing notices'
    foreach ($name in $noticeNames | Where-Object { $_ -ne 'docs/licenses/README.md' }) {
        $path = Join-Path $fixture.Root $name; Assert-FixturePath $path
        Remove-Item -LiteralPath $path -Force
    }
    Assert-PackageFailure $fixture 'Missing dependency license texts'
    $fixture = New-FixtureRepository 'binary is directory'
    $path = Join-Path $fixture.Binary 'taskbarx.exe'; Assert-FixturePath $path
    Remove-Item -LiteralPath $path
    [void][IO.Directory]::CreateDirectory($path)
    Assert-PackageFailure $fixture 'Missing input file'
    foreach ($mode in @('binary', 'invalid-utf8', 'empty')) {
        $fixture = New-FixtureRepository "invalid license $mode"
        $bytes = switch ($mode) { 'binary' { [byte[]]@(77,90,0,1) }; 'invalid-utf8' { [byte[]]@(255,255,255) }; 'empty' { New-Object byte[] 0 } }
        [IO.File]::WriteAllBytes((Join-Path $fixture.Root 'docs/licenses/anstream-1.0.0/LICENSE-MIT'), $bytes)
        Assert-PackageFailure $fixture '(Invalid license text|Unable to translate|fallback)'
    }

    $fixture = New-FixtureRepository 'invalid arguments and output'
    foreach ($architecture in @('arm32', 'AnyCPU', 'amd64', 'x128', '', 'x64;echo')) {
        Assert-PackageFailure $fixture '(ValidateSet|validation|cannot be validated|does not belong)' @{ Architecture = $architecture }
    }
    foreach ($version in @('', '.', '..', '../escape', '..\escape', 'a/b', 'a\b', 'a..b', '1.0.', ' 1.0', '1.0 ',
        "1.0`nLoad=evil", "1.0`r`n[rmskin]", "1.0`0bad", "1.0`tbad", "1.0$([char]127)",
        '1;evil', '1=evil', '1[evil]', '1"evil', '1%PATH%', '1|evil', '1&evil', '1:evil', ('a' * 49))) {
        Assert-PackageFailure $fixture '(Unsafe Version|ValidateNotNullOrEmpty|validation|empty)' @{ Version = $version }
    }
    foreach ($badPath in @('Env:PATH', 'C:relative', 'C:\unsafe:stream', '\\?\C:\unsafe', '..\escape', 'path\..\escape', 'output.', 'output ', 'NUL', "bad`npath")) {
        Assert-Throws { Invoke-CheckedPackage $fixture -Output $badPath } '(Unsafe filesystem path|filesystem provider)'
        Assert-Throws { Invoke-CheckedPackage $fixture -Binary $badPath } '(Unsafe filesystem path|filesystem provider)'
    }
    Assert-PackageFailure $fixture 'BinaryDirectory must be an existing directory' @{ Binary = (Join-Path $fixture.Case 'missing binary dir') }
    foreach ($output in @($fixture.Binary, (Join-Path $fixture.Binary 'nested output'), $fixture.Case)) {
        Assert-PackageFailure $fixture 'must not overlap' @{ Output = $output }
    }
    foreach ($hidden in @($false, $true)) {
        $output = New-OutputPath
        $sentinel = Join-Path $output 'existing sentinel.txt'
        Write-Fixture $sentinel 'DO NOT OVERWRITE OR CLEAR'
        if ($hidden) { [IO.File]::SetAttributes($sentinel, [IO.FileAttributes]::Hidden) }
        Assert-PackageFailure $fixture 'must be empty' @{ Output = $output }
    }
    $outputFile = Join-Path $scratch 'output is file.txt'
    Write-Fixture $outputFile 'DO NOT OVERWRITE'
    Assert-PackageFailure $fixture 'Output must be a directory' @{ Output = $outputFile }
    Assert-PackageFailure $fixture 'Path ancestor is not a directory' @{ Output = (Join-Path $outputFile 'child') }

    $hiddenDirectoryOutput = New-OutputPath
    $hiddenDirectory = Join-Path $hiddenDirectoryOutput 'hidden directory'
    [void][IO.Directory]::CreateDirectory($hiddenDirectory)
    [IO.File]::SetAttributes($hiddenDirectory, [IO.FileAttributes]::Hidden)
    Assert-PackageFailure $fixture 'must be empty' @{ Output = $hiddenDirectoryOutput }

    # Privilege-free Windows junctions exercise selected inputs and output ancestry.
    # Targets are exclusively fixtures. Snapshots verify no writes through links.
    $sourceLink = Join-Path $scratch 'linked binaries'
    if (New-FixtureJunction $sourceLink $fixture.Binary) {
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Binary = $sourceLink }
    }
    $parentLink = Join-Path $scratch 'linked input parent'
    if (New-FixtureJunction $parentLink $fixture.Case) {
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Binary = (Join-Path $parentLink ([IO.Path]::GetFileName($fixture.Binary))) }
    }
    $target = Join-Path $scratch 'junction output target'; [void][IO.Directory]::CreateDirectory($target)
    $outputLink = Join-Path $scratch 'linked output'
    if (New-FixtureJunction $outputLink $target) {
        $before = Get-Snapshot $target
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Output = $outputLink }
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Output = (Join-Path $outputLink 'new child') }
        Assert-Test ((Get-Snapshot $target) -ceq $before) 'Empty output reparse target remains empty'
    }
    $danglingTarget = Join-Path $scratch 'dangling junction target'
    [void][IO.Directory]::CreateDirectory($danglingTarget)
    $danglingLink = Join-Path $scratch 'dangling junction'
    if (New-FixtureJunction $danglingLink $danglingTarget) {
        Assert-FixturePath $danglingTarget
        # Nonrecursive deletion of only this verified, ordinary, empty fixture
        # target leaves a dangling reparse point; the link itself is never followed.
        Assert-Test (((Get-Item -LiteralPath $danglingTarget -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) 'Dangling-link fixture target is an ordinary directory'
        [IO.Directory]::Delete($danglingTarget, $false)
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Binary = $danglingLink }
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Output = $danglingLink }
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ Output = (Join-Path $danglingLink 'child') }
        Assert-Test (-not (Test-Path -LiteralPath $danglingTarget)) 'Dangling target was not recreated'
    }
    $linkedRepo = Join-Path $scratch 'linked checkout'
    if (New-FixtureJunction $linkedRepo $fixture.Root) {
        Assert-PackageFailure $fixture 'Reparse points are not permitted' @{ ScriptPath = (Join-Path $linkedRepo 'scripts/package-distribution.ps1') }
    }
    $noticeTarget = Join-Path $scratch 'notice target'
    Write-Fixture (Join-Path $noticeTarget 'license-mit') 'INERT NOTICE TARGET SENTINEL'
    $noticeLink = Join-Path $fixture.Root 'docs/licenses/windows-future-0.2.1'
    if (New-FixtureJunction $noticeLink $noticeTarget) {
        $before = Get-Snapshot $noticeTarget
        Assert-PackageFailure $fixture 'Reparse points are not permitted'
        Assert-Test ((Get-Snapshot $noticeTarget) -ceq $before) 'Selected notice reparse target unchanged'
    }
    # A file symlink may require administrator/developer-mode privileges. Try it,
    # but report the environment limitation instead of elevating/installing tools.
    $fixture = New-FixtureRepository 'file symlink'
    $link = Join-Path $fixture.Binary 'taskbarx.exe'; Assert-FixturePath $link
    $target = Join-Path $scratch 'symlink target.exe'
    Write-FakePe $target 0x8664
    Remove-Item -LiteralPath $link
    $symlinkCreated = $false
    try {
        $targetArgument = $target
        if ($PSVersionTable.PSVersion.Major -lt 6) { $targetArgument = [Management.Automation.WildcardPattern]::Escape($target) }
        $null = New-Item -ItemType SymbolicLink -Path $link -Target $targetArgument -ErrorAction Stop
        $symlinkCreated = $true
    } catch { $script:skipped.Add("File symlink fixture unavailable: $($_.Exception.Message)") }
    if ($symlinkCreated) {
        $before = Get-Hash ([IO.File]::ReadAllBytes($target))
        Assert-PackageFailure $fixture 'Reparse points are not permitted'
        Assert-Test ((Get-Hash ([IO.File]::ReadAllBytes($target))) -ceq $before) 'Selected binary symlink target unchanged'
        # A dangling selected reparse must be rejected too, not silently treated as
        # a normal missing file. Delete only the known fixture target, not the link.
        Assert-FixturePath $target; Remove-Item -LiteralPath $target
        Assert-PackageFailure $fixture 'Reparse points are not permitted'
    }

    Write-Host "PASS: $script:passed distribution assertions on PowerShell $($PSVersionTable.PSVersion)."
    foreach ($skip in $script:skipped) { Write-Host "SKIP: $skip" }
    Write-Host "INERT FIXTURES ONLY. No binaries/CMD/skins launched; no real-asset or Rainmeter runtime validation. Retained: $scratch"
} catch {
    Write-Host "FAIL after $script:passed assertions. Inert fixtures retained: $scratch"
    throw
}
