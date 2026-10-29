#Requires -Version 5.1
<#
.SYNOPSIS
Offline, read-only heuristic privacy check of a local file or directory.
.DESCRIPTION
Reads binary bytes (including inert or real EXEs), UTF-8/ASCII-compatible text,
and BOM-optional UTF-16LE at BOTH byte alignments. Additional ForbiddenText
strings are case-sensitive literals in those encodings, not regular expressions.
Home-path and credential heuristics are NOT proof that a release is secret-free.
No extraction, application execution, input assembly loading, or network access.

Directory traversal excludes only entries named .git (file or directory) and
directories named .worktrees, case-insensitively. These administrative exclusions
do NOT apply to a directly supplied input or ANY archive entry. Hidden files,
dist, target, EXEs, and archives remain in scope; .gitignore is never consulted.
Reparse points anywhere in the selected ancestry/scope are rejected, not followed
(excluded administrative entries are not visited). Use a stable, trusted local
directory: this is not a sandbox against concurrent hostile filesystem mutation.

Limits per invocation: 64 MiB per filesystem file, 64 MiB per expanded entry,
256 MiB cumulative filesystem bytes, 256 MiB cumulative decompressed bytes,
10,000 combined filesystem/archive entries, 4 archive levels, 64 directory levels.
Bytes are bounded WHILE reading, including when archive size metadata lies.
Whole bounded buffers avoid encoding/chunk-boundary misses. Text regexes have a
2-second timeout per rule/pass. Internal constants may be lowered in fixture
copies; there is no environment-variable or public limit-bypass switch.

Supports ordinary single-disk ZIP (stored/deflated), including nested payloads,
and real RMSKIN footers (UInt64 LE ZIP length, flags byte 0, RMSKIN plus NUL).
ZIP64, encryption, unsupported flags/methods, links/special files, ambiguous or
unsafe names, malformed framing/CRC, and known other archive formats fail closed.
ZIPs are recognized by extension or initial ZIP signature; RMSKIN also by footer.
Opaque binaries receive byte scanning, not executable parsing, disassembly, or
decryption; arbitrarily embedded/compressed executable resources are not unpacked.

Clean runs are silent. Failure writes only a sanitized relative/basename or nested
entry identifier and a fixed category to stderr, and exits 1. Deliberately uses
exit rather than an escaping PowerShell ErrorRecord (which could print host paths,
caller source, argument values, or raw exceptions). Check LASTEXITCODE when called
from another PowerShell script. Path is required but validated inside the script
to avoid parameter-validation messages echoing its value. Never dot-source this.
The caller's existing error history is preserved; scanner-internal error records
are removed so subsequently formatting $Error does not reveal failed IO paths.
.PARAMETER Path
Local literal filesystem file/directory; relative paths use the caller's location.
.PARAMETER ForbiddenText
Optional string array of additional literal strings. Omitted, null and empty
arrays are accepted; null/empty individual elements are rejected safely.
.EXAMPLE
& .\scripts\check-distribution-privacy.ps1 -Path '.\dist\candidate [review]'
if ($LASTEXITCODE -ne 0) { throw 'Distribution privacy check failed.' }
#>
param($Path = $null, $ForbiddenText = @())

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:previousErrors = @($Error)
$script:identifier = '[input]'
$script:category = 'inspection-failed'
$script:limits = @{
    InputBytes = 64MB; EntryBytes = 64MB; FileBytes = 256MB; ExpandedBytes = 256MB
    Entries = 10000; ArchiveDepth = 4; DirectoryDepth = 64
}
$script:counts = @{ FileBytes = [long]0; ExpandedBytes = [long]0; Entries = 0 }
$script:literals = @()

function Stop-Inspection {
    param([string]$Category)
    $script:category = $Category
    throw 'Inspection stopped.'
}

function Get-TextCategory {
    param([string]$Text)
    foreach ($literal in $script:literals) {
        if ($Text.IndexOf($literal, [StringComparison]::Ordinal) -ge 0) { return 'forbidden-text' }
    }
    foreach ($rule in $script:rules) {
        if ($rule.Regex.IsMatch($Text)) { return $rule.Category }
    }
    return ''
}

