# v0.1.3 validation

## Scope

Windows 10 classic taskbars. This release changes runtime handling of transient measurement/discovery failures; it does not intercept Ctrl+G or change Handy settings.

## Checks

- Native x64: workspace tests, formatting, all-target Clippy with warnings denied, debug/release builds, and read-only desktop diagnostics.
- x86: workspace tests under WOW64, all-target Clippy with warnings denied, release builds, and read-only diagnostics. Not native 32-bit Windows validation.
- Distribution script regression suite, mandatory privacy checks on both executables and final archives, and SHA-256 checksums.
- Regression cases cover empty/invalid measurements, recovery without a reset, discovery failure after successful centering, fullscreen resume, and explicit restoration. Existing partial-mutation failure and shutdown cleanup tests remain enabled.

## Limitations

The reported Ctrl+G/Handy overlay interaction has not been reproduced or live-verified. The reset/recenter behavior on failed measurements is confirmed by code inspection and regression tests, not claimed as the only possible cause. No live startup, Rainmeter installation, or native x86 desktop validation was performed. Binaries are unsigned. Source paths are remapped; diagnostic evidence stays local and is not included in downloads.
