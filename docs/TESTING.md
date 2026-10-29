# Testing and evidence boundaries

## Status

This documentation update inspected source and mapped requirements; it did **not** run the final combined gates. Current validation must come from the final invocation's retained `verification.json`/`release.json` and logs, summarized in [VALIDATION](VALIDATION.md). Historical helper-only or earlier build results do not validate later integrated source.

**Live positioning, styling, restoration, Explorer recovery, startup registration, shortcut creation/pinning, Start-menu and tray interaction: NOT EXECUTED by the automated procedure.** Its sole desktop probe is `diagnose --json`. PE checks, successful compilation, mocks and diagnostic JSON do not establish visual feature parity.

## Required automated gate

Run source scripts on native Windows x64 MSVC Rust with rustfmt and Clippy:

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
powershell.exe -NoProfile -File .\scripts\verify.ps1
powershell.exe -NoProfile -File .\scripts\release.ps1
```

Verification requires all six packages and a real root Cargo.lock; it never repairs a missing lockfile. Required operations include:

1. `cargo metadata --locked --format-version 1`: check workspace paths, packages and bins.
2. `cargo fmt --all -- --check`.
3. `cargo test --workspace --locked`.
4. `cargo clippy --workspace --all-targets --locked -- -D warnings`.
5. `cargo build --workspace --locked --message-format=json`.
6. `cargo build --release --workspace --locked --message-format=json`.
7. **Only desktop probe:** Cargo-reported debug `taskbarx.exe diagnose --json`.

The exact execution order/arguments are recorded in logs. Clippy is always required; `-Clippy` remains accepted for old callers only. Real debug/release artifacts are parsed from Cargo JSON and checked as x64 PE files. Release revalidates and packages verified release artifacts; it does not launch them.

The default target is a unique `dist/cargo-target/<id>`. Explicit `CARGO_TARGET_DIR` may be outside the checkout, but must be an absolute new/empty directory outside this checkout's `target/` and `app/` trees; relative, occupied and reparse-point paths/ancestors are rejected. A create-new marker prevents duplicate claims, and the caller's environment is restored. Application evidence uses unique `dist/` paths and survives failures. Nonzero commands, missing artifacts, malformed/null/empty diagnostics, absent lockfiles and unsupported target configurations fail instead of silently passing. Review diagnostic warnings separately from command success.

### Non-negotiable safety boundary

**No default test or release script may mutate the real taskbar or shell integration.** No engine `run`, `stop`, `configure`, GUI Apply/launch, startup enable/disable, Start-menu action, live shortcut create/remove, tray action or Explorer restart. Even stop mutates/restores state and is not a harmless automated cleanup step.

Use pure policies, injected adapters, fake process/IPC/startup/shortcut/tray backends, temporary config directories and synthetic handles/rectangles. Scripts run tests, not a sandbox: review examples/build scripts/test subprocesses too. Do not enable opt-in live tests inside the default workspace suite.

## Required coverage and source destinations

See [PARITY](PARITY.md) for settings/actions and exact source/test links. These are acceptance requirements, not a fabricated passing-test count.

| Owner | Required non-mutating coverage |
| --- | --- |
| Core | Signed/negative origins; horizontal/vertical geometry; full/free span; clamping/insufficient space; 42 easing names, endpoints, duration and finite values. Overshoot curves are not universally monotonic. |
| Config | Unchanged schema-1 defaults; complete sample; all new fields and missing-field defaults; malformed/unknown/range rejection; RGBA byte alpha vs optional 0-100 opacity; nullable battery inheritance; atomic temporary-path persistence. |
| Win32 | Fake primary/secondary trees and stale identities; fail-closed MSAA and real UIA adapter paths; distinct transparent styles; independent opacity/region/visibility capture and restore; no mutation for uncertain bounds; destroyed Explorer identities never restored. |
| Runtime/CLI | Safe help/show/diagnose; command/JSON contracts; no implicit startup; fake single-instance/cleanup acknowledgment; battery/maximized/fullscreen/width/overlap/toolbar policy; fullscreen avoids accessibility polling/mutation while stop remains responsive. |
| Shell | Pure argument-safe LaunchSpec; startup status/enable/disable with fake adapters, ownership and OS failures; no upstream task migration; owned .lnk validation/refusal; manual pinning boundary; tray actions queued, no reentrant mutation or phantom cleanup. |
| Configurator | Shared form defaults/validation and helpers; initialize/edit/reset/reload/save never run or register; Apply save -> stop acknowledgment -> run and abort on failure; explicit shell actions only; arguments with spaces; exact direct StartupStatus/shortcut JSON; fake process errors/timeouts visible. |
| Scripts | Required fmt/test/Clippy/release gates; six-package metadata; real artifact/profile checks; unique safe targets and override/path rejection; logs on failure; required packaged files, ZIP and SHA-256 evidence. |

A persisted setting with no runtime consumer is a gap. Merely selecting `uia` while continuing to use MSAA is a gap. Merely displaying success after an OS failure is a gap. Close these through source and fake tests, not unauthorized live validation.

## Script-only self-tests

`test-helpers.ps1` checks script parsing/helpers, fake metadata/build records, command failure propagation, JSON/artifact/path validation, all 31 sample fields (including 19 new defaults/types) and text-only ZIP/checksum fixtures. Such fixtures are not application binaries and must never be counted as a release build. Helper PASS is separate from the Cargo/application gate. Fixtures/logs use unique `%TEMP%/TaskbarX-Rust-script-tests-<UTC>-<GUID>` directories, not checkout `dist/script-tests`; old evidence remains untouched. No recursive cleanup of unrelated output is permitted.

## Manual matrix -- NOT EXECUTED

Every row below remains **NOT EXECUTED**. These are future, separately authorized tests on a disposable supported Windows 10 desktop. Record OS build, monitor/DPI setup, settings, binary hashes, before/after observations and limitations. Never infer these results from mocks or diagnostics.

| Scenario | Future acceptance observation | Status |
| --- | --- | --- |
| Multi-monitor/negative coordinates | Primary-only, secondary-only and both; offsets/full/free-span centering on monitors left/above origin | NOT EXECUTED |
| Vertical edges | Left/right/top/bottom taskbars without axis confusion/reserved-area overlap | NOT EXECUTED |
| DPI/display changes | Mixed DPI, attach/detach/resolution changes; fresh measurements and safe rediscovery | NOT EXECUTED |
| Icon/toolbar/tray resizing | Open/close/pin changes, empty/crowded lists, compensation and overlap policy | NOT EXECUTED |
| Six styles | Default, transparent, transparent-gradient, blur, acrylic, opaque; theme/build effects and cleanup | NOT EXECUTED |
| Total opacity/regions/visibility | Independent primary/secondary opacity, rounding/segments, Start/tray hiding and restoration | NOT EXECUTED |
| Battery/maximized/width/fullscreen policies | Correct monitor/power policy; width exceptions affect centering only; pause and resume; responsive shutdown | NOT EXECUTED |
| Measurement providers | MSAA and actual UIA on supported shell; no movement from uncertain results | NOT EXECUTED |
| Graceful stop/restart | Cleanup acknowledgment, timeout/failure visibility, no duplicate runner; third-party accent limits recorded | NOT EXECUTED |
| Explorer recovery | Separately authorized restart; destroyed identities discarded, no stale HWND mutation | NOT EXECUTED |
| Startup | Explicit owned current-user enable/status/disable, delay/config path and failure handling; Save/Apply untouched | NOT EXECUTED |
| Start shortcut | Owned .lnk/icon create/remove, Start action, refusal of unrelated files; user manually pins | NOT EXECUTED |
| Tray | Icon ownership/recreation, queued configure/restart/stop, cleanup without phantom-icon scanning | NOT EXECUTED |
| GUI | Complete controls, keyboard navigation, preview/hex/picker, feedback and explicit-action separation | NOT EXECUTED |

Hard termination/power loss can bypass cleanup and are not equivalent to graceful stop. Do not force-kill as a verification workaround.
