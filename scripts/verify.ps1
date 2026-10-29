#Requires -Version 5.1
[CmdletBinding()]
param([switch]$Clippy)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildHelpers.psm1') -Force
$root = Get-RepositoryRoot -ScriptDirectory $PSScriptRoot
# Fail before creating evidence or generating a lockfile in an isolated branch.
Assert-IntegratedWorkspace -Root $root
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'Verification requires native Windows x64; this is not a cross-compilation validation script.'
}
$evidence = New-OutputDirectory -Root $root -Category verification
$summaryPath = Join-Path $evidence 'verification.json'
$summary = [ordered]@{
    status = 'running'
    repository_root = $root
    started_utc = [DateTime]::UtcNow.ToString('o')
    os = [Environment]::OSVersion.VersionString
    powershell_version = $PSVersionTable.PSVersion.ToString()
    clippy_requested = [bool]$Clippy
    lockfile_generated = $false
    desktop_probe = 'target/debug/taskbarx.exe diagnose --json'
    only_read_only_desktop_diagnostics = $true
    diagnostics_completed = $false
    live_positioning_tested = $false
    live_styling_tested = $false
    manual_matrix = 'NOT EXECUTED by this script'
}
Write-JsonFile -Path $summaryPath -Value $summary
$oldTargetDir = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
try {
    $env:CARGO_TARGET_DIR = Join-Path $root 'target'
    $common = @{ WorkingDirectory = $root; EvidenceDirectory = $evidence }
    $rustc = Invoke-LoggedCommand @common -FilePath rustc -ArgumentList @('-vV') -Name rustc-version
    if ($rustc -notmatch '(?m)^host: x86_64-pc-windows-msvc\s*$') { throw 'Use the native x86_64-pc-windows-msvc Rust toolchain.' }
    $summary.compiler_version = $rustc.Trim()
    $summary.cargo_version = (Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('--version') -Name cargo-version).Trim()
    $summary.git_commit = (Invoke-LoggedCommand @common -FilePath git -ArgumentList @('rev-parse', 'HEAD') -Name git-commit).Trim()
    $summary.git_status = Invoke-LoggedCommand @common -FilePath git -ArgumentList @('status', '--porcelain') -Name git-status
    $lock = Join-Path $root 'Cargo.lock'
    if (-not (Test-Path -LiteralPath $lock -PathType Leaf)) {
        $null = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('generate-lockfile') -Name generate-lockfile
        $summary.lockfile_generated = $true
    }
    if (-not (Test-Path -LiteralPath $lock -PathType Leaf)) { throw 'Cargo did not generate the root Cargo.lock.' }
    $summary.lockfile_sha256 = Get-Sha256 -Path $lock
    $metadataText = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('metadata', '--locked', '--format-version', '1') -Name cargo-metadata
    $metadata = ConvertFrom-JsonEvidence -Text $metadataText -Label 'Cargo metadata'
    Assert-WorkspaceMetadata -Metadata $metadata -Root $root
    $null = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('fmt', '--all', '--', '--check') -Name cargo-fmt
    $null = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('test', '--workspace', '--locked') -Name cargo-test
    $buildText = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('build', '--workspace', '--locked', '--message-format=json') -Name cargo-build
    $binaries = Get-WorkspaceBinaryPaths -BuildOutput $buildText -Metadata $metadata -Root $root -Profile debug
    foreach ($path in $binaries.Values) { Assert-X64Executable -Path $path }
    if ($Clippy) {
        $null = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings') -Name cargo-clippy
    }
    # This is deliberately the only application process/desktop probe in verification.
    $diagnosticText = Invoke-LoggedCommand @common -FilePath $binaries['taskbarx'] -ArgumentList @('diagnose', '--json') -Name diagnose
    $null = ConvertFrom-JsonEvidence -Text $diagnosticText -Label 'Read-only diagnose output'
    Write-Utf8File -Path (Join-Path $evidence 'diagnose.json') -Text $diagnosticText
    $summary.diagnostics_completed = $true
    if ((Get-Sha256 -Path $lock) -ne $summary.lockfile_sha256) { throw 'Cargo.lock changed during locked validation.' }
    $summary.status = 'passed'
} catch {
    $summary.status = 'failed'
    $summary.error = $_.Exception.Message
    throw
} finally {
    $summary.finished_utc = [DateTime]::UtcNow.ToString('o')
    try { Write-JsonFile -Path $summaryPath -Value $summary }
    finally { [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $oldTargetDir, 'Process') }
}
Write-Host "Verification passed (read-only desktop diagnostics only). Evidence: $evidence"
[pscustomobject]@{ EvidenceDirectory = $evidence; SummaryPath = $summaryPath; LockfileSha256 = $summary.lockfile_sha256 }
