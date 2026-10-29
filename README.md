# TaskbarX-Rust

An **independent Rust rewrite** of [ChrisAnd1998/TaskbarX](https://github.com/ChrisAnd1998/TaskbarX), originally by Chris Andriessen. This is **not an official upstream release**, nor a binary-compatible replacement for its command line.

## Status and compatibility

The target is **Windows 10 x64 with the classic Explorer taskbar**. The development host is Windows 10 build **19045**. This document defines the combined implementation contract; the documentation branch alone contains no Rust application. A feature listed below is an implementation requirement, not evidence that it has passed live desktop testing. See [integration gates](docs/INTEGRATION.md) and [test status](docs/TESTING.md).

**Native modern Windows 11 taskbars are unsupported.** Elevation or a different animation setting does not make them compatible. Classic-shell modifications on Windows 11 are not a promised supported configuration. Do not run as administrator as a compatibility workaround.

## Architecture

| Package / location | Responsibility / output |
| --- | --- |
| `taskbarx-rust` / root | CLI, engine lifecycle and single-instance coordination; `taskbarx.exe` |
| `taskbarx-core` / `crates/taskbarx-core` | Pure geometry and easing; no real desktop mutation |
| `taskbarx-win32` / `crates/taskbarx-win32` | Shell discovery, MSAA measurements, movement, styles and restoration |
| `taskbarx-config` / `crates/taskbarx-config` | Validated, versioned JSON settings |
| `taskbarx-configurator` / `crates/taskbarx-configurator` | GUI; `taskbarx-configurator.exe` |

There was no pre-existing Rust or `package.json` project to preserve. The upstream reference is a VB.NET/.NET Framework application, not a build input or a source of executable files for this rewrite.

## Feature parity boundary

| Feature | Requested implemented scope after integration | Validation / exclusions |
| --- | --- | --- |
| Primary and secondary taskbars | Center either or both; horizontal and vertical edges | Requires accessible, recognized Windows 10 shell layout |
| Centering modes | Full-taskbar midpoint by default; free span between leading controls and tray with `--center-between` | Geometry must use signed screen/parent coordinates and avoid reserved areas |
| Offsets | Independent signed primary/secondary offsets along the taskbar axis | Positive moves right/down; negative moves left/up, subject to available space |
| Animation | 42 selections including `none`, plus configurable duration | Pure easing tests do not establish visual smoothness |
| Appearance | `default`, `transparent`, `blur`, `acrylic`, `opaque`, with RGBA color | Undocumented composition behavior varies; not an exact appearance guarantee |
| Configuration | JSON persistence, CLI overrides and GUI editing | Saving is not applying; Apply/start is explicit |
| Lifecycle | Single-instance operation; graceful stop with restoration | Never substitute force-killing another process for graceful stop |
| Explorer recovery | Rediscover taskbars after Explorer restarts | Stale handles must not be reused; live recovery not tested here |
| Startup and tray extras | **Not promised** | No startup scheduling or tray-icon feature parity |
| Shell customization extras | **Not promised** | No Start/tray hiding, taskbar segments/rounding or custom Start buttons |
| Policy and distribution extras | **Not promised** | No battery-specific policies, fullscreen/maximized exceptions or Store integration |
| Legacy CLI | **Not promised** | Historical upstream switches are not accepted aliases |

### Safety and limitations

- MSAA/accessibility measurement failures must cause **safe skipping**, not guessed icon bounds or taskbar movement. Unsupported shell layouts are reported.
- Composition APIs used for these styles are undocumented and can vary by Windows build, theme and other software.
- Restoration is best effort. Appearance set by another taskbar utility cannot always be captured or exactly restored. Avoid running competing appearance tools.
- Graceful close/stop attempts cleanup. Hard process termination, a crash or power loss can bypass cleanup. No promise of perfect restoration is made.
- Automated verification does **not** exercise real taskbar positioning or styling. Its only live desktop probe is `diagnose --json`, which is read-only.

## Command contract

No arguments, `help`, and `--help` are safe: they must not apply settings or launch a taskbar-changing loop. Only an explicit `run` or GUI Apply/start applies settings.

```text
taskbarx.exe
taskbarx.exe --help
taskbarx.exe run [--config PATH] [OPTIONS]
taskbarx.exe stop [--timeout-ms N]
taskbarx.exe diagnose [--json] [--config PATH]
taskbarx.exe config show [--config PATH]
taskbarx.exe config import --stdin [--config PATH]
taskbarx.exe configure [--config PATH]
taskbarx-configurator.exe [--engine PATH] [--config PATH]
```

| `run` option | Meaning |
| --- | --- |
| `--primary-offset N`, `--secondary-offset N` | Signed offsets |
| `--animation NAME` | Easing selection (below) |
| `--animation-ms N` | Animation duration in milliseconds |
| `--poll-ms N` | Discovery/measurement polling interval in milliseconds |
| `--style NAME` | `default`, `transparent`, `blur`, `acrylic`, `opaque` |
| `--color R,G,B,A` | Four integer bytes, each 0–255 |
| `--no-center` | Disable centering; appearance remains separately configurable |
| `--only-primary` / `--only-secondary` | Select one taskbar category; mutually exclusive |
| `--center-between` | Center in the available free span instead of the full taskbar |

`run` loads the selected configuration and applies explicit overrides. Use JSON import or GUI Save for persistence; do not assume run overrides are saved. `stop` requests graceful restoration and shutdown of the existing engine, waiting up to the supplied timeout. `diagnose` is read-only, including when given a config path. `config show` prints settings and does **not** create a missing file. `config import --stdin` validates and saves but does **not** start or apply anything. `configure` launches the sibling GUI; keep both executables together. The GUI's `--engine PATH` selects an engine explicitly.

Safe inspection examples (PowerShell):

```powershell
.\taskbarx.exe diagnose --json
.\taskbarx.exe config show
```

Intentional user actions, **not automated tests**:

```powershell
# Save only. This does not apply settings.
Get-Content -Raw .\settings.sample.json | .\taskbarx.exe config import --stdin
# Apply explicitly. While it is running, use a SECOND PowerShell window to stop.
.\taskbarx.exe run --primary-offset -20 --animation cubiceaseinout --style blur --color '0,0,0,128'
# In the second window, from the executable's directory:
.\taskbarx.exe stop --timeout-ms 5000
```

The rewrite does not accept upstream forms such as `-tbs=1`, `-ptbo=0`, `-as=...` or `-stop`. Translate settings into the new contract instead.

### Easing selections (42)

`none`, `linear`, and each combination of these ten prefixes with the four suffixes:

- Prefix: `quad`, `cubic`, `quart`, `quint`, `sine`, `expo`, `circ`, `back`, `elastic`, `bounce`.
- Suffix: `easein`, `easeout`, `easeinout`, `easeoutin`.

For example: `cubiceaseinout`, `backeaseout`, `bounceeaseoutin`. `none` means immediate positioning rather than animation.

## Settings

Default path: `%APPDATA%\TaskbarX-Rust\settings.json`. `--config PATH` selects a different file. Defaults and [the packaged sample](docs/settings.sample.json) use schema version 1:

```json
{
  "schema_version": 1,
  "center": true,
  "primary": true,
  "secondary": true,
  "center_between": false,
  "primary_offset": 0,
  "secondary_offset": 0,
  "animation": "cubiceaseinout",
  "animation_ms": 300,
  "poll_ms": 400,
  "style": "default",
  "color": [0, 0, 0, 128]
}
```

RGBA alpha is a **byte (0–255)**, not upstream's percentage (0–100). For instance, alpha 128 is approximately half opacity; upstream's value 50 is not equivalent to byte 50. Invalid values/schema versions must be rejected rather than silently applied. Numeric validation boundaries are owned by `taskbarx-config` and must agree with CLI/UI tests.

The portable folder does not automatically become the settings directory. Use `--config` for an explicit local file. GUI **Save** only persists; **Apply/start** is a separate, explicit action. Startup registration is not included.

## Build from source, verify and create a local package

Run these scripts in a source checkout, not the portable binary folder. After all branches are merged, use Windows PowerShell 5.1 or newer on Windows x64 with the native `x86_64-pc-windows-msvc` Rust toolchain, Cargo, rustfmt, and the MSVC C++ build tools/Windows SDK. Clippy is optional. No Node/npm build is involved.

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
powershell.exe -NoProfile -File .\scripts\verify.ps1
# Optional lint gate:
powershell.exe -NoProfile -File .\scripts\verify.ps1 -Clippy
powershell.exe -NoProfile -File .\scripts\release.ps1
```

Scripts resolve the repository from their own location, not your current directory. Missing packages are a hard **post-integration required** error, not a passing test. Verification checks format, tests and builds the entire workspace and captures JSON diagnostics. An absent root `Cargo.lock` is generated by Cargo only once a complete workspace exists; subsequent resolving commands use `--locked`. The integration owner must review and commit that generated lockfile.

Release repeats verification and builds the actual combined workspace in release mode. Each invocation preserves previous output and creates a uniquely named portable folder under `dist/TaskbarX-Rust/`, a local ZIP, SHA-256 checksums and evidence. Binaries are real Cargo artifacts, never copied/renamed upstream executables. The release process does not launch the GUI or release binaries, sign, publish, push, upload, install, schedule startup, or restart Explorer. `dist/` contains generated outputs, not source to commit.

This is a repeatable build procedure, **not a claim of bit-for-bit reproducible archives**: timestamps, toolchains, linker/SDK and environment can affect output. Metadata records tool versions, lockfile hash and read-only diagnostic scope. Read [TESTING](docs/TESTING.md) and [INTEGRATION](docs/INTEGRATION.md) before interpreting a successful build as release readiness.

## License and attribution

The complete upstream MIT license, including **Copyright (c) 2021 Chris Andriessen**, is preserved verbatim in [LICENSE](LICENSE). See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for provenance, asset exclusions and dependency-review requirements. The source license must not be assumed to license every binary or image found in upstream distribution folders. No upstream signing key or certificate is included or used.