function Set-Identifier {
    param([string]$Name)
    # Do not attempt partial masking: names can themselves be secrets. Invalid,
    # absolute, overly long or non-ASCII identifiers are withheld in their entirety.
    $script:identifier = '[redacted]'
    if ($Name.Length -le 200 -and $Name -cmatch '\A[A-Za-z0-9_. /!\[\]()+@-]+\z' -and
        $Name -notmatch '(^|!/)[/\\]' -and -not (Get-TextCategory $Name)) {
        $script:identifier = $Name
    }
    $script:category = 'inspection-failed'
}

function Test-Text {
    param([string]$Text)
    $category = Get-TextCategory $Text
    if ($category) { Stop-Inspection $category }
}

function Test-Bytes {
    param([byte[]]$Bytes)
    Test-Text ([Text.Encoding]::UTF8.GetString($Bytes))
    for ($offset = 0; $offset -lt 2; $offset++) {
        $length = $Bytes.Length - $offset
        $length -= $length % 2
        if ($length -gt 0) { Test-Text ([Text.Encoding]::Unicode.GetString($Bytes, $offset, $length)) }
    }
}

function Add-EntryCount {
    $script:counts.Entries++
    if ($script:counts.Entries -gt $script:limits.Entries) { Stop-Inspection 'entry-count-limit' }
}

function Read-Bounded {
    param([IO.Stream]$Stream, [long]$Maximum, [string]$Counter)
    $memory = New-Object IO.MemoryStream
    try {
        $buffer = New-Object byte[] 65536
        while ($true) {
            $remaining = [Math]::Min($Maximum - $memory.Length, $script:limits[$Counter] - $script:counts[$Counter])
            $read = $Stream.Read($buffer, 0, [int][Math]::Min($buffer.Length, $remaining + 1))
            if ($read -eq 0) { break }
            if ($read -gt $remaining) { Stop-Inspection 'byte-limit' }
            $script:counts[$Counter] += $read
            $memory.Write($buffer, 0, $read)
        }
        return ,$memory.ToArray()
    } finally { $memory.Dispose() }
}

