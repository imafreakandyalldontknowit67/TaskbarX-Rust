# Shared helpers only: importing this module never builds or probes the desktop.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-RepositoryRoot {
    param([Parameter(Mandatory)][string]$ScriptDirectory)
    return (Resolve-Path -LiteralPath (Join-Path $ScriptDirectory '..')).ProviderPath
}

function Assert-IntegratedWorkspace {
    param([Parameter(Mandatory)][string]$Root)
    $required = @('Cargo.toml', 'crates/taskbarx-core/Cargo.toml',
        'crates/taskbarx-win32/Cargo.toml', 'crates/taskbarx-config/Cargo.toml',
        'crates/taskbarx-configurator/Cargo.toml')
    $missing = @($required | Where-Object { -not (Test-Path -LiteralPath (Join-Path $Root $_) -PathType Leaf) })
    if ($missing.Count -gt 0) {
        throw "Incomplete workspace: $($missing -join ', '). Run this script AFTER integration of all six branches. Isolated-worktree checks are not application validation."
    }
}

function New-OutputDirectory {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][ValidateSet('verification', 'release-evidence', 'TaskbarX-Rust', 'script-tests')][string]$Category
    )
    $parent = $Root
    foreach ($part in @('dist', $Category)) {
        $parent = Join-Path $parent $part
        if (Test-Path -LiteralPath $parent) {
            $item = Get-Item -LiteralPath $parent
            if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                throw "Output directory must be a normal directory, not a file or reparse point: $parent"
            }
        } else {
            [void][IO.Directory]::CreateDirectory($parent)
        }
    }
    $id = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N')
    $path = Join-Path $parent $id
    [void][IO.Directory]::CreateDirectory($path)
    return $path
}

function Write-Utf8File {
    param([Parameter(Mandatory)][string]$Path, [AllowEmptyString()][string]$Text = '')
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}

function Write-JsonFile {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Value)
    Write-Utf8File -Path $Path -Text (ConvertTo-Json -InputObject $Value -Depth 100)
}

function ConvertFrom-JsonEvidence {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text, [string]$Label = 'JSON evidence')
    if ([string]::IsNullOrWhiteSpace($Text)) { throw "$Label is empty." }
    try { $value = ConvertFrom-Json -InputObject $Text -ErrorAction Stop }
    catch { throw "$Label is invalid JSON: $($_.Exception.Message)" }
    if ($null -eq $value) { throw "$Label is JSON null, not diagnostic/metadata evidence." }
    return $value
}

function Assert-CommandSuccess {
    param([Parameter(Mandatory)]$Result)
    if ($null -eq $Result.exit_code -or $Result.exit_code -ne 0) {
        throw "Command failed (exit $($Result.exit_code)): $($Result.executable). See $($Result.stderr_file)"
    }
}

