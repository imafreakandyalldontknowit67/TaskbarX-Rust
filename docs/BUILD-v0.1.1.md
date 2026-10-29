# v0.1.1 build and compatibility evidence

Build checks completed on Windows 10 build 19045 (x64), 2026-10-29, with Rust 1.99.1 and the locked dependency graph. These checks do not certify every visual effect or every Windows configuration.

## Published architecture scope

- **x64:** native workspace tests, strict all-target Clippy, formatting, debug/release builds and read-only taskbar diagnostics passed through `scripts/verify.ps1`.
- **x86:** genuine `i686-pc-windows-msvc` release build; workspace tests and strict all-target Clippy passed under WOW64 on the x64 host. Read-only diagnostics detected the classic taskbar without warnings. Native 32-bit Windows, live positioning/effects and startup at logon were not tested.
- **ARM64:** compilation reached the link stage, but the required Microsoft ARM64 linker/CRT tools are absent. Adding them requires an elevated installer. No ARM64 binary is published, and no ARM64 runtime compatibility is claimed.
- **ARM32 / neutral:** no supported ARM32 build or .NET-style AnyCPU native Rust executable. We do not rename x64 binaries to pretend otherwise.

Both published architectures statically link the MSVC CRT using `.cargo/config.toml`. Import-table inspection of all four release executables found Windows system DLLs, but no `VCRUNTIME140.dll` or separate CRT DLL dependency. Upstream .NET libraries, language-resource DLLs and .NET runtime installers are not needed or bundled. This is not a claim that Windows system dependencies are unnecessary.

## Changes

- Correct pointer-sized window userdata arguments for x86 Win32 bindings.
- Avoid unaligned references to the packed x86 tray notification structure.
- Align workspace package versions at 0.1.1.
- Add architecture-checked portable ZIPs and optional Rainmeter launcher packages.

The compiler intermittently crashed while compiling Windows dependencies during early attempts. Successful final build/test/lint runs were completed afterward; failed attempts are retained locally, not treated as passes.

## Limits

Modern Windows 11 taskbars remain unsupported. Existing startup enable/status checks are not an actual sign-in test. Rainmeter archives are structurally checked against the official format; installation and desktop launcher interaction have not been live-tested. The launcher only opens settings on an explicit click; Apply and startup registration remain separate actions.

Release archives exclude local diagnostics, machine-specific build metadata, source trees and upstream artwork. SHA256SUMS verifies download consistency, not a digital signature. These binaries are unsigned. The repository remains private.
