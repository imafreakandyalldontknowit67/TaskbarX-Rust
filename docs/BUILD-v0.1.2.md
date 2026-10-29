# v0.1.2 validation

## Application checks

- x64: workspace tests, formatting, strict all-target Clippy, debug/release builds, and read-only taskbar diagnostics passed through `scripts/verify.ps1`.
- x86: genuine `i686-pc-windows-msvc` release build, workspace tests, and strict all-target Clippy passed. Read-only diagnostics detected the classic Windows 10 taskbar under WOW64 without warnings.
- Both architectures statically link the MSVC CRT. Source paths are remapped to stable virtual paths for distribution.
- The distribution scanner checked all four executables and the original banner for home-directory paths, credential-shaped strings, and an additional build-profile identifier. These checks passed; heuristic scans are not a security certification.

## Build and package checks

`verify.ps1` enables path remapping automatically and restores the caller's environment afterward. For manual cross-compilation, dot-source `scripts/set-build-path-remapping.ps1` first. `package-distribution.ps1` requires the privacy checker and rejects detected leaks before producing a successful package result.

Tests use synthetic inputs; they do not apply taskbar settings or register startup. Local build logs and evidence are not included in release archives.

## Compatibility limits

Windows 10 classic taskbars only. Modern Windows 11 taskbars, ARM builds, native 32-bit Windows runtime behavior, actual sign-in launch, and live Rainmeter installation remain unvalidated or unsupported as documented in the user guide. Automated tests and read-only detection do not establish that every visual effect works on every desktop.