function Invoke-LoggedCommand {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$ArgumentList,
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [Parameter(Mandatory)][string]$EvidenceDirectory,
        [Parameter(Mandatory)][ValidatePattern('^[a-zA-Z0-9-]+$')][string]$Name
    )
    # Resolve a native application before temporarily accommodating PowerShell 5.1's
    # conversion of native stderr into non-terminating NativeCommandError records.
    $command = Get-Command -Name $FilePath -CommandType Application -ErrorAction Stop | Select-Object -First 1
    $stdoutPath = Join-Path $EvidenceDirectory "$Name.stdout.txt"
    $stderrPath = Join-Path $EvidenceDirectory "$Name.stderr.txt"
    $recordPath = Join-Path $EvidenceDirectory "$Name.command.json"
    foreach ($path in @($stdoutPath, $stderrPath, $recordPath)) {
        if (Test-Path -LiteralPath $path) { throw "Refusing to overwrite command evidence: $path" }
    }
    $started = [DateTime]::UtcNow.ToString('o')
    Write-Host ("[{0}] {1}" -f $Name, ($ArgumentList -join ' '))
    $exitCode = $null
    Push-Location -LiteralPath $WorkingDirectory
    try {
        $savedPreference = $ErrorActionPreference
        # Exit codes below are authoritative on both Windows PowerShell and PS 7.
        $PSNativeCommandUseErrorActionPreference = $false
        try {
            $ErrorActionPreference = 'Continue'
            # Native commands update the global automatic variable. A local
            # LASTEXITCODE would mask it in a module function on PowerShell 5.1.
            $global:LASTEXITCODE = $null
            & $command.Source @ArgumentList 1> $stdoutPath 2> $stderrPath
            $exitCode = $global:LASTEXITCODE
        } finally {
            $ErrorActionPreference = $savedPreference
        }
    } finally {
        Pop-Location
    }
    # Native redirection uses different encodings in PS 5 and PS 7; normalize logs.
    $stdout = [string](Get-Content -LiteralPath $stdoutPath -Raw)
    $stderr = [string](Get-Content -LiteralPath $stderrPath -Raw)
    Write-Utf8File -Path $stdoutPath -Text $stdout
    Write-Utf8File -Path $stderrPath -Text $stderr
    $result = [pscustomobject][ordered]@{
        executable = $command.Source
        arguments = @($ArgumentList)
        working_directory = $WorkingDirectory
        started_utc = $started
        finished_utc = [DateTime]::UtcNow.ToString('o')
        exit_code = $exitCode
        stdout_file = $stdoutPath
        stderr_file = $stderrPath
    }
    Write-JsonFile -Path $recordPath -Value $result
    Assert-CommandSuccess -Result $result
    return $stdout
}

function Assert-WorkspaceMetadata {
    param([Parameter(Mandatory)]$Metadata, [Parameter(Mandatory)][string]$Root)
    $expected = [ordered]@{
        'taskbarx-rust' = 'Cargo.toml'
        'taskbarx-core' = 'crates/taskbarx-core/Cargo.toml'
        'taskbarx-win32' = 'crates/taskbarx-win32/Cargo.toml'
        'taskbarx-config' = 'crates/taskbarx-config/Cargo.toml'
        'taskbarx-configurator' = 'crates/taskbarx-configurator/Cargo.toml'
    }
    if ([IO.Path]::GetFullPath($Metadata.workspace_root) -ne [IO.Path]::GetFullPath($Root)) {
        throw 'Cargo resolved a different workspace root.'
    }
    foreach ($name in $expected.Keys) {
        $packages = @($Metadata.packages | Where-Object { $_.name -eq $name -and $_.id -in $Metadata.workspace_members })
        if ($packages.Count -ne 1) { throw "Required integrated workspace package missing/ambiguous: $name" }
        $path = [IO.Path]::GetFullPath((Join-Path $Root $expected[$name]))
        if ([IO.Path]::GetFullPath($packages[0].manifest_path) -ne $path) { throw "Wrong package location: $name" }
    }
    foreach ($pair in @(@('taskbarx-rust', 'taskbarx'), @('taskbarx-configurator', 'taskbarx-configurator'))) {
        $package = @($Metadata.packages | Where-Object { $_.name -eq $pair[0] -and $_.id -in $Metadata.workspace_members })[0]
        $targets = @($package.targets | Where-Object { $_.name -eq $pair[1] -and 'bin' -in $_.kind })
        if ($targets.Count -ne 1) { throw "Required binary target missing: $($pair[1])" }
    }
}

