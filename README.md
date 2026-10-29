# TaskbarX-Rust

An **independent Rust rewrite** of [ChrisAnd1998/TaskbarX](https://github.com/ChrisAnd1998/TaskbarX), originally by Chris Andriessen. Not an official upstream release or a binary-compatible replacement for its CLI.

## Compatibility and evidence

Target: **native Windows 10 x64 with the classic Explorer taskbar**. Native modern Windows 11 taskbars are unsupported; classic-shell modifications on Windows 11 are not a promised configuration. Elevation is not a compatibility workaround. Microsoft Store packaging/protocol activation is excluded.

The [source-linked parity inventory](docs/PARITY.md) distinguishes source mappings, pending integration, dormant upstream code and deliberate differences. It is not a visual-parity claim. **Live positioning, styles, restoration, startup, shortcuts and tray interaction are not verified by automated release checks.** Their only desktop probe is read-only `diagnose --json`. See [testing](docs/TESTING.md), [validation status](docs/VALIDATION.md) and [release gates](docs/INTEGRATION.md).

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

## Commands

No arguments, `help` and `--help` are safe. Legacy forms such as `-tbs=1`, `-ptbo=0`, `-as=...` and `-stop` are not aliases.

```text
taskbarx.exe
taskbarx.exe --help
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

| `run` option | Meaning |
| --- | --- |
| `--primary-offset N`, `--secondary-offset N` | Signed offsets |
| `--animation NAME`, `--animation-ms N` | Easing/duration |
| `--poll-ms N` | Normal polling interval |
| `--style NAME`, `--color R,G,B,A` | Appearance; four bytes, each 0-255 |
| `--no-center` | Disable centering independently of appearance |
| `--only-primary` / `--only-secondary` | Mutually exclusive taskbar selection |
| `--center-between` | Center between leading controls and tray |
| `--background` | Explicit background launch; never enables startup |

`run` overrides are not persisted. Other settings use the GUI or JSON import. `stop` requests graceful cleanup with a timeout. `config show` does not create a missing file; `config import --stdin` saves without applying. `configure` opens the sibling GUI; keep both executables together or supply the GUI's `--engine PATH`.

Startup status is read-only. Startup JSON is the direct `StartupStatus` object. Enable/Disable reports OS errors, not false success. Enabling registers a current-user launch with the selected config/delay; it does not immediately run it. Keep executable/config paths stable, or explicitly update/disable registration before moving the folder.

The task name and ownership marker include the current user's SID; its sole logon trigger uses an interactive, least-privilege principal, permits battery operation and has a validated delay. Inaccessible status is an error, not "absent"; a disabled logon trigger reports effective `enabled:false`. Updates validate the complete prepared XML first. Failed-update rollback only replaces that exact prepared definition and re-verifies the prior definition; interference or inaccessible state produces an honest failure, not an overwrite or a false restoration claim.

`start-menu` explicitly requests Start, not centering. Success confirms bounded `SC_TASKLIST` message delivery, not a visual-open assertion. Shortcut creation writes an owned `.lnk` to that action, optionally with a user-supplied icon. Unrelated existing files/shortcuts must not be overwritten or removed. Success JSON is `{"ok":true,"path":"..."}`. **Pinning remains a manual Windows shell action**; OS pinning restrictions are not bypassed.

Shell paths must be absolute, valid Unicode and NUL-free. Task Scheduler rejects XML 1.0-invalid controls and `%...%` expansion; Shell Link target/working-directory/icon paths of 260 UTF-16 units or longer, or containing `%`, fail explicitly rather than truncate or expand. Shortcut parent directories must already exist. A supplied icon's availability, format and rights are the user's responsibility. The tray copies Windows' system `IDI_APPLICATION` icon; no bundled/runtime icon asset is required. See [shell APIs and limitations](docs/SHELL.md).

Safe inspection:

```powershell
.\taskbarx.exe diagnose --json
.\taskbarx.exe config show
.\taskbarx.exe startup status --json
```

Intentional user actions, **not automated tests**:

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

Default: `%APPDATA%\TaskbarX-Rust\settings.json`. `--config PATH` selects another file; the portable folder does not implicitly become the settings directory. [Complete schema-1 defaults](docs/settings.sample.json):

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

## Build, verify and package locally

Use a source checkout, PowerShell 5.1+, Windows x64, native `x86_64-pc-windows-msvc` Rust/Cargo, **rustfmt and Clippy**, MSVC C++ tools and Windows SDK. No npm or upstream .NET build.

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
powershell.exe -NoProfile -File .\scripts\verify.ps1
powershell.exe -NoProfile -File .\scripts\release.ps1
```

Required gates: format, workspace tests, Clippy with warnings denied and real compiler-reported binaries; release requires the workspace release build. Dependency-resolving commands use `--locked`. Unique safe target/evidence directories avoid accepting stale artifacts and preserve old output. Scripts never run/stop the engine, open the GUI, register startup, create live shortcuts or restart Explorer. Only read-only diagnostics may probe the desktop.

The default Cargo target is a unique `dist/cargo-target/<id>` directory. An explicit `CARGO_TARGET_DIR` may be outside the checkout, but must be an absolute new/empty directory, outside this checkout's `target/` and `app/` trees, with no reparse-point ancestors. Relative, occupied or reparse paths are rejected. Script-only helper fixtures/logs use a unique `%TEMP%/TaskbarX-Rust-script-tests-<id>` directory, not release evidence under `dist/`.

Release repeats verification and emits both Rust executables, documents/sample, dependency inventory, SHA-256 checksums and a local ZIP under unique `dist/` paths. It does not sign, publish, upload, install, push or alter an existing `app/` deployment. This is a repeatable locked procedure, **not a byte-identical archive guarantee**. See [release acceptance](docs/INTEGRATION.md).

## License and attribution

[LICENSE](LICENSE) preserves upstream's MIT license and **Copyright (c) 2021 Chris Andriessen**. See [third-party notices](THIRD_PARTY_NOTICES.md). No upstream executable, Store asset, signing key or certificate is included or used. Upstream update/version checks describe the upstream product, not releases of this rewrite.
