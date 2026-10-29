#Requires -Version 5.1
<#
.SYNOPSIS
Dot-source before Cargo builds to keep workstation paths out of distributable binaries.
.EXAMPLE
. ./scripts/set-build-path-remapping.ps1
./scripts/verify.ps1
.NOTES
Sets process-local Cargo flags only. Does not change persistent environment settings.
Includes static CRT linking because encoded flags override target rustflags.
#>
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\', '/')
$homeDirectory = [Environment]::GetFolderPath('UserProfile').TrimEnd('\', '/')
if (-not $homeDirectory) { throw 'Cannot determine the build profile directory.' }
$buildFlags = @('-C', 'target-feature=+crt-static', '-C', 'link-arg=/PDBALTPATH:%_PDB%')
# Rust source paths can use either separator spelling. Specific repository mappings
# follow the profile mappings so the source tree has a stable, useful virtual path.
foreach ($mapping in @(
    @($homeDirectory, '/build/user'),
    @($repository, '/src/taskbarx-rust')
)) {
    foreach ($spelling in @($mapping[0], $mapping[0].Replace('\', '/'))) {
        $buildFlags += "--remap-path-prefix=$spelling=$($mapping[1])"
    }
}
# Unit-separator encoding preserves spaces in real build paths.
$env:CARGO_ENCODED_RUSTFLAGS = $buildFlags -join [char]31
