# Integration validation

The independent review findings were addressed in the integrated source:
- Movement ownership survives temporary discovery gaps.
- Animation scheduling uses elapsed duration, including bounce/elastic target crossings.
- Stop waits for a positive cleanup-success acknowledgment; crashes/failed cleanup are not reported as success to a waiting stopper.
- Accent restoration follows the surviving root window rather than replaceable task-list children.
- Successful appearance fallback is logged as a warning, not returned as cleanup failure.
- Positively destroyed Explorer identities are discarded without restoring stale handles.

Regression coverage is part of the workspace test suite. Desktop checks are read-only.
The configurator was opened and its settings-loaded state and controls inspected through accessibility; screenshot capture timed out, so visual layout was not verified. It was closed without saving or applying settings.

Live centering, styling, stop/restoration, monitor changes, and Explorer restart remain manually unverified. See TESTING.md. Release evidence is generated under dist by scripts/release.ps1.

For the local built copy, double-click `Open Settings.cmd` in the project folder, or open `app/taskbarx-configurator.exe`. Apply/start explicitly changes the taskbar; opening the settings window does not.
