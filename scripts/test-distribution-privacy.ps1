#Requires -Version 5.1
<#
.SYNOPSIS
Offline privacy scanner tests; generated inert bytes only, never application code.
.DESCRIPTION
Creates a fresh GUID-named temporary directory, retains ALL fixtures and prior
runs, and never extracts archives, builds, installs, downloads, or runs inputs.
Only the scanner (and lowered-limit copies of that trusted script) is invoked.
Fixtures include fictional credential shapes assembled at runtime. No operator
credentials/private files are read. Diagnostics and assertion failures never
print expected/actual sensitive values or absolute fixture paths.
.EXAMPLE
powershell.exe -NoProfile -File .\scripts\test-distribution-privacy.ps1
#>
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$script:passed = 0; $script:cases = 0; $script:nextFile = 0; $script:skipped = 0
$script:stage = 'initialization'
$production = Join-Path $PSScriptRoot 'check-distribution-privacy.ps1'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('TaskbarX privacy [fixtures] ' + [Guid]::NewGuid().ToString('N'))
$script:sensitive = New-Object 'Collections.Generic.List[string]'
$script:sensitive.Add($scratch)
$script:sensitive.Add([IO.Path]::GetTempPath().TrimEnd('\', '/'))

function Assert-Test {
    param([bool]$Condition, [string]$Label)
    if (-not $Condition) { $script:stage = $Label; throw 'Fixture assertion failed.' }
    $script:passed++
}

function Assert-FixturePath {
    param([string]$Path)
    Assert-Test ([IO.Path]::GetFullPath($Path).StartsWith($scratch + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) 'fixture containment'
}

function New-BytesFile {
    param([byte[]]$Bytes, [string]$Extension = '.bin', [string]$Name = '')
    $script:nextFile++
    $directory = Join-Path $scratch ('case-{0:D4} [literal]' -f $script:nextFile)
    [void][IO.Directory]::CreateDirectory($directory)
    if (-not $Name) { $Name = 'payload [inert]' + $Extension }
    $path = Join-Path $directory $Name
    Write-Bytes $path $Bytes
    return $path
}

function Write-Bytes {
    param([string]$Path, [byte[]]$Bytes)
    Assert-FixturePath $Path
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($Path))
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($Bytes, 0, $Bytes.Length) } finally { $stream.Dispose() }
}

function Get-Encoded {
    param([string]$Text, [string]$Encoding = 'ascii')
    switch ($Encoding) {
        'ascii' { return ,([Text.Encoding]::ASCII.GetBytes($Text)) }
        'utf8' { return ,([Text.Encoding]::UTF8.GetBytes($Text)) }
        'wide' { return ,([Text.Encoding]::Unicode.GetBytes($Text)) }
        'bom' { return ,([byte[]](@(255, 254) + [Text.Encoding]::Unicode.GetBytes($Text))) }
        'odd' { return ,([byte[]](@(123) + [Text.Encoding]::Unicode.GetBytes($Text))) }
        'odd-bom' { return ,([byte[]](@(123, 255, 254) + [Text.Encoding]::Unicode.GetBytes($Text))) }
    }
    throw 'Unknown fixture encoding.'
}

function Get-Snapshot {
    param([string]$Directory = $scratch, [string]$Prefix = '')
    foreach ($item in @(Get-ChildItem -LiteralPath $Directory -Force | Sort-Object Name)) {
        $relative = $Prefix + $item.Name
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            'LINK|' + $relative
        } elseif ($item.PSIsContainer) {
            'DIR|' + $relative
            Get-Snapshot $item.FullName ($relative + '/')
        } else {
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $hash = [Convert]::ToBase64String($sha.ComputeHash([IO.File]::ReadAllBytes($item.FullName))) }
            finally { $sha.Dispose() }
            '{0}|{1}|{2}|{3}' -f $relative, $hash, $item.LastWriteTimeUtc.Ticks, $item.Attributes
        }
    }
}

function Assert-SafeOutput {
    param([string]$Text)
    foreach ($value in $script:sensitive) {
        Assert-Test ($Text.IndexOf($value, [StringComparison]::OrdinalIgnoreCase) -lt 0) 'diagnostic redaction'
    }
    Assert-Test ($Text -notmatch '(?i)Exception|At line|CategoryInfo|FullyQualifiedErrorId|System\.IO|stack trace|access.*denied|central directory|End of Central') 'no raw errors'
}

