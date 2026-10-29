# TaskbarX-Rust v0.1.0

## Downloads

**Prefer an architecture-specific portable ZIP if actually attached to the [v0.1.0 release page](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/tag/v0.1.0).** The repository remains **private**; downloads require access. These are planned filenames, not confirmation that any new package has been built or uploaded:

| Architecture | Primary portable ZIP, if available | Optional Rainmeter package, if available |
| --- | --- | --- |
| x64 | `TaskbarX-Rust_0.1.0_x64.zip` | `TaskbarX-Rust_0.1.0_x64.rmskin` |
| x86 | `TaskbarX-Rust_0.1.0_x86.zip` | `TaskbarX-Rust_0.1.0_x86.rmskin` |
| ARM64 | `TaskbarX-Rust_0.1.0_arm64.zip` | None |

**x86 and ARM64 are build candidates, not runtime verified.** Packaging support, successful compilation, runtime verification and uploaded availability are separate. Root/release owner decides which assets can publish. See [distribution selection, layouts and publication gates](DISTRIBUTION.md).

The ZIP contract includes both EXEs together at its root, `Open Settings.cmd`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `settings.sample.json`, a concise `README.txt` and `docs/licenses/`. Repository guides, source, artwork, upstream .NET DLLs/resources and private developer evidence are not bundled.

GitHub's automatic **Source code (zip)** and **Source code (tar.gz)** archives are source, **not the ready-to-run app**. They cannot be removed like uploaded release assets.

## Use

- Extract **all files** from the selected ZIP into a stable folder. Open `Open Settings.cmd` or `taskbarx-configurator.exe`; opening settings **does not start the engine**.
- **Save** only persists JSON. **Apply** saves settings and starts/restarts the engine; it **does not enable startup**.
- To start at sign-in, separately choose **Startup > Enable / update** using saved settings; the default delay is **six seconds**.
- Settings default to `%APPDATA%\TaskbarX-Rust\settings.json`, not the portable folder.
- Closing the GUI leaves a running engine running. **Stop TaskbarX** requests graceful restoration, not guaranteed crash recovery or exact restoration of third-party effects.

Optional Rainmeter: install Rainmeter separately, install an available matching `.rmskin`, manually load `Launcher.ini`, then explicitly click its settings launcher. Installation/loading/refresh does not launch the configurator, apply the engine or register TaskbarX startup. The click only opens settings; Apply and Startup remain later actions. Rainmeter's own launch-at-sign-in setting is not engine startup. See the [Rainmeter workflow and official references](DISTRIBUTION.md#optional-rainmeter-workflow); live Rainmeter installation has not been verified here.

Before updates/removal, stop and confirm completion, back up settings and close the GUI before replacing files. Explicitly disable/update owned startup before moving executable/config paths, including those under Rainmeter; re-enable/update from the new location if wanted. Upstream tasks are not automatically migrated or removed. See the [user guide](USER-GUIDE.md#quick-use).

## Scope and limitations

- Initial release, **not a prerelease**. Both binaries are **unsigned**; keep Windows security checks enabled.
- Existing tested scope is **native Windows 10 x64 with the classic Explorer taskbar**, not comprehensive feature certification. Modern Windows 11 taskbars are **unsupported**; shell patching/elevation are not promised workarounds.
- Architecture matching does not establish shell compatibility. There is no neutral/AnyCPU native executable or supported ARM32 package; no emulation support is promised.
- Startup enable/status was verified, but **actual logon launch and most visual effects have not been live-tested**. No visual-parity or comprehensive live-testing claim.
- Live positioning/restoration, performance, real shortcuts/pinning and tray interaction are not established by automated tests or the limited startup repair; see [compatibility and evidence](USER-GUIDE.md#compatibility-and-evidence).

## Attribution

Independent Rust rewrite of Chris Andriessen's TaskbarX, not an official upstream release. [MIT license](../LICENSE), Copyright (c) 2021 Chris Andriessen; preserve [third-party notices](../THIRD_PARTY_NOTICES.md) and [dependency license texts](licenses/README.md). Upstream-like package choices do not require upstream ModernWPF, TaskScheduler or Emoji.Wpf DLLs/resource folders; never relabel upstream binaries as Rust builds. This is not a claim that every system dependency is statically linked or that a package format confers endorsement. Repository artwork depicts upstream, not this Rust build, and is excluded from these packages.