function Assert-EntryName {
    param([string]$Name)
    if (-not $Name -or $Name.Contains('\') -or $Name.StartsWith('/')) { Stop-Inspection 'unsafe-entry-path' }
    $parts = $Name.TrimEnd('/').Split('/')
    foreach ($part in $parts) {
        if (-not $part -or $part -in @('.', '..') -or $part -match '[\x00-\x1f\x7f-\x9f<>:"|?*]' -or
            $part -match '[. ]$' -or $part -match '^(?i:CON|PRN|AUX|NUL|CLOCK\$|CONIN\$|CONOUT\$|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(?:[ .]|$)') {
            Stop-Inspection 'unsafe-entry-path'
        }
    }
    if ($Name.EndsWith('//')) { Stop-Inspection 'unsafe-entry-path' }
}

function Get-LocalPath {
    if ($Path -isnot [string] -or [string]::IsNullOrWhiteSpace($Path) -or
        $Path -match '^[\\/]{2}|[\x00-\x1f\x7f<>"|?*]|::|^[A-Za-z]:($|[^\\/])') { Stop-Inspection 'invalid-input-path' }
    $spelling = $Path.Replace('\', '/') -replace '^[A-Za-z]:', ''
    foreach ($part in $spelling.Split('/')) {
        if ($part -eq '..' -or $part.Contains(':') -or ($part -ne '.' -and $part -match '[. ]$') -or
            $part -match '^(?i:CON|PRN|AUX|NUL|CLOCK\$|CONIN\$|CONOUT\$|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(?:[ .]|$)') { Stop-Inspection 'invalid-input-path' }
    }
    $provider = $null; $drive = $null
    $full = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path, [ref]$provider, [ref]$drive)
    if ($provider.Name -ne 'FileSystem' -or $full -notmatch '^[A-Za-z]:[\\/]' -or
        ($null -ne $drive -and $drive.DisplayRoot -match '^[\\/]{2}')) { Stop-Inspection 'nonlocal-input' }
    $full = [IO.Path]::GetFullPath($full)
    $volume = New-Object IO.DriveInfo([IO.Path]::GetPathRoot($full))
    if ($volume.DriveType -notin @([IO.DriveType]::Fixed, [IO.DriveType]::Removable, [IO.DriveType]::Ram, [IO.DriveType]::CDRom)) { Stop-Inspection 'nonlocal-input' }
    if ($full.Length -gt 3) { $full = $full.TrimEnd('\', '/') }
    return $full
}

function Assert-NormalAncestry {
    param([string]$FullPath)
    $ancestors = New-Object 'Collections.Generic.List[string]'
    for ($current = $FullPath; $current; $current = [IO.Path]::GetDirectoryName($current)) { $ancestors.Add($current) }
    # Root first: never even query a descendant of an untrusted junction.
    for ($i = $ancestors.Count - 1; $i -ge 0; $i--) {
        if ([IO.File]::GetAttributes($ancestors[$i]) -band [IO.FileAttributes]::ReparsePoint) { Stop-Inspection 'filesystem-link' }
    }
}

function Get-U16 { param([byte[]]$Bytes, [int]$Offset) return [BitConverter]::ToUInt16($Bytes, $Offset) }
function Get-U32 { param([byte[]]$Bytes, [int]$Offset) return [BitConverter]::ToUInt32($Bytes, $Offset) }

function Get-ZipRecords {
    param([byte[]]$Bytes, [int]$Length)
    # A deliberately narrow ZIP preflight bounds metadata BEFORE ZipArchive loads
    # entries, rejects unsupported features, and checks local/central framing.
    $end = -1
    for ($i = $Length - 22; $i -ge [Math]::Max(0, $Length - 65557); $i--) {
        if ((Get-U32 $Bytes $i) -eq 0x06054b50 -and $i + 22 + (Get-U16 $Bytes ($i + 20)) -eq $Length) { $end = $i; break }
    }
    if ($end -lt 0) { Stop-Inspection 'malformed-archive' }
    $count = Get-U16 $Bytes ($end + 10)
    [long]$size = Get-U32 $Bytes ($end + 12)
    [long]$offset = Get-U32 $Bytes ($end + 16)
    if ((Get-U32 $Bytes ($end + 4)) -ne 0 -or (Get-U16 $Bytes ($end + 8)) -ne $count -or
        $count -eq 65535 -or $size -eq 4294967295 -or $offset -eq 4294967295) { Stop-Inspection 'unsupported-archive' }
    if ($count -gt $script:limits.Entries - $script:counts.Entries) { Stop-Inspection 'entry-count-limit' }
    if ($offset + $size -ne $end) { Stop-Inspection 'malformed-archive' }
    $records = New-Object 'Collections.Generic.List[object]'
    $parent = $script:identifier
    $cursor = $offset
    for ($i = 0; $i -lt $count; $i++) {
        if ($cursor + 46 -gt $end -or (Get-U32 $Bytes $cursor) -ne 0x02014b50) { Stop-Inspection 'malformed-archive' }
        $flags = Get-U16 $Bytes ($cursor + 8); $method = Get-U16 $Bytes ($cursor + 10)
        [long]$compressed = Get-U32 $Bytes ($cursor + 20); [long]$expanded = Get-U32 $Bytes ($cursor + 24)
        $nameLength = Get-U16 $Bytes ($cursor + 28)
        $next = $cursor + 46 + $nameLength + (Get-U16 $Bytes ($cursor + 30)) + (Get-U16 $Bytes ($cursor + 32))
        [long]$local = Get-U32 $Bytes ($cursor + 42)
        if (($flags -band 0xf7f1) -ne 0 -or $method -notin @(0, 8) -or
            (Get-U16 $Bytes ($cursor + 34)) -ne 0 -or $compressed -eq 4294967295 -or $expanded -eq 4294967295 -or $local -eq 4294967295) { Stop-Inspection 'unsupported-archive' }
        # Only deflate option bits 1/2, descriptor bit 3, and UTF-8 bit 11 allowed.
        if ($next -gt $end -or $local + 30 -gt $offset -or (Get-U32 $Bytes $local) -ne 0x04034b50) { Stop-Inspection 'malformed-archive' }
        Set-Identifier ($parent + '!/[entry-' + $i + ']')
        $encoding = [Text.Encoding]::GetEncoding(437)
        if ($flags -band 0x800) { $encoding = New-Object Text.UTF8Encoding($false, $true) }
        $name = $encoding.GetString($Bytes, $cursor + 46, $nameLength)
        Assert-EntryName $name
        Set-Identifier ($parent + '!/' + $name)
        Test-Text $name
        $localName = Get-U16 $Bytes ($local + 26)
        $data = $local + 30 + $localName + (Get-U16 $Bytes ($local + 28))
        if ($nameLength -ne $localName -or $data + $compressed -gt $offset -or
            (Get-U16 $Bytes ($local + 6)) -ne $flags -or (Get-U16 $Bytes ($local + 8)) -ne $method) { Stop-Inspection 'malformed-archive' }
        for ($j = 0; $j -lt $nameLength; $j++) {
            if ($Bytes[$local + 30 + $j] -ne $Bytes[$cursor + 46 + $j]) { Stop-Inspection 'malformed-archive' }
        }
        $crc = Get-U32 $Bytes ($cursor + 16)
        $finish = $data + $compressed
        $check = $local + 14
        if ($flags -band 8) {
            $check = $finish
            if ($check + 4 -le $offset -and (Get-U32 $Bytes $check) -eq 0x08074b50) { $check += 4 }
            $finish = $check + 12
        }
        if ($finish -gt $offset -or (Get-U32 $Bytes $check) -ne $crc -or
            (Get-U32 $Bytes ($check + 4)) -ne $compressed -or (Get-U32 $Bytes ($check + 8)) -ne $expanded) { Stop-Inspection 'malformed-archive' }
        $records.Add([pscustomobject]@{ Local = $local; End = $finish; Crc = $crc; Expanded = $expanded })
        $cursor = $next
    }
    if ($cursor -ne $end) { Stop-Inspection 'malformed-archive' }
    [long]$boundary = 0
    foreach ($record in @($records | Sort-Object Local)) {
        if ($record.Local -ne $boundary) { Stop-Inspection 'unsupported-archive' }
        $boundary = $record.End
    }
    if ($boundary -ne $offset) { Stop-Inspection 'malformed-archive' }
    return ,$records
}

function Test-Archive {
    param([byte[]]$Bytes, [int]$Length, [int]$Depth, [string]$Parent)
    if ($Depth -gt $script:limits.ArchiveDepth) { Stop-Inspection 'archive-depth-limit' }
    $records = Get-ZipRecords $Bytes $Length
    $memory = New-Object IO.MemoryStream($Bytes, 0, $Length, $false, $false)
    try {
        $zip = New-Object IO.Compression.ZipArchive($memory, [IO.Compression.ZipArchiveMode]::Read, $true)
        try {
            if ($zip.Entries.Count -ne $records.Count) { Stop-Inspection 'malformed-archive' }
            $seen = @{}; $index = 0
            foreach ($entry in $zip.Entries) {
                # Never emit an unsafe name, even when path validation itself fails.
                Set-Identifier ($Parent + '!/[entry-' + $index + ']')
                Add-EntryCount
                Assert-EntryName $entry.FullName
                Set-Identifier ($Parent + '!/' + $entry.FullName)
                Test-Text $entry.FullName
                $key = $entry.FullName.TrimEnd('/')
                if ($seen.ContainsKey($key)) { Stop-Inspection 'ambiguous-entry-path' }
                $seen[$key] = $true
                $kind = ($entry.ExternalAttributes -shr 16) -band 0xf000
                if (($entry.ExternalAttributes -band 0x400) -ne 0 -or $kind -notin @(0, 0x8000, 0x4000)) { Stop-Inspection 'archive-link-or-special' }
                $directory = $entry.FullName.EndsWith('/')
                if (($kind -eq 0x4000 -or ($entry.ExternalAttributes -band 0x10)) -and -not $directory) { Stop-Inspection 'malformed-archive' }
                $stream = $entry.Open()
                try { $payload = Read-Bounded $stream $script:limits.EntryBytes 'ExpandedBytes' }
                finally { $stream.Dispose() }
                if ($payload.Length -ne $records[$index].Expanded -or [DistributionPrivacy.Crc32]::Compute($payload) -ne $records[$index].Crc -or
                    ($directory -and $payload.Length -ne 0)) { Stop-Inspection 'malformed-archive' }
                Test-Payload $payload $entry.FullName ($Depth + 1) $script:identifier
                $index++
            }
        } finally { $zip.Dispose() }
    } finally { $memory.Dispose() }
}

function Test-Payload {
    param([byte[]]$Bytes, [string]$Name, [int]$Depth, [string]$Identifier)
    Set-Identifier $Identifier
    $extension = [IO.Path]::GetExtension($Name).ToLowerInvariant()
    $length = $Bytes.Length
    $footer = $length -ge 16 -and [Text.Encoding]::ASCII.GetString($Bytes, $length - 7, 7) -ceq "RMSKIN`0"
    $isZip = $length -ge 4 -and (Get-U32 $Bytes 0) -in @(0x04034b50, 0x06054b50, 0x08074b50)
    if ($extension -eq '.rmskin' -or $footer) {
        if (-not $footer -or [BitConverter]::ToUInt64($Bytes, $length - 16) -ne ($length - 16)) { Stop-Inspection 'malformed-rmskin' }
        if ($Bytes[$length - 8] -ne 0) { Stop-Inspection 'unsupported-rmskin' }
        $length -= 16; $isZip = $true
    }
    if ($isZip -or $extension -eq '.zip') {
        # Validate paths before processing entries. Also scan raw bytes afterward
        # to cover archive comments, metadata and stored binary strings.
        Test-Archive $Bytes $length $Depth $script:identifier
        Set-Identifier $Identifier
    } elseif ($extension -in @('.7z', '.rar', '.tar', '.gz', '.tgz', '.bz2', '.xz', '.cab', '.iso', '.zst') -or
        ($Bytes.Length -ge 2 -and $Bytes[0] -eq 0x1f -and $Bytes[1] -eq 0x8b) -or
        ($Bytes.Length -ge 6 -and [BitConverter]::ToString($Bytes, 0, 6) -in @('37-7A-BC-AF-27-1C', 'FD-37-7A-58-5A-00')) -or
        ($Bytes.Length -ge 4 -and [Text.Encoding]::ASCII.GetString($Bytes, 0, 4) -in @('Rar!', 'MSCF')) -or
        ($Bytes.Length -ge 3 -and [Text.Encoding]::ASCII.GetString($Bytes, 0, 3) -ceq 'BZh') -or
        ($Bytes.Length -ge 262 -and [Text.Encoding]::ASCII.GetString($Bytes, 257, 5) -ceq 'ustar')) { Stop-Inspection 'unsupported-archive' }
    Test-Bytes $Bytes
}

function Test-FilesystemItem {
    param([string]$FullPath, [string]$Relative, [int]$Depth = 0)
    Set-Identifier $Relative
    $attributes = [IO.File]::GetAttributes($FullPath)
    if ($attributes -band [IO.FileAttributes]::ReparsePoint) { Stop-Inspection 'filesystem-link' }
    Test-Text $Relative
    if ($attributes -band [IO.FileAttributes]::Directory) {
        if ($Depth -gt $script:limits.DirectoryDepth) { Stop-Inspection 'directory-depth-limit' }
        foreach ($child in [IO.Directory]::EnumerateFileSystemEntries($FullPath)) {
            Set-Identifier $Relative
            Add-EntryCount
            $name = [IO.Path]::GetFileName($child)
            if ($name -ieq '.git') { continue }
            if ($name -ieq '.worktrees' -and ([IO.File]::GetAttributes($child) -band [IO.FileAttributes]::Directory)) { continue }
            $next = $name
            if ($Relative -ne '[directory]') { $next = $Relative + '/' + $name }
            Test-FilesystemItem $child $next ($Depth + 1)
        }
    } else {
        $stream = [IO.File]::Open($FullPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        try { $bytes = Read-Bounded $stream $script:limits.InputBytes 'FileBytes' }
        finally { $stream.Dispose() }
        Test-Payload $bytes ([IO.Path]::GetFileName($FullPath)) 1 $Relative
    }
}

try {
    if ($args.Count -gt 0) { Stop-Inspection 'invalid-arguments' }
    if ($null -ne $ForbiddenText) {
        foreach ($value in $ForbiddenText) {
            if ($value -isnot [string] -or $value.Length -eq 0) { Stop-Inspection 'invalid-forbidden-text' }
            $script:literals += $value
        }
    }
    $patterns = [ordered]@{
        'home-path' = '(?i:[A-Z]:[\\/]Users[\\/])|(?<![A-Za-z0-9_./:\\%~-])/(?:Users|home)/'
        'github-token' = '(?<![A-Za-z0-9_])(?:gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{60,255})(?![A-Za-z0-9_])'
        'openai-key' = '(?<![A-Za-z0-9_-])sk-(?:[A-Za-z0-9]{48}|(?:proj|svcacct)-[A-Za-z0-9_-]{40,240})(?![A-Za-z0-9_-])'
        'aws-access-key' = '(?<![A-Za-z0-9])(?:AKIA|ASIA)[A-Z0-9]{16}(?![A-Za-z0-9])'
        'aws-secret-key' = '(?i:\b(?:aws[_ -]?)?secret[_ -]?access[_ -]?key\b)["'']?\s{0,8}[:=]\s{0,8}["'']?[A-Za-z0-9/+=]{40}(?![A-Za-z0-9/+=])'
        'pem-private-key' = '-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----'
    }
    $script:rules = @(foreach ($key in $patterns.Keys) {
        [pscustomobject]@{ Category = $key; Regex = (New-Object Text.RegularExpressions.Regex($patterns[$key], [Text.RegularExpressions.RegexOptions]::CultureInvariant, [TimeSpan]::FromSeconds(2))) }
    })
    Add-Type -AssemblyName System.IO.Compression
    # Trusted, fixed helper source only, NOT code or assemblies from scan inputs.
    # .NET Framework ZipArchive does not verify entry CRCs; a small managed loop
    # avoids an impractically slow per-byte PowerShell CRC loop on release EXEs.
    if ($null -eq ('DistributionPrivacy.Crc32' -as [type])) {
        Add-Type -TypeDefinition @'
namespace DistributionPrivacy {
    public static class Crc32 {
        private static readonly uint[] Table = MakeTable();
        private static uint[] MakeTable() {
            var table = new uint[256];
            for (uint i = 0; i < table.Length; i++) {
                uint c = i;
                for (int b = 0; b < 8; b++) c = (c & 1) != 0 ? 0xedb88320U ^ (c >> 1) : c >> 1;
                table[i] = c;
            }
            return table;
        }
        public static uint Compute(byte[] bytes) {
            uint crc = 0xffffffffU;
            foreach (byte b in bytes) crc = Table[(crc ^ b) & 255] ^ (crc >> 8);
            return crc ^ 0xffffffffU;
        }
    }
}
'@
    }
    $full = Get-LocalPath
    Assert-NormalAncestry $full
    $name = [IO.Path]::GetFileName($full)
    if ([IO.File]::GetAttributes($full) -band [IO.FileAttributes]::Directory) { $name = '[directory]' }
    Test-FilesystemItem $full $name
} catch {
    # Do not rethrow, Write-Error, format $_, or attach it as an inner exception.
    [Console]::Error.WriteLine(('{0}: {1}' -f $script:identifier, $script:category))
    exit 1
} finally {
    # Caught .NET/PowerShell failures still enter $Error. Restore the pre-call
    # records (including order) rather than leaving raw input paths in history.
    $Error.Clear()
    foreach ($record in $script:previousErrors) { [void]$Error.Add($record) }
}
exit 0