function Invoke-Scan {
    param($InputPath, [string]$Category = '', [hashtable]$Options = @{}, [string]$Scanner = $production)
    $script:cases++
    $before = @(Get-Snapshot) -join "`n"
    $arguments = @{ Path = $InputPath } + $Options
    $stderr = New-Object IO.StringWriter
    $stdout = New-Object IO.StringWriter
    $oldErr = [Console]::Error; $oldOut = [Console]::Out
    $escaped = $false; $code = -1; $output = @()
    $priorErrors = @($Error)
    try {
        [Console]::SetError($stderr); [Console]::SetOut($stdout)
        try {
            $global:LASTEXITCODE = -1
            $output = @(& $Scanner @arguments *>&1)
            $code = $global:LASTEXITCODE
        } catch { $escaped = $true }
    } finally { [Console]::SetError($oldErr); [Console]::SetOut($oldOut) }
    try { $text = $stdout.ToString() + $stderr.ToString() + ($output | Out-String) }
    finally { $stderr.Dispose(); $stdout.Dispose() }
    Assert-Test (-not $escaped) 'no escaping PowerShell error record'
    Assert-Test ($Error.Count -eq $priorErrors.Count) 'no raw scanner error history'
    for ($i = 0; $i -lt $priorErrors.Count; $i++) {
        Assert-Test ([object]::ReferenceEquals($Error[$i], $priorErrors[$i])) 'caller error history preserved'
    }
    Assert-SafeOutput $text
    Assert-Test ((@(Get-Snapshot) -join "`n") -ceq $before) 'inputs unchanged and no extraction'
    if ($Category) {
        Assert-Test ($code -ne 0) 'failure exit status'
        Assert-Test ($text.Trim() -cmatch ('\A[A-Za-z0-9_. /!\[\]()+@-]+: ' + [regex]::Escape($Category) + '\z')) 'expected safe failure category'
    } else {
        Assert-Test ($code -eq 0) 'clean exit status'
        Assert-Test ([string]::IsNullOrWhiteSpace($text)) 'silent clean scan'
    }
}

