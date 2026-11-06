# v0.1.4 validation

## Scope

Windows 10 classic taskbars. Dynamic recentering repairs an externally displaced, previously settled taskbar without restarting its easing animation when measured geometry and target are unchanged. The configured polling interval remains unchanged.

## Checks

- Native x64: locked workspace tests, strict all-target Clippy, release build of both executables, and read-only desktop diagnostics.
- x86: locked workspace tests, strict all-target Clippy, release build of both executables, and read-only diagnostics under WOW64.
- Formatting, distribution-script fixtures, mandatory privacy checks on executable inputs and archives, and SHA-256 checksums.
- Regression tests cover drift with toolbar compensation enabled/disabled, animated content changes, and preservation of in-flight animation. Existing measurement-gap and restoration tests remain enabled.
- Controlled live x64 check of the patch: a 30-pixel displacement returned directly from x=374 to x=344; polling observed the correction after 223 ms, with no intermediate animation positions. This was tested before the release version bump, on the same runtime patch.

## Limitations

The original intermittent movement and the specific Handy interaction have not been reproduced or attributed to a particular process. This fix repairs observed drift, not its external cause, and does not remove polling latency. Native 32-bit Windows, live x86 effects, Rainmeter installation and startup launch were not tested. Binaries are unsigned. Workstation paths are remapped and local build evidence is excluded from downloads.