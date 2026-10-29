# TaskbarX-Rust v0.1.3

## Downloads

| Architecture | Portable ZIP | Optional Rainmeter launcher |
| --- | --- | --- |
| x64 — recommended for 64-bit Intel/AMD Windows | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x64.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x64.rmskin) |
| x86 — limited runtime validation | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x86.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x86.rmskin) |

Each ZIP includes both applications, **Open Settings.cmd**, instructions, sample settings, and license notices. Extract the whole ZIP into a stable folder, then open **Open Settings.cmd** or **taskbarx-configurator.exe**. SHA256SUMS.txt covers all four packages.

## Changes

- Hold the current centered position when Explorer temporarily returns empty or ambiguous taskbar measurements, rather than resetting and centering again.
- Read-only discovery failures now suspend animation and retry without resetting positions or styles. Stop, disabled centering, and mutation-error cleanup still restore normally.
- Added regression tests for measurement gaps, recovery, discovery failures after centering, and explicit restoration.
- Preserved privacy-hardened builds and complete x64/x86 ZIP and Rainmeter packages.

This addresses a confirmed reset/recenter path found while investigating a reported Ctrl+G/Handy overlay jump. The specific Handy interaction has not been reproduced or live-verified; other causes of movement are not ruled out.

## Use and compatibility

**Windows 10 classic taskbars only.** Modern Windows 11 taskbars are unsupported. x64 passed native checks; x86 passed builds/tests and read-only detection under WOW64, not native 32-bit Windows validation. ARM builds are unavailable.

Opening settings does not start the engine. **Apply** saves and starts/restarts it; **Startup > Enable / update** separately enables sign-in startup. Actual logon launch and most visual effects have not been live-tested.

Rainmeter is optional. Install the matching `.rmskin`, manually load `TaskbarX-Rust-<arch>/Launcher.ini`, then click **Open Settings**. Loading the skin does not apply settings or enable startup. Archive structure is tested; live installation remains untested.

Binaries are unsigned; keep Windows security checks enabled. The repository remains private. See the [user guide](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.3/docs/USER-GUIDE.md) and [validation summary](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.3/docs/BUILD-v0.1.3.md).

Independent Rust rewrite of Chris Andriessen's [TaskbarX](https://github.com/ChrisAnd1998/TaskbarX), not an official upstream release.
