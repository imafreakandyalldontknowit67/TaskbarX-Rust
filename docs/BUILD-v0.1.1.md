# v0.1.1 build and compatibility evidence

Historical v0.1.1 build checks completed on Windows 10 build 19045 (x64), 2026-10-29, with Rust 1.99.1 and the locked dependency graph. These results apply only to [v0.1.1](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/tag/v0.1.1), not the planned v0.1.2 release. They do not certify every visual effect or Windows configuration.

## Historical architecture scope

- **x64:** native workspace tests, strict all-target Clippy, formatting, debug/release builds and read-only taskbar diagnostics passed through `scripts/verify.ps1`.
- **x86:** genuine `i686-pc-windows-msvc` release build; workspace tests and strict all-target Clippy passed under WOW64 on the x64 host. Read-only diagnostics detected the classic taskbar without warnings. Native 32-bit Windows, live positioning/effects and startup at logon were not tested.
- **ARM64:** compilation reached the link stage but did not produce a completed build. No ARM64 binary is available, and runtime compatibility is unvalidated.
- **ARM32 / neutral:** no supported ARM32 build or .NET-style AnyCPU native Rust executable.

Both v0.1.1 architectures statically linked the MSVC CRT using `.cargo/config.toml`. Import-table inspection of all four release executables found Windows system DLLs, but no `VCRUNTIME140.dll` or separate CRT DLL dependency. Upstream .NET libraries, language-resource DLLs and .NET runtime installers were not needed or bundled. Windows system dependencies are still required.

## Changes

- Correct pointer-sized window userdata arguments for x86 Win32 bindings.
- Avoid unaligned references to the packed x86 tray notification structure.
- Align workspace package versions at 0.1.1.
- Add architecture-checked portable ZIPs and optional Rainmeter launcher packages.

The compiler intermittently crashed while compiling Windows dependencies during early attempts. Successful final build/test/lint runs were completed afterward; failed attempts are retained locally, not treated as passes.

## Limits

Support is limited to the Windows 10 classic taskbar; Windows 11 is unsupported. Historical startup Enable/update and status checks are not an actual sign-in test. Most visual effects remain untested. Rainmeter archives were structurally checked against the official format; installation and desktop launcher interaction have not been live-tested. The launcher only opens settings on an explicit click; Apply and startup registration remain separate actions.

The v0.1.1 distribution archives excluded local diagnostics, machine-specific build metadata, source trees and upstream artwork. SHA256SUMS verifies download consistency, not a digital signature. These binaries were unsigned.

Packaging validation: 2,035 inert-fixture assertions passed on PowerShell 5.1; privileged file-symlink fixtures were skipped. All four real archives passed independent CRC, entry uniqueness, embedded-binary byte equality, PE architecture and (for Rainmeter) footer checks. These are not live Rainmeter tests.
