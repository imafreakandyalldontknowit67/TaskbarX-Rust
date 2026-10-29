#Requires -Version 5.1
[CmdletBinding()]
param([switch]$Clippy)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildHelpers.psm1') -Force
$root = Get-RepositoryRoot -ScriptDirectory $PSScriptRoot
Assert-IntegratedWorkspace -Root $root

# Calls the real verification script, never a fake build or subset of the workspace.
$verification = & (Join-Path $PSScriptRoot 'verify.ps1') -Clippy:$Clippy
if ($null -eq $verification -or -not (Test-Path -LiteralPath $verification.SummaryPath -PathType Leaf)) {
    throw 'Verification did not return evidence. No release package can be produced.'
}
$verified = ConvertFrom-JsonEvidence -Text (Get-Content -LiteralPath $verification.SummaryPath -Raw) -Label 'Verification summary'
if ($verified.status -ne 'passed' -or -not $verified.diagnostics_completed) { throw 'Verification did not pass.' }
$evidence = New-OutputDirectory -Root $root -Category release-evidence
$state = [ordered]@{
    status = 'running'
    started_utc = [DateTime]::UtcNow.ToString('o')
    verification_evidence = $verification.EvidenceDirectory
    only_read_only_desktop_diagnostics = $true
    release_executables_launched = $false
    live_positioning_tested = $false
    live_styling_tested = $false
    third_party_license_review = 'REQUIRED before distribution; inventory is not legal clearance'
}
$statePath = Join-Path $evidence 'release.json'
Write-JsonFile -Path $statePath -Value $state
$oldTargetDir = [Environment]::GetEnvironmentVariable('CARGO_TARGET_DIR', 'Process')
try {
    $env:CARGO_TARGET_DIR = Join-Path $root 'target'
    $common = @{ WorkingDirectory = $root; EvidenceDirectory = $evidence }
    $lock = Join-Path $root 'Cargo.lock'
    if ((Get-Sha256 -Path $lock) -ne $verification.LockfileSha256) { throw 'Cargo.lock changed after verification.' }
    $metadataText = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('metadata', '--locked', '--format-version', '1') -Name cargo-metadata
    $metadata = ConvertFrom-JsonEvidence -Text $metadataText -Label 'Release Cargo metadata'
    Assert-WorkspaceMetadata -Metadata $metadata -Root $root
    $buildText = Invoke-LoggedCommand @common -FilePath cargo -ArgumentList @('build', '--release', '--workspace', '--locked', '--message-format=json') -Name cargo-release
    $binaries = Get-WorkspaceBinaryPaths -BuildOutput $buildText -Metadata $metadata -Root $root -Profile release
    foreach ($path in $binaries.Values) { Assert-X64Executable -Path $path }
    if ((Get-Sha256 -Path $lock) -ne $verification.LockfileSha256) { throw 'Cargo.lock changed during release.' }

    $package = New-OutputDirectory -Root $root -Category TaskbarX-Rust
    foreach ($name in $binaries.Keys) {
        Copy-Item -LiteralPath $binaries[$name] -Destination (Join-Path $package "$name.exe")
    }
    foreach ($name in @('LICENSE', 'THIRD_PARTY_NOTICES.md', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $root $name) -Destination (Join-Path $package $name)
    }
    Copy-Item -LiteralPath (Join-Path $root 'docs') -Destination (Join-Path $package 'docs') -Recurse
    $sample = Join-Path $root 'docs/settings.sample.json'
    $null = ConvertFrom-JsonEvidence -Text (Get-Content -LiteralPath $sample -Raw) -Label 'Sample settings'
    Copy-Item -LiteralPath $sample -Destination (Join-Path $package 'settings.sample.json')
    Copy-Item -LiteralPath $lock -Destination (Join-Path $evidence 'Cargo.lock')
    $inventory = @($metadata.packages | Sort-Object name, version | ForEach-Object {
        [pscustomobject][ordered]@{
            name = $_.name; version = $_.version; source = $_.source
            license = $_.license; license_file = $_.license_file
            workspace_member = ($_.id -in $metadata.workspace_members)
        }
    })
    Write-JsonFile -Path (Join-Path $evidence 'dependency-inventory.json') -Value $inventory
    $state.compiler_version = $verified.compiler_version
    $state.cargo_version = $verified.cargo_version
    $state.os = $verified.os
    $state.git_commit = $verified.git_commit
    $state.git_status = $verified.git_status
    $state.lockfile_sha256 = $verification.LockfileSha256
    $state.package_directory = $package
    $state.status = 'build-passed-packaging-pending'
    Write-JsonFile -Path $statePath -Value $state

    $packageEvidence = Join-Path $package 'evidence'
    [void][IO.Directory]::CreateDirectory($packageEvidence)
    Copy-Item -LiteralPath $verification.EvidenceDirectory -Destination (Join-Path $packageEvidence 'verification') -Recurse
    Copy-Item -LiteralPath $evidence -Destination (Join-Path $packageEvidence 'build') -Recurse
    # ZIP success is recorded outside the ZIP to avoid self-referential hashes.
    $metadataForPackage = [ordered]@{
        status = 'built-and-verified-local-candidate'
        compiler_version = $verified.compiler_version
        cargo_version = $verified.cargo_version
        git_commit = $verified.git_commit
        lockfile_sha256 = $verification.LockfileSha256
        only_read_only_desktop_diagnostics = $true
        release_executables_launched = $false
        manual_matrix = 'NOT EXECUTED by scripts'
        third_party_license_review = 'REQUIRED before distribution'
    }
    Write-JsonFile -Path (Join-Path $package 'build-metadata.json') -Value $metadataForPackage
    $checksums = Write-PayloadChecksums -Directory $package
    $id = Split-Path -Leaf $package
    $zip = Join-Path $root "dist/TaskbarX-Rust-$id.zip"
    $zipHash = New-LocalArchive -Directory $package -ZipPath $zip
    $state.status = 'passed'
    $state.zip = $zip
    $state.zip_sha256 = $zipHash
    $state.checksums = $checksums
} catch {
    $state.status = 'failed'
    $state.error = $_.Exception.Message
    throw
} finally {
    $state.finished_utc = [DateTime]::UtcNow.ToString('o')
    try { Write-JsonFile -Path $statePath -Value $state }
    finally { [Environment]::SetEnvironmentVariable('CARGO_TARGET_DIR', $oldTargetDir, 'Process') }
}
Write-Host "Local candidate only (no live positioning/style tests): $package"
Write-Host "ZIP: $zip`nSHA-256: $zipHash`nRelease evidence: $evidence"
[pscustomobject]@{ PackageDirectory = $package; ZipPath = $zip; ChecksumsPath = $state.checksums; EvidenceDirectory = $evidence }
