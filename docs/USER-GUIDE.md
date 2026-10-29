# TaskbarX-Rust user guide

An **independent Rust rewrite** of [ChrisAnd1998/TaskbarX](https://github.com/ChrisAnd1998/TaskbarX), originally by Chris Andriessen. Not an official upstream release or a binary-compatible replacement for its CLI.

## Quick use

**v0.1.2 is unsigned.** Download the matching complete package from the [release page](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/tag/v0.1.2). Keep Windows security checks enabled.

**Windows 10 classic taskbar only; Windows 11 is unsupported.** Choose x64 for 64-bit Intel/AMD Windows. v0.1.2 x86 checks ran under WOW64, not native 32-bit Windows. ARM64 is unavailable and unvalidated. See [package selection](DISTRIBUTION.md) and [v0.1.2 evidence](BUILD-v0.1.2.md).

1. Once available, download the matching ZIP and **extract all files into a stable folder**. Keep both EXEs beside each other; do not run from inside the archive.
2. Open `Open Settings.cmd` or `taskbarx-configurator.exe`. Opening settings does not start the engine.
3. Select settings, then press **Apply** to save and start/restart the engine. **Save** only persists JSON. Neither enables startup.
4. If wanted, separately choose **Startup > Enable / update**. This uses the selected saved configuration and the delay shown (default **six seconds**); it does not save the form or start the engine. Editing the delay alone does not update registration.
5. Closing the GUI leaves the engine running. Use **Stop TaskbarX** for a graceful stop and attempted restoration; crashes/force termination can bypass cleanup, and exact restoration of third-party effects is not guaranteed.

The ZIP layout includes both EXEs, `Open Settings.cmd`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, root-level `settings.sample.json`, a concise `README.txt` and `docs/licenses/`. Repository guides, source, artwork and local build evidence are not bundled. GitHub's automatic **Source code (zip)** and **Source code (tar.gz)** archives contain source, **not the ready-to-run application**.

For an optional available `.rmskin`, install Rainmeter separately, install the package, manually load `Launcher.ini`, then explicitly click its settings launcher. Installation/loading/refresh does not launch the configurator, apply the engine or register TaskbarX startup. The click only opens settings; Apply and Startup remain distinct later actions. Rainmeter's own launch-at-sign-in setting is not TaskbarX engine startup. Follow the [Rainmeter workflow and official references](DISTRIBUTION.md#optional-rainmeter-workflow).

Settings default to `%APPDATA%\TaskbarX-Rust\settings.json`, not the portable binaries' folder. Use `--config PATH` to select another file. For example, from the folder containing both EXEs, `.\taskbarx-configurator.exe --config 'C:\Tools\TaskbarX-Rust\settings.json'` opens the editor for that path without starting the engine.

**Update or remove:** stop first and confirm completion; preserve/back up settings. Explicitly disable the owned startup task before moving/removing executable or config paths, then use **Enable / update** from the new location if desired. Close the GUI before replacing the package, including both EXEs. This also applies before updating/removing a Rainmeter skin whose `@Resources/TaskbarX-Rust/` paths are registered for startup; removing the skin does not unregister TaskbarX. Remove owned custom shortcuts explicitly on **Start Button**, then manually remove the portable folder if uninstalling. No application folder, unrelated task or upstream TaskbarX task is automatically deleted or migrated.

Reference: [compatibility and evidence](#compatibility-and-evidence), [architecture](#architecture), [safeguards](#safety-and-behavior), [GUI actions](#configurator-actions), [commands and all options](#commands), [settings/defaults](#settings), [building and verification](#build-verify-and-package-locally), [attribution](#license-and-attribution).

## Compatibility and evidence

v0.1.2 checks covered **native Windows 10 x64 with the classic Explorer taskbar** and x86 build/tests and read-only detection under WOW64 on Windows 10 x64. Native 32-bit Windows and live effects remain untested. ARM64 is unavailable and unvalidated; there is no supported ARM32 or neutral/AnyCPU package. See [v0.1.2 build evidence](BUILD-v0.1.2.md) and the [architecture/verification matrix](DISTRIBUTION.md#architecture-and-verification). Packaging support does not establish runtime compatibility or download availability. Windows 11, including classic-shell modifications, is unsupported; elevation is not a compatibility workaround. Microsoft Store packaging/protocol activation is excluded.

The [parity inventory (v0.1.2 source)](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.2/docs/PARITY.md) distinguishes implemented source mappings, dormant upstream code and deliberate differences. It is not a visual-parity claim. Automated release checks cover pure/fake tests, build/script validation and read-only `diagnose --json`; they do not establish live taskbar effects or shell behavior.

**startup Enable/update and status checks verified the account-name-to-SID fix (`ac69090`).** The task read back as registered, enabled and owned, with a six-second delay and no warnings. This verifies registration, not actual logon launch. **Actual launch at sign-in, most visual effects and live Rainmeter interaction have not been tested.** Live positioning/restoration, visual parity, performance, real shortcuts/pinning and tray interaction are not established by the automated checks or that startup repair.

See [testing](TESTING.md), [validation status](VALIDATION.md) and the [local validation checklist](INTEGRATION.md). Versioned source links target the v0.1.2 tag. Repository guides are not bundled; local evidence packages are distinct from distribution archives. Earlier results do not substitute for the version-specific validation summary.

## Architecture

| Package | Responsibility |
| --- | --- |
| `taskbarx-rust` | `taskbarx.exe`: CLI, runtime policies, animation scheduling, single-instance coordination and graceful shutdown |
| `taskbarx-core` | Pure signed-coordinate geometry and 42 easing selections |
| `taskbarx-win32` | Classic-shell discovery, MSAA/UI Automation measurement, owned mutations and restoration |
| `taskbarx-config` | Shared validated schema-1 JSON settings and atomic persistence |
| `taskbarx-shell` | Explicit current-user startup, owned shortcuts, Start-menu action and queued tray actions |
| `taskbarx-configurator` | `taskbarx-configurator.exe`: settings editor and explicit engine/shell actions |

The upstream VB.NET application is reference material, not a Rust build input or a source of executable files to rename and redistribute.

## Safety and behavior

- Opening help/configuration and reading settings never starts the engine. Save persists only. Apply/start explicitly saves, requests graceful stop, then starts. Failed stop is not bypassed by force-killing or duplicate launch.
- **Save, Apply, run and startup-delay preference changes never register startup.** Only explicit Enable/Disable changes the rewrite's owned startup registration. Upstream tasks are not migrated or removed.
- Failed/empty/ambiguous accessibility measurements skip movement rather than guess bounds. MSAA is default; `uia` selects a real alternate provider, not an alias. Individual out-of-process accessibility calls can block despite traversal budgets.
- Full-taskbar and free-span centering use signed coordinates. Independent offsets apply along the taskbar axis: positive right/down, negative left/up, within available space.
- Styles are `default`, `transparent`, **`transparent-gradient`**, `blur`, `acrylic`, `opaque`. Transparent and transparent-gradient are distinct. Composition APIs are undocumented and build/theme dependent.
- Capture opacity, region and control visibility before owned changes. Third-party accent state cannot always be captured or exactly restored; fallback requests Windows-managed appearance. Avoid competing taskbar utilities.
- Graceful shutdown attempts cleanup. Hard termination, crashes and power loss can bypass it. Perfect restoration is not guaranteed. Explorer recovery discards destroyed identities, never restoring reused handles.
- Width exceptions suspend **centering only**, based on the primary monitor width. Fullscreen pause suspends accessibility polling/taskbar mutation while keeping shutdown responsive. Null battery overrides inherit normal settings.
- Tray callbacks queue actions for the runtime, not reentrant taskbar work. Phantom-tray cleanup, remote Explorer memory writes and process-name killing are not ported.

## Configurator actions

| Control | Effect |
| --- | --- |
| Open / **Reload** | Read saved settings; no engine start or taskbar changes. Invalid input reports an error instead of silently replacing the form with defaults. |
| **Reset** | Reset the local form only; Save or Apply is still required. |
| **Save** | Validate and atomically persist JSON; leave the running engine unchanged. |
| **Apply** | Save, request graceful stop, then launch using saved settings. If stop fails, no replacement runner is launched. Startup is never enabled or updated. |
| **Restart saved** | Stop/restart using saved settings, without saving the current form or changing startup. |
| **Stop TaskbarX** | Request graceful stop and restoration of owned changes; this is not a crash-recovery guarantee. |
| **Startup > Refresh status** | Read-only registration query; opening the page does not register anything. |
| **Startup > Enable / update** | Explicitly register the selected saved config and selected delay; do not save the form or restart/run the engine. |
| **Startup > Disable / remove** | Remove the rewrite's owned startup task; preserve settings and the running engine. |
| **Mechanics > Read-only diagnostics** | Show the engine's compatibility/measurement report, not a GUI support guarantee. |
| **Animation > Preview ordinary / Preview battery** | Animate only a local slider, never the taskbar. |
| **Start Button** | Show a command, explicitly create/remove an owned `.lnk`, or explicitly request **Open Start now**. Pinning is manual. |
| Close window | Leave the engine running. |

A launched process is not proof of health or compatibility. Read reported errors; do not assume a timed-out operation succeeded or retry by force-killing. The **Uninstall** page exposes separate owned-startup removal and graceful-stop actions; it preserves settings and does not delete an application folder.

## Commands

No arguments, `help` and `--help` are safe. Legacy forms such as `-tbs=1`, `-ptbo=0`, `-as=...` and `-stop` are not aliases.

```text
taskbarx.exe
taskbarx.exe --help
taskbarx.exe --version
taskbarx.exe run [--config PATH] [--background] [OPTIONS]
taskbarx.exe stop [--timeout-ms N]
taskbarx.exe diagnose [--json] [--config PATH]
taskbarx.exe config show [--config PATH]
taskbarx.exe config import --stdin [--config PATH]
taskbarx.exe configure [--config PATH]
taskbarx.exe startup status [--json]
taskbarx.exe startup enable [--config PATH] [--delay-secs N] [--json]
taskbarx.exe startup disable [--json]
taskbarx.exe start-menu
taskbarx.exe shortcut create --path PATH [--icon PATH] [--json]
taskbarx.exe shortcut remove --path PATH [--json]
taskbarx-configurator.exe [--engine PATH] [--config PATH]
```

Use `taskbarx.exe help`, a command's `--help` (for example `taskbarx.exe run --help`), or `taskbarx-configurator.exe --help` for usage. These do not start the engine.

### All run options

`BOOL` means an explicit `true` or `false`. Only `--center-between` also accepts omission of its value to mean `true`; flag-only options are shown without a value. Options override the loaded configuration for that run only.

| `run` option | Meaning |
| --- | --- |
| `--config PATH` | Load this JSON file instead of the default path |
| `--primary-offset N`, `--secondary-offset N` | Signed offsets |
| `--animation NAME`, `--animation-ms N` | Easing/duration |
| `--poll-ms N` | Normal polling interval |
| `--style NAME`, `--color R,G,B,A` | Appearance; four bytes, each 0-255 |
| `--no-center` | Disable centering independently of appearance |
| `--center BOOL` | Explicit centering value; conflicts with `--no-center` |
| `--only-primary` / `--only-secondary` | Mutually exclusive taskbar selection |
| `--primary BOOL`, `--secondary BOOL` | Select taskbars for centering and styling; conflict with either `--only-*` flag |
| `--center-between [BOOL]` | Center between leading controls and tray |
| `--background` | Detach from the console unless `show_console` is true; never enables startup |
| `--primary-opacity N`, `--secondary-opacity N` | Total taskbar opacity percent, 0-100; `default` or `inherit` clears the configured override |
| `--corner-radius N` | Corner radius in physical pixels |
| `--taskbar-segments BOOL` | Segmented taskbar region policy |
| `--default-style-on-maximized BOOL` | Default-style policy for the relevant monitor when maximized |
| `--battery-animation NAME` | Battery easing, or `inherit` / `default` to use the normal easing |
| `--battery-poll-ms N` | Battery polling interval, or `inherit` / `default` to use the normal interval |
| `--skip-resolution N` | Replace configured primary-monitor widths; repeat at most three times |
| `--clear-skip-resolutions` | Clear width exceptions; conflicts with `--skip-resolution` |
| `--pause-on-fullscreen BOOL` | Pause polling/mutation on fullscreen applications |
| `--hide-primary-start BOOL`, `--hide-secondary-start BOOL` | Hide selected Start controls |
| `--hide-primary-tray BOOL`, `--hide-secondary-tray BOOL` | Hide selected tray areas |
| `--revert-on-tray-overlap BOOL` | Revert centering on tray overlap |
| `--toolbar-compensation BOOL` | Compensate for toolbar/tray size changes |
| `--tray-icon BOOL` | Show the engine notification icon with queued settings/restart/stop actions |
| `--measurement msaa` / `--measurement uia` | Accessibility provider |
| `--show-console BOOL` | Engine console preference |

`run` overrides are not persisted. User settings are editable in the GUI; JSON import also includes the schema version. `run` normally stays in the foreground until stopped or interrupted. `stop` requests graceful cleanup for this user's runtime in this session, with a timeout of 1-60000 ms (default 5000). `config show` does not create a missing file; missing settings mean in-memory defaults, while permission/validation errors are not hidden. `config import --stdin` validates at most 64 KiB of UTF-8 JSON and saves without applying. `configure` opens the sibling GUI; keep both executables together or supply the GUI's `--engine PATH` (it never searches PATH for the engine).

### Startup

Startup status is read-only. Startup JSON is the direct `StartupStatus` object (`registered`, `enabled`, `owned`, `task_name`, `launch`, `delay_secs`, `warnings`). Enable/Disable reports OS errors, not false success. Enabling registers a current-user launch with the selected config/delay; it does not immediately run it. CLI `--delay-secs` accepts 0-86400 seconds; when omitted it uses the saved `startup_delay_secs` preference (default six). Keep executable/config paths stable, or explicitly update/disable registration before moving the folder.

The task name `TaskbarX-Rust.Startup.v1.<current-user-SID>` and ownership marker include the current user's SID; its sole logon trigger uses an interactive, least-privilege principal, permits battery operation and has a validated delay. Inaccessible status is an error, not "absent"; a disabled task or logon trigger reports effective `enabled:false`. Updates validate the complete prepared XML first. Failed-update rollback only replaces that exact prepared definition and re-verifies the prior definition; interference or inaccessible state produces an honest failure, not an overwrite or a false restoration claim. These safeguards are not an atomic guarantee against every external writer. Foreign tasks and upstream TaskbarX registrations are not replaced, migrated or removed.

### Start shortcuts, icons and shell limitations

`start-menu` explicitly requests Start, not centering. Success confirms bounded `SC_TASKLIST` message delivery, not a visual-open assertion. Shortcut creation writes an owned `.lnk` to that action, optionally with a user-supplied icon. Unrelated existing files/shortcuts must not be overwritten or removed. Success JSON is `{"ok":true,"path":"..."}`. **Pinning remains a manual Windows shell action**; OS pinning restrictions are not bypassed.

Optional user-supplied `.ico` shortcut icons are supported; the [README Start icon links](../README.md#start-icon-files) point to Chris Andriessen's upstream downloads rather than bundled assets. Creating or previewing a shortcut does not hide Start controls. Pinning restrictions are not bypassed.

Shell paths must be absolute, valid Unicode and NUL-free; individual values and the rendered command line are limited to 32766 UTF-16 units. Displayed argv strings are not safe shell-language source to interpolate into cmd.exe or PowerShell. Task Scheduler rejects XML 1.0-invalid controls and possible `%...%` expansion (conservatively, two or more percent characters in a field). Shell Link target/working-directory/icon paths of 260 UTF-16 units or longer, or containing `%`, fail explicitly rather than truncate or expand. Shortcut destinations must be explicit absolute `.lnk` paths with an existing parent; non-regular/reparse-point destinations and unrelated/modified links are refused. Ownership markers detect accidental changes, not malicious changes by another process running as the same user. A supplied icon's availability, format and rights are the user's responsibility; correct rendering is not guaranteed. The tray copies Windows' system `IDI_APPLICATION` icon; no bundled/runtime icon asset is required. See [shell APIs and limitations (v0.1.2 source)](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.2/docs/SHELL.md).

### Examples

Run these from the extracted package folder containing the two EXEs. Read-only inspection:

```powershell
.\taskbarx.exe diagnose --json
.\taskbarx.exe config show
.\taskbarx.exe startup status --json
```

Intentional user actions, **not automated tests**:

The extracted package includes `settings.sample.json` at its root. In a source checkout the sample is instead `docs/settings.sample.json`; use that path and the path to your built engine for checkout examples.

```powershell
# Save only; no taskbar or startup change.
Get-Content -Raw .\settings.sample.json | .\taskbarx.exe config import --stdin
# Apply explicitly; issue stop from a second PowerShell window.
.\taskbarx.exe run --primary-offset -20 --style blur --color '0,0,0,128'
.\taskbarx.exe stop --timeout-ms 5000
# Startup is a separate explicit action:
.\taskbarx.exe startup enable --delay-secs 6 --json
.\taskbarx.exe startup disable --json
```

### Easing selections (42)

`none`, `linear`, plus ten prefixes (`quad`, `cubic`, `quart`, `quint`, `sine`, `expo`, `circ`, `back`, `elastic`, `bounce`) combined with four suffixes (`easein`, `easeout`, `easeinout`, `easeoutin`). Example: `cubiceaseinout`. `none` positions immediately. Pure tests do not establish visual smoothness.

## Settings

Default: `%APPDATA%\TaskbarX-Rust\settings.json`. `--config PATH` selects another file; the portable folder does not implicitly become the settings directory. [Complete schema-1 defaults](settings.sample.json) are in `docs/settings.sample.json` in a checkout and `settings.sample.json` at the distribution payload root:

| Fields | Defaults / units |
| --- | --- |
| `schema_version` | `1` |
| `center`, `primary`, `secondary` | `true` |
| `center_between` | `false` |
| `primary_offset`, `secondary_offset` | `0`, signed physical pixels |
| `animation`, `animation_ms`, `poll_ms` | `"cubiceaseinout"`, `300`, `400` ms |
| `style`, `color` | `"default"`, `[0, 0, 0, 128]` RGBA |
| `primary_opacity`, `secondary_opacity` | `null` leaves unchanged; otherwise **percent 0-100**, including zero |
| `corner_radius`, `taskbar_segments` | `0` pixels, `false` |
| `default_style_on_maximized` | `false`; enabled policy affects the relevant monitor |
| `battery_animation`, `battery_poll_ms` | `null`: inherit normal values |
| `skip_resolutions` | `[]`: primary-monitor widths suspending centering only |
| `pause_on_fullscreen` | `false` |
| `hide_primary_start`, `hide_secondary_start` | `false` |
| `hide_primary_tray`, `hide_secondary_tray` | `false` |
| `revert_on_tray_overlap` | `false` |
| `toolbar_compensation` | `true` |
| `tray_icon` | `false` |
| `startup_delay_secs` | `6`; preference only, never implicit registration/update |
| `measurement` | `"msaa"`; alternate `"uia"` |
| `show_console` | `false` |

Existing schema-1 defaults remain unchanged. Missing fields default; unknown fields, types, versions and out-of-range values fail validation. Shared validation is authoritative; CLI/GUI must not silently clamp errors. RGBA alpha is a **byte (0-255)**, not upstream's percentage: alpha 128 is about half opacity, whereas byte 50 is not upstream alpha 50. Total taskbar opacity is a separate percentage.

Limits: offsets -10000..10000 physical pixels; animation duration 0..10000 ms (zero is instant); normal/battery polling 50..10000 ms; corner radius 0..10000 physical pixels; at most three skip widths of 1..65535 physical pixels; startup delay 0..86400 seconds. Easing, style and measurement names are case-sensitive. JSON must be an object of at most 64 KiB. Invalid input is rejected, not reset to defaults.

## Build, verify and package locally

This is the **native x64 local build/verification workflow**, not a general cross-architecture distribution packager. Use a source checkout, PowerShell 5.1+, Windows x64, native `x86_64-pc-windows-msvc` Rust/Cargo, **rustfmt and Clippy**, MSVC C++ tools and Windows SDK. This is a Rust 2021 Cargo workspace, not an npm or upstream .NET build. For the separate offline, prebuilt-binary packaging contract and fake-PE tests, see [distribution packaging](DISTRIBUTION.md#offline-packaging-and-fixture-checks).

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
powershell.exe -NoProfile -File .\scripts\verify.ps1
powershell.exe -NoProfile -File .\scripts\release.ps1
```

Required gates: format, workspace tests, Clippy with warnings denied and real compiler-reported binaries; release requires the workspace release build. Dependency-resolving commands use `--locked`. Unique safe target/evidence directories avoid accepting stale artifacts and preserve old output. Scripts never run/stop the engine, open the GUI, register startup, create live shortcuts or restart Explorer. Only read-only diagnostics may probe the desktop.

The default Cargo target is a unique `dist/cargo-target/<id>` directory. An explicit `CARGO_TARGET_DIR` may be outside the checkout, but must be an absolute new/empty directory, outside this checkout's `target/` and `app/` trees, with no reparse-point ancestors. Relative, occupied or reparse paths are rejected. Script-only helper fixtures/logs use a unique `%TEMP%/TaskbarX-Rust-script-tests-<id>` directory, not release evidence under `dist/`.

The local `scripts/release.ps1` workflow repeats x64 verification and packages both Rust executables, broad documentation/sample, dependency inventory, SHA-256 checksums and verification/build evidence under unique `dist/` paths. Its ZIP may include local paths and desktop diagnostics: it is an **evidence package, not a distribution archive**. `scripts/package-preview.ps1` is an older x64-oriented repackager of a checksummed local candidate, with a different name/layout. Use the [allowlisted distribution workflow](DISTRIBUTION.md#offline-packaging-and-fixture-checks) for portable ZIP/Rainmeter packages.

These scripts do not sign, publish, upload, install, push or alter an existing `app/` deployment. The local build procedure is locked and repeatable, **not a byte-identical archive guarantee**. See [validation checklist (v0.1.2 source)](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.2/docs/INTEGRATION.md). Build scripts and source are not part of the portable user workflow.

## License and attribution

[LICENSE](../LICENSE) preserves upstream's MIT license and **Copyright (c) 2021 Chris Andriessen**. See [third-party notices](../THIRD_PARTY_NOTICES.md). No upstream executable, Store asset, signing key or certificate is included or used. Upstream update/version checks describe the upstream product, not releases of this rewrite.

Upstream-like package selection does not reproduce upstream .NET internals. ModernWPF, TaskScheduler and Emoji.Wpf DLLs and their resource directories are **not needed by this native Rust rewrite**. Upstream binaries must never be relabeled as Rust builds. This Cargo workspace uses its own crates plus dependencies such as `clap`, `serde`, `serde_json` and `windows`; the resolved Cargo.lock/metadata inventory, not the upstream README, identifies the actual dependency versions. This does not imply every system dependency is statically linked. Preserve notices for derived source, formulas and defaults. The [dependency license texts](licenses/README.md) are included as `docs/licenses/` alongside `LICENSE` and `THIRD_PARTY_NOTICES.md` in distribution payloads; declared license metadata alone is not a compliance certification.

The banner is an original AI-generated conceptual illustration, not an application screenshot or upstream-derived artwork, and is not bundled in distribution packages. See [artwork provenance](assets/README.md). No upstream, Microsoft, Rainmeter, Rust project or Rust Foundation endorsement is implied. Optional Chris Andriessen / upstream Start icons are linked rather than redistributed; their use/redistribution rights must not be inferred from the source MIT license.
