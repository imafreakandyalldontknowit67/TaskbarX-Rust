#Requires -Version 5.1
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildHelpers.psm1') -Force
$root = Get-RepositoryRoot -ScriptDirectory $PSScriptRoot
$scratch = New-OutputDirectory -Root $root -Category script-tests
$script:passed = 0

function Assert-Test {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw "Helper test failed: $Message" }
    $script:passed++
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$Pattern)
    $caught = $null
    try { & $Action | Out-Null } catch { $caught = $_.Exception.Message }
    Assert-Test ($null -ne $caught -and $caught -match $Pattern) "Expected error matching '$Pattern'; got '$caught'"
}

try {
    foreach ($file in @(Get-ChildItem -LiteralPath $PSScriptRoot -File | Where-Object { $_.Extension -in @('.ps1', '.psm1') })) {
        $tokens = $null
        $errors = $null
        $null = [Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$errors)
        Assert-Test ($errors.Count -eq 0) "Syntax errors in $($file.Name): $errors"
    }
    Push-Location -LiteralPath $scratch
    try { Assert-Test ((Get-RepositoryRoot -ScriptDirectory $PSScriptRoot) -eq $root) 'Root must not depend on current directory' }
    finally { Pop-Location }
    Assert-Throws { Assert-IntegratedWorkspace -Root $scratch } 'AFTER integration'
    Assert-Test ((ConvertFrom-JsonEvidence -Text '{"read_only":true}').read_only) 'JSON parsing'
    Assert-Throws { ConvertFrom-JsonEvidence -Text '' } 'empty'
    Assert-Throws { ConvertFrom-JsonEvidence -Text 'not JSON' } 'invalid JSON'
    Assert-Throws { ConvertFrom-JsonEvidence -Text 'null' } 'JSON null'
    Assert-CommandSuccess -Result ([pscustomobject]@{ exit_code = 0; executable = 'FAKE'; stderr_file = 'FAKE' })
    Assert-Throws { Assert-CommandSuccess -Result ([pscustomobject]@{ exit_code = 19; executable = 'FAKE'; stderr_file = 'FAKE' }) } 'exit 19'

    # Fake in-memory Cargo results only; no Cargo manifests, binaries or lockfiles
    # are fabricated. These cannot be mistaken for integrated build evidence.
    $names = @('taskbarx-rust', 'taskbarx-core', 'taskbarx-win32', 'taskbarx-config', 'taskbarx-configurator')
    $fake = [pscustomobject]@{
        workspace_root = $root
        workspace_members = @($names | ForEach-Object { "fake-id:$_" })
        packages = @($names | ForEach-Object {
            $manifest = if ($_ -eq 'taskbarx-rust') { 'Cargo.toml' } else { "crates/$_/Cargo.toml" }
            $binary = if ($_ -eq 'taskbarx-rust') { 'taskbarx' } else { $_ }
            $kind = if ($_ -in @('taskbarx-rust', 'taskbarx-configurator')) { 'bin' } else { 'lib' }
            [pscustomobject]@{ name = $_; id = "fake-id:$_"; manifest_path = (Join-Path $root $manifest)
                targets = @([pscustomobject]@{ name = $binary; kind = @($kind) }) }
        })
    }
    Assert-WorkspaceMetadata -Metadata $fake -Root $root
    $script:passed++
    $missing = ConvertFrom-JsonEvidence -Text (ConvertTo-Json -InputObject $fake -Depth 20)
    $missing.workspace_members = @($missing.workspace_members | Where-Object { $_ -ne 'fake-id:taskbarx-config' })
    Assert-Throws { Assert-WorkspaceMetadata -Metadata $missing -Root $root } 'package missing'
    $wrong = ConvertFrom-JsonEvidence -Text (ConvertTo-Json -InputObject $fake -Depth 20)
    $wrong.packages[0].targets[0].name = 'wrong-engine-name'
    Assert-Throws { Assert-WorkspaceMetadata -Metadata $wrong -Root $root } 'binary target missing'
    $artifactLines = @(
        [pscustomobject]@{ reason = 'compiler-artifact'; package_id = 'fake-id:taskbarx-rust'; target = @{ name = 'taskbarx'; kind = @('bin') }; executable = (Join-Path $root 'target/release/taskbarx.exe') },
        [pscustomobject]@{ reason = 'compiler-artifact'; package_id = 'fake-id:taskbarx-configurator'; target = @{ name = 'taskbarx-configurator'; kind = @('bin') }; executable = (Join-Path $root 'target/release/taskbarx-configurator.exe') },
        [pscustomobject]@{ reason = 'build-finished'; success = $true }
    )
    $fakeOutput = ($artifactLines | ForEach-Object { ConvertTo-Json -InputObject $_ -Depth 10 -Compress }) -join "`n"
    $paths = Get-WorkspaceBinaryPaths -BuildOutput $fakeOutput -Metadata $fake -Root $root -Profile release
    Assert-Test ($paths.Count -eq 2) 'Both compiler-reported binaries must be selected'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput $fakeOutput -Metadata $fake -Root $root -Profile debug } 'Unexpected binary path'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput '{"reason":"build-finished","success":true}' -Metadata $fake -Root $root -Profile release } 'did not report'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput ($fakeOutput + "`n" + $fakeOutput) -Metadata $fake -Root $root -Profile release } 'exactly one'

    # Exercise the real argument-array runner using a harmless temporary PowerShell
    # child script, not a fake .exe. It never touches the desktop or invokes Cargo.
    $child = Join-Path $scratch 'fake command with spaces.ps1'
    Write-Utf8File -Path $child -Text @'