function New-ZipBytes {
    param([object[]]$Members, [switch]$Stored)
    $memory = New-Object IO.MemoryStream
    try {
        $zip = New-Object IO.Compression.ZipArchive($memory, [IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            foreach ($member in $Members) {
                $level = [IO.Compression.CompressionLevel]::Optimal
                if ($Stored) { $level = [IO.Compression.CompressionLevel]::NoCompression }
                # Framework writers reject some control characters before an
                # unsafe-name fixture can be built. Patch those name bytes in
                # both headers AFTER closing the ZIP, without changing lengths.
                $writerName = $member.Name -replace '[\x00-\x1f<>"|]', 'x'
                $entry = $zip.CreateEntry($writerName, $level)
                if ($member.ContainsKey('Attributes')) { $entry.ExternalAttributes = $member.Attributes }
                $stream = $entry.Open()
                try { $stream.Write($member.Bytes, 0, $member.Bytes.Length) } finally { $stream.Dispose() }
            }
        } finally { $zip.Dispose() }
        $bytes = $memory.ToArray()
        $central = [int][BitConverter]::ToUInt32($bytes, $bytes.Length - 6)
        foreach ($member in $Members) {
            $local = [int][BitConverter]::ToUInt32($bytes, $central + 42)
            $length = [BitConverter]::ToUInt16($bytes, $central + 28)
            $original = [Text.Encoding]::UTF8.GetBytes($member.Name)
            Assert-Test ($original.Length -eq $length) 'fixture name byte length'
            [Array]::Copy($original, 0, $bytes, $central + 46, $length)
            [Array]::Copy($original, 0, $bytes, $local + 30, $length)
            $central += 46 + $length + [BitConverter]::ToUInt16($bytes, $central + 30) + [BitConverter]::ToUInt16($bytes, $central + 32)
        }
        return ,$bytes
    } finally { $memory.Dispose() }
}

function Add-RmskinFooter {
    param([byte[]]$Bytes)
    # Independent LE footer construction; no production reader/writer reused.
    $footer = New-Object byte[] 16
    [long]$size = $Bytes.Length
    for ($i = 0; $i -lt 8; $i++) { $footer[$i] = [byte]($size -band 255); $size = $size -shr 8 }
    [Array]::Copy([byte[]]@(82,77,83,75,73,78,0), 0, $footer, 9, 7)
    return ,([byte[]]($Bytes + $footer))
}

function New-LimitedScanner {
    param([hashtable]$Limits)
    $text = [IO.File]::ReadAllText($production)
    $defaults = @{ InputBytes = '64MB'; EntryBytes = '64MB'; FileBytes = '256MB'; ExpandedBytes = '256MB'; Entries = '10000'; ArchiveDepth = '4'; DirectoryDepth = '64' }
    foreach ($key in $Limits.Keys) {
        $old = $key + ' = ' + $defaults[$key]
        Assert-Test ($text.Contains($old)) 'lowered fixture limit anchor'
        $text = $text.Replace($old, ($key + ' = ' + $Limits[$key]))
    }
    return New-BytesFile ([Text.Encoding]::UTF8.GetBytes($text)) '.ps1' 'limited-scanner.ps1'
}

function Test-ChildProcess {
    param([string]$InputPath, [string]$Category = '', [switch]$OmitPath, [switch]$SkipSnapshot)
    # Independently exercise -File and OS exit status/stderr (not just & semantics).
    # Paths here contain no quotes; sensitive content is in the inert input only.
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = Join-Path $PSHOME 'powershell.exe'
    $info.Arguments = '-NoProfile -NonInteractive -File "' + $production + '"'
    if (-not $OmitPath) { $info.Arguments += ' -Path "' + $InputPath + '"' }
    $info.UseShellExecute = $false; $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true; $info.RedirectStandardError = $true
    $before = ''
    if (-not $SkipSnapshot) { $before = @(Get-Snapshot) -join "`n" }
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $info
    try {
        [void]$process.Start()
        $outTask = $process.StandardOutput.ReadToEndAsync()
        $errTask = $process.StandardError.ReadToEndAsync()
        Assert-Test ($process.WaitForExit(30000)) 'child completes within bound'
        $text = $outTask.Result + $errTask.Result
        Assert-SafeOutput $text
        if (-not $SkipSnapshot) { Assert-Test ((@(Get-Snapshot) -join "`n") -ceq $before) 'child leaves all inputs unchanged' }
        if ($Category) {
            Assert-Test ($process.ExitCode -ne 0 -and $text.Trim().EndsWith(': ' + $Category)) 'child failure status and category'
        } else { Assert-Test ($process.ExitCode -eq 0 -and -not $text) 'child clean status and silence' }
    } finally { $process.Dispose() }
}

try {
    Add-Type -AssemblyName System.IO.Compression
    Assert-Test (-not (Test-Path -LiteralPath $scratch)) 'fresh GUID fixture directory'
    for ($ancestor = [IO.Path]::GetDirectoryName($scratch); $ancestor; $ancestor = [IO.Path]::GetDirectoryName($ancestor)) {
        Assert-Test (([IO.File]::GetAttributes($ancestor) -band [IO.FileAttributes]::ReparsePoint) -eq 0) 'ordinary temporary ancestry'
    }
    [void][IO.Directory]::CreateDirectory($scratch)
    foreach ($file in @($production, $PSCommandPath)) {
        $tokens = $null; $errors = $null
        $null = [Management.Automation.Language.Parser]::ParseFile($file, [ref]$tokens, [ref]$errors)
        Assert-Test ($errors.Count -eq 0) 'PowerShell parser'
    }

    # Fictional values only; committed source contains no complete secret shape.
    $fictional = 'fixture-person-' + [Guid]::NewGuid().ToString('N')
    $homes = @(
        ('C:' + '\' + 'Users' + '\' + $fictional + '\source.rs'),
        ('C:' + '/' + 'Users' + '/' + $fictional + '/source.rs'),
        ('/' + 'Users' + '/' + $fictional + '/source.rs'),
        ('/' + 'home' + '/' + $fictional + '/source.rs')
    )
    $secrets = @(
        @{ Text = ('gh' + 'p_' + ('aB3' * 12)); Category = 'github-token' },
        @{ Text = ('github_' + 'pat_' + ('aB3_' * 20) + 'AB'); Category = 'github-token' },
        @{ Text = ('s' + 'k-' + ('aB3' * 16)); Category = 'openai-key' },
        @{ Text = ('s' + 'k-proj-' + ('Ab3_' * 40)); Category = 'openai-key' },
        @{ Text = ('s' + 'k-svcacct-' + ('Ab3_' * 40)); Category = 'openai-key' },
        @{ Text = ('AK' + 'IA' + ('A3' * 8)); Category = 'aws-access-key' },
        @{ Text = ('AS' + 'IA' + ('B4' * 8)); Category = 'aws-access-key' },
        @{ Text = ('aws_secret_access_key = "' + ('Ab3/' * 10) + '"'); Category = 'aws-secret-key' },
        @{ Text = ('"SecretAccessKey": "' + ('De6+' * 10) + '"'); Category = 'aws-secret-key' }
    )
    foreach ($variant in @('', 'RSA ', 'EC ', 'DSA ', 'OPENSSH ', 'ENCRYPTED ')) {
        $secrets += @{ Text = ('-----BEGIN ' + $variant + 'PRIVATE' + ' KEY-----'); Category = 'pem-private-key' }
    }
    foreach ($value in $homes + @($secrets | ForEach-Object { $_.Text })) { $script:sensitive.Add($value) }
    $script:sensitive.Add($fictional)
    $script:sensitive.Add(('Ab3/' * 10))
    $script:sensitive.Add(('De6+' * 10))
    $clean = 'INERT ONLY. https://example.invalid/Users/docs https://example.invalid/home/docs ' +
        'Users home github secret password sk-brief ' + [Guid]::NewGuid().ToString() + ' ' + ('0123456789abcdef' * 4) +
        ' -----BEGIN PUBLIC KEY----- ABCDEF0123456789 -----END PUBLIC KEY----- ' +
        '-----BEGIN RSA PUBLIC KEY----- ABCDEF -----END RSA PUBLIC KEY----- ' + ('Ab3/' * 10)
    $cleanFile = New-BytesFile (Get-Encoded $clean) '.txt'
    Invoke-Scan $cleanFile
    Invoke-Scan (New-BytesFile ([byte[]]@(77,90,0,255,1,2,3)) '.exe')
    Invoke-Scan (New-BytesFile (New-Object byte[] 0))
    Invoke-Scan ([IO.Path]::GetDirectoryName($cleanFile))
    foreach ($encoding in @('wide', 'bom', 'odd', 'odd-bom', 'utf8')) { Invoke-Scan (New-BytesFile (Get-Encoded $clean $encoding)) }
    Invoke-Scan $cleanFile -Options @{ ForbiddenText = @() }
    Invoke-Scan $cleanFile -Options @{ ForbiddenText = $null }
    Invoke-Scan $cleanFile 'invalid-forbidden-text' @{ ForbiddenText = @('') }
    Invoke-Scan $cleanFile 'invalid-forbidden-text' @{ ForbiddenText = @('valid', $null) }
    Invoke-Scan $cleanFile 'invalid-forbidden-text' @{ ForbiddenText = @(42) }
    Invoke-Scan $null 'invalid-input-path'
    Invoke-Scan '' 'invalid-input-path'
    Invoke-Scan @('not', 'a-path') 'invalid-input-path'

    foreach ($homePath in $homes) {
        foreach ($encoding in @('ascii', 'wide', 'bom', 'odd', 'odd-bom')) {
            Invoke-Scan (New-BytesFile (Get-Encoded ("`0" + $homePath + "`0") $encoding) '.exe') 'home-path'
        }
    }
    Invoke-Scan (New-BytesFile (Get-Encoded $homes[0].ToLowerInvariant())) 'home-path'
    foreach ($secret in $secrets) {
        foreach ($encoding in @('ascii', 'odd')) { Invoke-Scan (New-BytesFile (Get-Encoded $secret.Text $encoding)) $secret.Category }
    }
    foreach ($prefix in @('o', 'u', 's', 'r')) {
        $value = 'gh' + $prefix + '_' + ('Q1x' * 12)
        $script:sensitive.Add($value)
        Invoke-Scan (New-BytesFile (Get-Encoded $value)) 'github-token'
    }
    $literals = @('literal[one].*+?', 'second(x)|$^', ('unicode-' + [char]0x03a9))
    foreach ($literal in $literals) { $script:sensitive.Add($literal) }
    foreach ($literal in $literals) {
        foreach ($encoding in @('utf8', 'wide', 'bom', 'odd')) {
            Invoke-Scan (New-BytesFile (Get-Encoded $literal $encoding)) 'forbidden-text' @{ ForbiddenText = $literals }
        }
    }
    Invoke-Scan (New-BytesFile (Get-Encoded 'literaloooo secondx unicode-X')) -Options @{ ForbiddenText = $literals }
    Invoke-Scan (New-BytesFile (Get-Encoded $literals[0].ToUpperInvariant())) -Options @{ ForbiddenText = $literals }
    Invoke-Scan (New-BytesFile (Get-Encoded (('Z' * 65530) + "`0" + $homes[0]))) 'home-path'
    Invoke-Scan (New-BytesFile (Get-Encoded (('Z' * 32765) + "`0" + $literals[0]) 'odd')) 'forbidden-text' @{ ForbiddenText = $literals }

    $zip = New-ZipBytes @(@{ Name = 'folder/'; Bytes = [byte[]]@() }, @{ Name = 'folder/clean [1].exe'; Bytes = (Get-Encoded $clean) })
    $rmskin = Add-RmskinFooter $zip
    Invoke-Scan (New-BytesFile $zip '.zip')
    Invoke-Scan (New-BytesFile $zip '.bin')
    Invoke-Scan (New-BytesFile $rmskin '.rmskin')
    Invoke-Scan (New-BytesFile $rmskin '.bin')
    Invoke-Scan (New-BytesFile (New-ZipBytes @()) '.zip')
    Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = ('notes-' + [char]0x03a9 + '/'); Bytes = [byte[]]@(); Attributes = 0x41ed0010 })) '.zip')
    $nested = New-ZipBytes @(@{ Name = 'inner.rmskin'; Bytes = $rmskin })
    Invoke-Scan (New-BytesFile $nested '.zip')
    $comment = Get-Encoded $homes[0]
    $commented = [byte[]]($zip + $comment)
    $commented[$zip.Length - 2] = [byte]($comment.Length -band 255)
    $commented[$zip.Length - 1] = [byte]($comment.Length -shr 8)
    Invoke-Scan (New-BytesFile $commented '.zip') 'home-path'
    $dirtyZip = New-ZipBytes @(@{ Name = 'settings.json'; Bytes = (Get-Encoded $secrets[0].Text 'odd') })
    Invoke-Scan (New-BytesFile $dirtyZip '.zip') 'github-token'
    Invoke-Scan (New-BytesFile (Add-RmskinFooter $dirtyZip) '.rmskin') 'github-token'
    $nestedDirty = New-ZipBytes @(@{ Name = 'inner.bin'; Bytes = (Add-RmskinFooter $dirtyZip) })
    Invoke-Scan (New-BytesFile $nestedDirty '.zip') 'github-token'
    $sensitiveName = $secrets[0].Text + '.txt'
    Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = $sensitiveName; Bytes = [byte[]]@() })) '.zip') 'github-token'
    $sensitiveFile = New-BytesFile ([byte[]]@()) '.txt' $sensitiveName
    Invoke-Scan $sensitiveFile 'github-token'
    Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = $homes[2]; Bytes = [byte[]]@() })) '.zip') 'unsafe-entry-path'

    # Exclusions apply only during filesystem traversal, never supplied inputs or
    # ZIP members. Neither dist/target nor a .gitignore rule hides ordinary data.
    $admin = Join-Path $scratch 'administrative scope'
    Write-Bytes (Join-Path $admin '.git') (Get-Encoded $homes[0])
    Write-Bytes (Join-Path $admin '.worktrees/nested/private.txt') (Get-Encoded $secrets[0].Text)
    Write-Bytes (Join-Path $admin 'clean.exe') (Get-Encoded $clean)
    Invoke-Scan $admin
    Invoke-Scan (Join-Path $admin '.git') 'home-path'
    Invoke-Scan (Join-Path $admin '.worktrees') 'github-token'
    $gitDirectory = Join-Path $scratch 'administrative directory scope'
    Write-Bytes (Join-Path $gitDirectory '.GIT/objects/inert') (Get-Encoded $homes[0])
    Invoke-Scan $gitDirectory
    Invoke-Scan (Join-Path $gitDirectory '.GIT') 'home-path'
    foreach ($name in @('.git', '.worktrees/inert.txt', 'dist/taskbarx.exe', 'target/nested/data.txt')) {
        Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = $name; Bytes = (Get-Encoded $homes[1]) })) '.zip') 'home-path'
    }
    foreach ($name in @('dist/taskbarx.exe', 'target/settings.json', '.hidden', '.worktrees')) {
        $script:nextFile++
        $directory = Join-Path $scratch ('normal-payload-' + $script:nextFile)
        Write-Bytes (Join-Path $directory '.gitignore') (Get-Encoded '*')
        $file = Join-Path $directory $name
        Write-Bytes $file (Get-Encoded $homes[3])
        [IO.File]::SetAttributes($file, [IO.FileAttributes]::Hidden)
        Invoke-Scan $directory 'home-path'
    }

    foreach ($name in @('../escape', 'a/../../escape', '/rooted', 'C:/drive', '\\host\share', 'a\b', 'file:stream',
        'NUL.txt', 'CON', 'CON .txt', 'aux/x', 'COM1.txt', ('LPT' + [char]0x00b2 + '.txt'), 'trailing.', 'trailing ',
        'a//b', 'dir//', './a', "bad`nname", "bad`0name", 'a?b', 'CONIN$', 'CLOCK$')) {
        Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = $name; Bytes = [byte[]]@() })) '.zip') 'unsafe-entry-path'
    }
    Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = 'same'; Bytes = [byte[]]@() }, @{ Name = 'SAME'; Bytes = [byte[]]@() })) '.zip') 'ambiguous-entry-path'
    foreach ($attribute in @(-1577058304, 0x400, 0x10000000)) {
        Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = 'link'; Bytes = (Get-Encoded 'outside'); Attributes = $attribute })) '.zip') 'archive-link-or-special'
    }
    Invoke-Scan (New-BytesFile (New-ZipBytes @(@{ Name = 'dir/'; Bytes = (Get-Encoded 'not empty') })) '.zip') 'malformed-archive'
    foreach ($bytes in @([byte[]]@(), [byte[]]@(80,75,3,4), (Get-Encoded 'not an archive'))) {
        Invoke-Scan (New-BytesFile $bytes '.zip') 'malformed-archive'
    }
    foreach ($mode in @('length', 'high-length', 'flags', 'signature', 'nul', 'truncated', 'missing', 'trailing')) {
        $bad = [byte[]]$rmskin.Clone(); $footer = $bad.Length - 16
        switch ($mode) {
            'length' { $bad[$footer] = $bad[$footer] -bxor 1 }
            'high-length' { $bad[$footer + 7] = 128 }
            'flags' { $bad[$footer + 8] = 1 }
            'signature' { $bad[$footer + 9] = 0 }
            'nul' { $bad[$bad.Length - 1] = 1 }
            'truncated' { $bad = [byte[]]$bad[0..($bad.Length - 2)] }
            'missing' { $bad = $zip }
            'trailing' { $bad = [byte[]]($bad + @(0)) }
        }
        $category = 'malformed-rmskin'
        if ($mode -eq 'flags') { $category = 'unsupported-rmskin' }
        Invoke-Scan (New-BytesFile $bad '.rmskin') $category
    }
    $stored = New-ZipBytes @(@{ Name = 'plain.txt'; Bytes = (Get-Encoded 'AAAAAAAAAAAA') }) -Stored
    $central = [int][BitConverter]::ToUInt32($stored, $stored.Length - 6)
    foreach ($signed in @($true, $false)) {
        $descriptor = [byte[]]$stored[14..25]
        if ($signed) { $descriptor = [byte[]](@(80,75,7,8) + $descriptor) }
        $withDescriptor = [byte[]]($stored[0..($central - 1)] + $descriptor + $stored[$central..($stored.Length - 1)])
        $withDescriptor[6] = $withDescriptor[6] -bor 8
        $withDescriptor[$central + $descriptor.Length + 8] = $withDescriptor[$central + $descriptor.Length + 8] -bor 8
        [Array]::Clear($withDescriptor, 14, 12)
        [Array]::Copy([BitConverter]::GetBytes([uint32]($central + $descriptor.Length)), 0, $withDescriptor, $withDescriptor.Length - 6, 4)
        Invoke-Scan (New-BytesFile $withDescriptor '.zip')
    }
    foreach ($mode in @('crc', 'method', 'encrypted', 'multidisk', 'zip64', 'name', 'trailing', 'truncated')) {
        $bad = [byte[]]$stored.Clone(); $category = 'malformed-archive'
        switch ($mode) {
            'crc' { $bad[14] = $bad[14] -bxor 1; $bad[$central + 16] = $bad[$central + 16] -bxor 1 }
            'method' { $bad[8] = 99; $bad[$central + 10] = 99; $category = 'unsupported-archive' }
            'encrypted' { $bad[6] = 1; $bad[$central + 8] = 1; $category = 'unsupported-archive' }
            'multidisk' { $bad[$bad.Length - 18] = 1; $category = 'unsupported-archive' }
            'zip64' { $bad[$bad.Length - 12] = 255; $bad[$bad.Length - 11] = 255; $category = 'unsupported-archive' }
            'name' { $bad[30] = 98 }
            'trailing' { $bad = [byte[]]($bad + @(0)) }
            'truncated' { $bad = [byte[]]$bad[0..($bad.Length - 4)] }
        }
        Invoke-Scan (New-BytesFile $bad '.zip') $category
    }
    Invoke-Scan (New-BytesFile (Get-Encoded 'inert') '.7z') 'unsupported-archive'
    Invoke-Scan (New-BytesFile ([byte[]]@(31,139,8,0))) 'unsupported-archive'

    $smallFile = New-BytesFile (New-Object byte[] 200)
    Invoke-Scan $smallFile 'byte-limit' -Scanner (New-LimitedScanner @{ InputBytes = 128 })
    Invoke-Scan $smallFile 'byte-limit' -Scanner (New-LimitedScanner @{ FileBytes = 128 })
    $bomb = New-ZipBytes @(@{ Name = 'expanded'; Bytes = (New-Object byte[] 1024) })
    $brokenDeflate = [byte[]]$bomb.Clone()
    $dataOffset = 30 + [BitConverter]::ToUInt16($brokenDeflate, 26) + [BitConverter]::ToUInt16($brokenDeflate, 28)
    $brokenDeflate[$dataOffset] = 7 # Reserved deflate block type, not executable code.
    Invoke-Scan (New-BytesFile $brokenDeflate '.zip') 'inspection-failed'
    Invoke-Scan (New-BytesFile $bomb '.zip') 'byte-limit' -Scanner (New-LimitedScanner @{ EntryBytes = 128 })
    # Lie about BOTH local and central expanded lengths: limits must still be
    # enforced on actual decompressor output, not merely declared Length fields.
    $forged = [byte[]]$bomb.Clone()
    $central = [int][BitConverter]::ToUInt32($forged, $forged.Length - 6)
    [Array]::Copy([byte[]]@(1,0,0,0), 0, $forged, 22, 4)
    [Array]::Copy([byte[]]@(1,0,0,0), 0, $forged, $central + 24, 4)
    Invoke-Scan (New-BytesFile $forged '.zip') 'byte-limit' -Scanner (New-LimitedScanner @{ EntryBytes = 128 })
    $two = New-ZipBytes @(@{ Name = 'one'; Bytes = (New-Object byte[] 80) }, @{ Name = 'two'; Bytes = (New-Object byte[] 80) })
    Invoke-Scan (New-BytesFile $two '.zip') 'byte-limit' -Scanner (New-LimitedScanner @{ ExpandedBytes = 128 })
    Invoke-Scan (New-BytesFile $two '.zip') 'entry-count-limit' -Scanner (New-LimitedScanner @{ Entries = 1 })
    Invoke-Scan (New-BytesFile $nested '.zip') 'archive-depth-limit' -Scanner (New-LimitedScanner @{ ArchiveDepth = 1 })
    $deep = $zip
    for ($i = 0; $i -lt 4; $i++) { $deep = New-ZipBytes @(@{ Name = 'nested.zip'; Bytes = $deep }) }
    Invoke-Scan (New-BytesFile $deep '.zip') 'archive-depth-limit'
    $directory = Join-Path $scratch 'filesystem limits'
    Write-Bytes (Join-Path $directory 'one') (New-Object byte[] 80)
    Write-Bytes (Join-Path $directory 'two') (New-Object byte[] 80)
    Invoke-Scan $directory 'byte-limit' -Scanner (New-LimitedScanner @{ FileBytes = 128 })
    Invoke-Scan $directory 'entry-count-limit' -Scanner (New-LimitedScanner @{ Entries = 1 })
    $deepDirectory = Join-Path $scratch 'directory depth'
    Write-Bytes (Join-Path $deepDirectory 'a/b/c/data') ([byte[]]@())
    Invoke-Scan $deepDirectory 'directory-depth-limit' -Scanner (New-LimitedScanner @{ DirectoryDepth = 1 })

    foreach ($path in @('\\fixture.invalid\share', '//fixture.invalid/share', '\\?\C:\fixture', '\\.\NUL',
        'C:relative', 'Env:PATH', 'C:\fixture:stream', '..\fixture', 'NUL', 'bad.', "bad`npath")) {
        Invoke-Scan $path 'invalid-input-path'
    }
    $missing = Join-Path $scratch ($secrets[0].Text + '-missing')
    Invoke-Scan $missing 'inspection-failed'
    Push-Location -LiteralPath $scratch
    try { Invoke-Scan $cleanFile.Substring($scratch.Length + 1) }
    finally { Pop-Location }
    # An exclusive lock produces a real read failure, without touching permissions
    # or exposing raw IOException messages. Snapshot the parent before taking it.
    $locked = New-BytesFile $zip '.zip'
    $beforeLock = @(Get-Snapshot) -join "`n"
    $lock = [IO.File]::Open($locked, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    try { Test-ChildProcess $locked 'inspection-failed' -SkipSnapshot }
    finally { $lock.Dispose() }
    Assert-Test ((@(Get-Snapshot) -join "`n") -ceq $beforeLock) 'locked input and scope unchanged'

    $target = Join-Path $scratch 'junction target'
    Write-Bytes (Join-Path $target 'clean') (Get-Encoded $clean)
    $link = Join-Path $scratch 'junction scope/link'
    [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($link))
    $created = $false
    try {
        $targetArgument = [Management.Automation.WildcardPattern]::Escape($target)
        $null = New-Item -ItemType Junction -Path $link -Target $targetArgument
        $created = $true
    } catch { $script:skipped++ }
    if ($created) {
        Invoke-Scan $link 'filesystem-link'
        Invoke-Scan (Join-Path $link 'clean') 'filesystem-link'
        Invoke-Scan ([IO.Path]::GetDirectoryName($link)) 'filesystem-link'
    }
    $symlink = Join-Path $scratch 'file-link.bin'; $created = $false
    try {
        $null = New-Item -ItemType SymbolicLink -Path $symlink -Target ([Management.Automation.WildcardPattern]::Escape($cleanFile))
        $created = $true
    } catch { $script:skipped++ }
    if ($created) { Invoke-Scan $symlink 'filesystem-link' }

    try { throw 'Inert caller error sentinel.' } catch { }
    Invoke-Scan $cleanFile # Nonempty preexisting error history must be preserved.

    Test-ChildProcess $cleanFile
    Test-ChildProcess (New-BytesFile (Get-Encoded $homes[0])) 'home-path'
    Test-ChildProcess $missing 'inspection-failed'
    Test-ChildProcess $sensitiveFile 'github-token'
    Test-ChildProcess '' 'invalid-input-path' -OmitPath
    Write-Output ("PASS: {0} assertions; {1} in-process scans; 6 child-process checks; {2} unavailable link fixtures." -f $script:passed, $script:cases, $script:skipped)
    Write-Output 'Inert fixtures retained in a fresh temporary directory. No application execution, extraction, network access, or cleanup.'
} catch {
    # Fixed labels only. Never format $_, fixture paths, or expected/actual values.
    [Console]::Error.WriteLine(('FAIL: fixture check [{0}], case {1}, after {2} assertions. Fixtures retained.' -f $script:stage, $script:cases, $script:passed))
    exit 1
}
