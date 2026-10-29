#Requires -Version 5.1
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildHelpers.psm1') -Force
$root = Get-RepositoryRoot -ScriptDirectory $PSScriptRoot
# Helper fixtures are not application/release evidence. Keep them outside the
# checkout, retain their logs, and never remove previous runs' artifacts.
$scratchId = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ') + '-' + [Guid]::NewGuid().ToString('N')
$scratch = Join-Path ([IO.Path]::GetTempPath()) "TaskbarX-Rust-script-tests-$scratchId"
[void][IO.Directory]::CreateDirectory($scratch)
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

    $allocated = New-CargoTargetDirectory -Root $scratch
    $allocatedAgain = New-CargoTargetDirectory -Root $scratch
    Assert-Test ($allocated -ne $allocatedAgain -and (Test-Path -LiteralPath $allocated -PathType Container)) 'Default Cargo target directories are fresh and unique'
    $dynamicTarget = New-CargoTargetDirectory -Root $scratch -RequestedDirectory (Join-Path $scratch 'explicit cargo output [fixture]')
    Assert-Test ($dynamicTarget -eq (Join-Path $scratch 'explicit cargo output [fixture]')) 'Explicit safe CARGO_TARGET_DIR is respected'
    Assert-Throws { New-CargoTargetDirectory -Root $scratch -RequestedDirectory $dynamicTarget } 'new or empty'
    Assert-Throws { New-CargoTargetDirectory -Root $scratch -RequestedDirectory 'relative-output' } 'absolute path'
    Assert-Throws { New-CargoTargetDirectory -Root $scratch -RequestedDirectory 'C:drive-relative-output' } 'absolute path'
    Assert-Throws { New-CargoTargetDirectory -Root $scratch -RequestedDirectory '\root-relative-output' } 'absolute path'
    $emptyTarget = Join-Path $scratch 'existing empty build directory'
    [void][IO.Directory]::CreateDirectory($emptyTarget)
    Assert-Test ((New-CargoTargetDirectory -Root $scratch -RequestedDirectory $emptyTarget) -eq $emptyTarget) 'Explicit existing empty directory is safe to claim'
    foreach ($protected in @('target', 'target/debug', 'app', 'app/nested')) {
        Assert-Throws { New-CargoTargetDirectory -Root $scratch -RequestedDirectory (Join-Path $scratch $protected) } 'potentially running'
    }

    # Exercise the production gate orchestrator with a module-scoped fake native
    # runner. It cannot invoke Cargo or any application; restore the real runner
    # before the independent argument/log tests below.
    $module = Get-Module BuildHelpers
    $originalRunner = & $module { (Get-Item Function:Invoke-LoggedCommand).ScriptBlock }
    try {
        & $module {
            $script:fakeCalls = @()
            $script:fakeFailName = ''
            function script:Invoke-LoggedCommand {
                param($WorkingDirectory, $EvidenceDirectory, $FilePath, $ArgumentList, $Name)
                $script:fakeCalls += [pscustomobject]@{ Name = $Name; Arguments = @($ArgumentList); FilePath = $FilePath }
                if ($Name -eq $script:fakeFailName) { throw 'FAKE clippy failure' }
                return "FAKE output: $Name"
            }
        }
        $outputs = Invoke-WorkspaceValidation -Root $scratch -EvidenceDirectory $scratch
        $calls = @(& $module { $script:fakeCalls })
        Assert-Test ($calls.Count -eq 5 -and $outputs.Count -eq 5) 'All five required gates execute without a Clippy option'
        $expectedArgs = @{
            'cargo-fmt' = 'fmt --all -- --check'
            'cargo-test' = 'test --workspace --locked'
            'cargo-clippy' = 'clippy --workspace --all-targets --locked -- -D warnings'
            'cargo-build' = 'build --workspace --locked --message-format=json'
            'cargo-release' = 'build --release --workspace --locked --message-format=json'
        }
        foreach ($call in $calls) {
            Assert-Test ($call.FilePath -eq 'cargo' -and ($call.Arguments -join ' ') -eq $expectedArgs[$call.Name]) "Required gate arguments: $($call.Name)"
        }
        & $module { $script:fakeCalls = @(); $script:fakeFailName = 'cargo-clippy' }
        Assert-Throws { Invoke-WorkspaceValidation -Root $scratch -EvidenceDirectory $scratch } 'FAKE clippy failure'
        $calls = @(& $module { $script:fakeCalls })
        Assert-Test ($calls.Count -eq 3 -and $calls[-1].Name -eq 'cargo-clippy') 'Clippy failure prevents debug/release builds'
        & $module { $script:fakeCalls = @(); $script:fakeFailName = '' }
        $null = Invoke-ReadOnlyDiagnostic -FilePath (Join-Path $dynamicTarget 'debug/taskbarx.exe') -ArgumentList @('diagnose', '--json') -WorkingDirectory $scratch -EvidenceDirectory $scratch
        Assert-Test (@(& $module { $script:fakeCalls }).Count -eq 1) 'Exact diagnose --json passes the guarded fake runner'
        foreach ($arguments in @(@(), @('startup', 'status'), @('startup', 'enable'), @('shortcut'), @('start-menu'), @('run'), @('stop'), @('configure'), @('--help'), @('diagnose'), @('diagnose', '--json', '--extra'), @('diagnose', '--JSON'))) {
            Assert-Throws { Invoke-ReadOnlyDiagnostic -FilePath (Join-Path $dynamicTarget 'debug/taskbarx.exe') -ArgumentList $arguments -WorkingDirectory $scratch -EvidenceDirectory $scratch } 'Only the exact read-only'
        }
        Assert-Test (@(& $module { $script:fakeCalls }).Count -eq 1) 'Rejected probes never reach the command runner'
        Assert-Throws { Invoke-ReadOnlyDiagnostic -FilePath 'taskbarx-configurator.exe' -ArgumentList @('diagnose', '--json') -WorkingDirectory $scratch -EvidenceDirectory $scratch } 'Only the verified taskbarx'
    } finally {
        & $module { param($Runner) Set-Item Function:script:Invoke-LoggedCommand -Value $Runner } $originalRunner
    }

    # Fake in-memory Cargo results only; no Cargo manifests, binaries or lockfiles
    # are fabricated. These cannot be mistaken for integrated build evidence.
    $names = @('taskbarx-rust', 'taskbarx-core', 'taskbarx-win32', 'taskbarx-config', 'taskbarx-configurator', 'taskbarx-shell')
    $fake = [pscustomobject]@{
        workspace_root = $root
        target_directory = $dynamicTarget
        workspace_members = @($names | ForEach-Object { "fake-id:$_" })
        packages = @($names | ForEach-Object {
            $manifest = if ($_ -eq 'taskbarx-rust') { 'Cargo.toml' } else { "crates/$_/Cargo.toml" }
            $binary = if ($_ -eq 'taskbarx-rust') { 'taskbarx' } else { $_ }
            $kind = if ($_ -in @('taskbarx-rust', 'taskbarx-configurator')) { 'bin' } else { 'lib' }
            [pscustomobject]@{ name = $_; id = "fake-id:$_"; manifest_path = (Join-Path $root $manifest)
                targets = @([pscustomobject]@{ name = $binary; kind = @($kind) }) }
        })
    }
    Assert-WorkspaceMetadata -Metadata $fake -Root $root -TargetDirectory $dynamicTarget
    $script:passed++
    $missing = ConvertFrom-JsonEvidence -Text (ConvertTo-Json -InputObject $fake -Depth 20)
    $missing.workspace_members = @($missing.workspace_members | Where-Object { $_ -ne 'fake-id:taskbarx-config' })
    Assert-Throws { Assert-WorkspaceMetadata -Metadata $missing -Root $root } 'package missing'
    $missingShell = ConvertFrom-JsonEvidence -Text (ConvertTo-Json -InputObject $fake -Depth 20)
    $missingShell.workspace_members = @($missingShell.workspace_members | Where-Object { $_ -ne 'fake-id:taskbarx-shell' })
    Assert-Throws { Assert-WorkspaceMetadata -Metadata $missingShell -Root $root } 'taskbarx-shell'
    Assert-Throws { Assert-WorkspaceMetadata -Metadata $fake -Root $root -TargetDirectory $allocated } 'different target directory'
    $wrong = ConvertFrom-JsonEvidence -Text (ConvertTo-Json -InputObject $fake -Depth 20)
    $wrong.packages[0].targets[0].name = 'wrong-engine-name'
    Assert-Throws { Assert-WorkspaceMetadata -Metadata $wrong -Root $root } 'binary target missing'
    $artifactLines = @(
        [pscustomobject]@{ reason = 'compiler-artifact'; package_id = 'fake-id:taskbarx-rust'; target = @{ name = 'taskbarx'; kind = @('bin') }; executable = (Join-Path $dynamicTarget 'release/taskbarx.exe') },
        [pscustomobject]@{ reason = 'compiler-artifact'; package_id = 'fake-id:taskbarx-configurator'; target = @{ name = 'taskbarx-configurator'; kind = @('bin') }; executable = (Join-Path $dynamicTarget 'release/taskbarx-configurator.exe') },
        [pscustomobject]@{ reason = 'build-finished'; success = $true }
    )
    $fakeOutput = ($artifactLines | ForEach-Object { ConvertTo-Json -InputObject $_ -Depth 10 -Compress }) -join "`n"
    $paths = Get-WorkspaceBinaryPaths -BuildOutput $fakeOutput -Metadata $fake -Root $root -Profile release
    Assert-Test ($paths.Count -eq 2) 'Both compiler-reported binaries must be selected'
    Assert-Test ($paths['taskbarx'] -eq (Join-Path $dynamicTarget 'release/taskbarx.exe')) 'Artifacts use Cargo metadata target directory instead of root/target'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput $fakeOutput -Metadata $fake -Root $root -Profile debug } 'Unexpected binary path'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput '{"reason":"build-finished","success":true}' -Metadata $fake -Root $root -Profile release } 'did not report'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput ($fakeOutput + "`n" + $fakeOutput) -Metadata $fake -Root $root -Profile release } 'exactly one'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput ($fakeOutput.Replace($dynamicTarget.Replace('\', '\\'), $allocated.Replace('\', '\\'))) -Metadata $fake -Root $root -Profile release } 'Unexpected binary path'
    Assert-Throws { Get-WorkspaceBinaryPaths -BuildOutput ($fakeOutput.Replace('"success":true', '"success":false')) -Metadata $fake -Root $root -Profile release } 'successful build-finished'

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
    # Copied entry points have no workspace at all: fail before invoking Cargo,
    # creating build output, or running an application. Do not create fake manifests.
    $incomplete = Join-Path $scratch 'incomplete workspace fixture'
    $incompleteScripts = Join-Path $incomplete 'scripts'
    [void][IO.Directory]::CreateDirectory($incompleteScripts)
    foreach ($name in @('BuildHelpers.psm1', 'verify.ps1', 'release.ps1')) {
        Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $incompleteScripts $name)
    }
    foreach ($entryPoint in @('verify', 'release')) {
        Assert-Throws { Invoke-LoggedCommand @common -Name "$entryPoint-incomplete" -ArgumentList @('-NoProfile', '-NonInteractive', '-File', (Join-Path $incompleteScripts "$entryPoint.ps1")) } 'exit 1'
        $refusal = Get-Content -LiteralPath (Join-Path $scratch "$entryPoint-incomplete.stderr.txt") -Raw
        Assert-Test ($refusal -match 'Cargo.lock' -and $refusal -match 'taskbarx-shell/Cargo.toml') "$entryPoint requires both integrated lockfile and shell crate"
    }
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $incomplete 'dist')) -and -not (Test-Path -LiteralPath (Join-Path $incomplete 'Cargo.lock'))) 'Incomplete entry points create no evidence, target directory, package or lockfile'
    Assert-Throws { Assert-X64Executable -Path $child } 'Not a PE executable'
    Assert-Throws { Assert-X64Executable -Path (Join-Path $scratch 'never-created.exe') } 'missing'
    if ([Environment]::Is64BitProcess) { Assert-X64Executable -Path $shell; $script:passed++ }
    $sample = ConvertFrom-JsonEvidence -Text (Get-Content -LiteralPath (Join-Path $root 'docs/settings.sample.json') -Raw)
    Assert-Test ($sample.schema_version -eq 1 -and $sample.color.Count -eq 4 -and $sample.color[3] -eq 128 -and $sample.animation -eq 'cubiceaseinout') 'Documented sample defaults'
    Assert-Test ($sample.center -and $sample.primary -and $sample.secondary -and -not $sample.center_between -and $sample.primary_offset -eq 0 -and $sample.secondary_offset -eq 0 -and $sample.animation_ms -eq 300 -and $sample.poll_ms -eq 400 -and $sample.style -eq 'default' -and ($sample.color -join ',') -eq '0,0,0,128') 'Complete sample defaults'
    $newDefaults = [ordered]@{
        primary_opacity = $null
        secondary_opacity = $null
        corner_radius = 0
        taskbar_segments = $false
        default_style_on_maximized = $false
        battery_animation = $null
        battery_poll_ms = $null
        skip_resolutions = @()
        pause_on_fullscreen = $false
        hide_primary_start = $false
        hide_secondary_start = $false
        hide_primary_tray = $false
        hide_secondary_tray = $false
        revert_on_tray_overlap = $false
        toolbar_compensation = $true
        tray_icon = $false
        startup_delay_secs = 6
        measurement = 'msaa'
        show_console = $false
    }
    foreach ($name in $newDefaults.Keys) {
        $expected = $newDefaults[$name]
        $actual = $sample.$name
        $matches = if ($null -eq $expected) { $null -eq $actual }
            elseif ($expected -is [array]) { $actual -is [array] -and $actual.Count -eq 0 }
            else { $null -ne $actual -and $actual.GetType() -eq $expected.GetType() -and $actual -ceq $expected }
        Assert-Test $matches "Frozen sample default and JSON type: $name"
    }
    $expectedFields = @('schema_version', 'center', 'primary', 'secondary', 'center_between', 'primary_offset', 'secondary_offset', 'animation', 'animation_ms', 'poll_ms', 'style', 'color') + @($newDefaults.Keys)
    Assert-Test ((@($sample.PSObject.Properties.Name | Sort-Object) -join ',') -ceq (@($expectedFields | Sort-Object) -join ',')) 'Sample contains exactly the frozen configuration fields'

    # Packaging-helper fixture is text-only, not a dummy application release.
    $payload = Join-Path $scratch 'text-only payload [fixture]'
    [void][IO.Directory]::CreateDirectory((Join-Path $payload 'nested'))
    Write-Utf8File -Path (Join-Path $payload 'fixture.txt') -Text 'SCRIPT HELPER TEST ONLY'
    Write-Utf8File -Path (Join-Path $payload 'nested/second.txt') -Text 'not an application binary'
    $artifactSource = Join-Path $dynamicTarget 'compiler-reported-fixture.txt'
    Write-Utf8File -Path $artifactSource -Text 'TEXT ONLY: selected artifact packaging fixture'
    Copy-ArtifactPayload -Artifacts ([ordered]@{ fixture = $artifactSource }) -Directory $payload
    Assert-Test ((Get-Sha256 -Path $artifactSource) -eq (Get-Sha256 -Path (Join-Path $payload 'compiler-reported-fixture.txt'))) 'Packaging copies the selected dynamic artifact, byte-for-byte'
    Assert-Throws { Copy-ArtifactPayload -Artifacts ([ordered]@{ fixture = $artifactSource }) -Directory $payload } 'overwrite a packaged artifact'
    $checksums = Write-PayloadChecksums -Directory $payload
    $lines = @(Get-Content -LiteralPath $checksums)
    Assert-Test ($lines.Count -eq 3) 'Checksum manifest excludes itself'
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
    try { Assert-Test ($archive.Entries.Count -eq 4) 'ZIP has all selected text fixtures and payload checksums' }
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