function Get-WorkspaceBinaryPaths {
    param(
        [Parameter(Mandatory)][string]$BuildOutput,
        [Parameter(Mandatory)]$Metadata,
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][ValidateSet('debug', 'release')][string]$Profile
    )
    $records = @($BuildOutput -split '\r?\n' | Where-Object { $_.Trim() } | ForEach-Object {
        ConvertFrom-JsonEvidence -Text $_ -Label 'Cargo build artifact record'
    })
    $paths = [ordered]@{}
    foreach ($pair in @(@('taskbarx-rust', 'taskbarx'), @('taskbarx-configurator', 'taskbarx-configurator'))) {
        $package = @($Metadata.packages | Where-Object { $_.name -eq $pair[0] -and $_.id -in $Metadata.workspace_members })[0]
        $artifacts = @($records | Where-Object {
            $_.reason -eq 'compiler-artifact' -and $_.package_id -eq $package.id -and
            $_.target.name -eq $pair[1] -and 'bin' -in $_.target.kind -and $null -ne $_.executable
        })
        if ($artifacts.Count -ne 1) { throw "Cargo did not report exactly one built executable for $($pair[1])." }
        $expected = [IO.Path]::GetFullPath((Join-Path $Root "target/$Profile/$($pair[1]).exe"))
        $actual = [IO.Path]::GetFullPath($artifacts[0].executable)
        if ($actual -ne $expected) {
            throw "Unexpected binary path: $actual. Expected $expected. Remove cross-target/build.target overrides for this native Windows x64 workflow."
        }
        $paths[$pair[1]] = $actual
    }
    return $paths
}

function Assert-X64Executable {
    param([Parameter(Mandatory)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Built executable is missing: $Path" }
    $stream = [IO.File]::OpenRead($Path)
    $reader = New-Object IO.BinaryReader($stream)
    try {
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE executable: $Path" }
        $stream.Position = 0x3c
        $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or ($offset + 6) -gt $stream.Length) { throw "Invalid PE header: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
            throw "Not a Windows x64 PE executable: $Path"
        }
    } finally {
        $reader.Dispose()
        $stream.Dispose()
    }
}

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Write-PayloadChecksums {
    param([Parameter(Mandatory)][string]$Directory)
    $Directory = (Resolve-Path -LiteralPath $Directory).ProviderPath.TrimEnd('\', '/')
    $manifest = Join-Path $Directory 'SHA256SUMS.txt'
    if (Test-Path -LiteralPath $manifest) { throw "Refusing to overwrite checksum manifest: $manifest" }
    $files = @(Get-ChildItem -LiteralPath $Directory -Recurse -File | Sort-Object FullName)
    if ($files.Count -eq 0) { throw 'Cannot checksum an empty payload.' }
    $lines = @($files | ForEach-Object {
        $relative = $_.FullName.Substring($Directory.Length + 1).Replace('\', '/')
        '{0}  {1}' -f (Get-Sha256 -Path $_.FullName), $relative
    })
    Write-Utf8File -Path $manifest -Text (($lines -join "`n") + "`n")
    return $manifest
}

function New-LocalArchive {
    param([Parameter(Mandatory)][string]$Directory, [Parameter(Mandatory)][string]$ZipPath)
    foreach ($path in @($ZipPath, "$ZipPath.sha256")) {
        if (Test-Path -LiteralPath $path) { throw "Refusing to overwrite archive evidence: $path" }
    }
    # .NET accepts literal paths including spaces/brackets; never invokes a shell.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($Directory, $ZipPath)
    if ((Get-Item -LiteralPath $ZipPath).Length -le 0) { throw 'The ZIP is empty.' }
    $hash = Get-Sha256 -Path $ZipPath
    Write-Utf8File -Path "$ZipPath.sha256" -Text ("$hash  $(Split-Path -Leaf $ZipPath)`n")
    return $hash
}

Export-ModuleMember -Function Get-RepositoryRoot, Assert-IntegratedWorkspace, New-OutputDirectory,
    Write-Utf8File, Write-JsonFile, ConvertFrom-JsonEvidence, Assert-CommandSuccess,
    Invoke-LoggedCommand, Assert-WorkspaceMetadata, Get-WorkspaceBinaryPaths,
    Assert-X64Executable, Get-Sha256, Write-PayloadChecksums, New-LocalArchive