param([string]$Echo, [int]$Code = 0, [switch]$Stderr)
if ($Stderr) { [Console]::Error.WriteLine('expected helper-test stderr') }
ConvertTo-Json -Compress -InputObject @{ echo = $Echo; working_directory = (Get-Location).Path }
exit $Code
'@
    $shell = (Get-Process -Id $PID).Path
    $special = 'argument with spaces,comma;semicolon&literal'
    $common = @{ FilePath = $shell; WorkingDirectory = $scratch; EvidenceDirectory = $scratch }
    $output = Invoke-LoggedCommand @common -Name runner-success -ArgumentList @('-NoProfile', '-NonInteractive', '-File', $child, '-Echo', $special, '-Stderr')
    $parsed = ConvertFrom-JsonEvidence -Text $output
    Assert-Test ($parsed.echo -eq $special) 'Argument arrays preserve spaces and literal shell metacharacters'
    Assert-Test ($parsed.working_directory -eq $scratch) 'Child working directory'
    Assert-Test ((Get-Content -LiteralPath (Join-Path $scratch 'runner-success.stderr.txt') -Raw) -match 'expected helper-test stderr') 'Stderr evidence retained on success'
    Assert-Throws {
        Invoke-LoggedCommand @common -Name runner-failure -ArgumentList @('-NoProfile', '-NonInteractive', '-File', $child, '-Code', '23')
    } 'exit 23'
    Assert-Throws {
        Invoke-LoggedCommand @common -Name runner-success -ArgumentList @('-NoProfile', '-NonInteractive', '-File', $child)
    } 'overwrite command evidence'
    Assert-Throws { Assert-X64Executable -Path $child } 'Not a PE executable'
    Assert-Throws { Assert-X64Executable -Path (Join-Path $scratch 'never-created.exe') } 'missing'
    if ([Environment]::Is64BitProcess) { Assert-X64Executable -Path $shell; $script:passed++ }
    $sample = ConvertFrom-JsonEvidence -Text (Get-Content -LiteralPath (Join-Path $root 'docs/settings.sample.json') -Raw)
    Assert-Test ($sample.schema_version -eq 1 -and $sample.color.Count -eq 4 -and $sample.color[3] -eq 128 -and $sample.animation -eq 'cubiceaseinout') 'Documented sample defaults'
    Assert-Test ($sample.center -and $sample.primary -and $sample.secondary -and -not $sample.center_between -and $sample.primary_offset -eq 0 -and $sample.secondary_offset -eq 0 -and $sample.animation_ms -eq 300 -and $sample.poll_ms -eq 400 -and $sample.style -eq 'default' -and ($sample.color -join ',') -eq '0,0,0,128') 'Complete sample defaults'

    # Packaging-helper fixture is text-only, not a dummy application release.
    $payload = Join-Path $scratch 'text-only payload [fixture]'
    [void][IO.Directory]::CreateDirectory((Join-Path $payload 'nested'))
    Write-Utf8File -Path (Join-Path $payload 'fixture.txt') -Text 'SCRIPT HELPER TEST ONLY'
    Write-Utf8File -Path (Join-Path $payload 'nested/second.txt') -Text 'not an application binary'
    $checksums = Write-PayloadChecksums -Directory $payload
    $lines = @(Get-Content -LiteralPath $checksums)
    Assert-Test ($lines.Count -eq 2) 'Checksum manifest excludes itself'
    foreach ($line in $lines) {
        $parts = $line -split '  ', 2
        Assert-Test ((Get-Sha256 -Path (Join-Path $payload $parts[1])) -eq $parts[0]) 'Payload checksums match files'
    }
    Assert-Throws { Write-PayloadChecksums -Directory $payload } 'overwrite checksum manifest'
    $zip = Join-Path $scratch 'text-only archive [fixture].zip'
    $hash = New-LocalArchive -Directory $payload -ZipPath $zip
    Assert-Test ((Get-Sha256 -Path $zip) -eq $hash) 'Archive checksum'
    Assert-Test ((Get-Content -LiteralPath "$zip.sha256" -Raw).StartsWith("$hash  ")) 'Adjacent ZIP checksum evidence'
    $archive = [IO.Compression.ZipFile]::OpenRead($zip)
    try { Assert-Test ($archive.Entries.Count -eq 3) 'ZIP has both text files and payload checksums' }
    finally { $archive.Dispose() }
    Assert-Throws { New-LocalArchive -Directory $payload -ZipPath $zip } 'overwrite archive evidence'
    Write-JsonFile -Path (Join-Path $scratch 'helper-tests.json') -Value ([ordered]@{
        status = 'passed'; assertions = $script:passed
        evidence_kind = 'SCRIPT HELPERS ONLY; fake command results; NOT a combined application build'
        cargo_invoked = $false; application_binaries_created = $false; desktop_probed = $false
        powershell_version = $PSVersionTable.PSVersion.ToString()
    })
    Write-Host "PASS: $script:passed helper assertions. No application build or desktop probe. Evidence: $scratch"
} catch {
    Write-JsonFile -Path (Join-Path $scratch 'helper-tests.json') -Value ([ordered]@{
        status = 'failed'; assertions_before_failure = $script:passed; error = $_.Exception.Message
        evidence_kind = 'SCRIPT HELPERS ONLY; NOT a combined application build'
    })
    throw
}
