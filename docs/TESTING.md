# Testing and evidence boundaries

## Status

Current checks are summarized in [BUILD-v0.1.2](BUILD-v0.1.2.md). Historical workspace and isolated results are summarized in [VALIDATION](VALIDATION.md); architecture-specific v0.1.1 results are in [BUILD-v0.1.1](BUILD-v0.1.1.md). Validation of a new candidate requires its own `verification.json`/`release.json` and logs. Earlier results do not validate later source.

**Live positioning, styling, restoration, Explorer recovery, startup registration, shortcut creation/pinning, Start-menu and tray interaction: NOT EXECUTED by the automated procedure.** Its sole desktop probe is `diagnose --json`. PE checks, successful compilation, mocks and diagnostic JSON do not establish visual feature parity.

Historical native checks ran on Windows 10 x64 with the classic taskbar; x86 checks ran under WOW64, not native 32-bit Windows. Windows 11 is unsupported. ARM64 is unavailable and unvalidated. A separate [startup repair](#startup-verification-hotfix-2026-10-29) verified Enable/update and status, but not launch at sign-in. Most visual effects and live Rainmeter interaction remain untested.

## Required automated gate

Run source scripts with PowerShell 5.1+ on native Windows x64 MSVC Rust with rustfmt and Clippy:

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

### Automated safety boundary

**No default test or release script may mutate the real taskbar or shell integration.** No engine `run`, `stop`, `configure`, GUI Apply/launch, startup enable/disable, Start-menu action, live shortcut create/remove, tray action or Explorer restart. Even stop mutates/restores state and is not a harmless automated cleanup step.

Use pure policies, injected adapters, fake process/IPC/startup/shortcut/tray backends, temporary config directories and synthetic handles/rectangles. Scripts run tests, not a sandbox: review examples/build scripts/test subprocesses too. Do not enable opt-in live tests inside the default workspace suite.

## Required coverage and source destinations

See [PARITY](PARITY.md) for settings/actions and exact source/test links. These are coverage requirements, not current passing-test results.

| Component | Required non-mutating coverage |
| --- | --- |
| Core | Signed/negative origins; horizontal/vertical geometry; full/free span; clamping/insufficient space; 42 easing names, endpoints, duration and finite values. Overshoot curves are not universally monotonic. |
| Config | Unchanged schema-1 defaults; complete sample; all new fields and missing-field defaults; malformed/unknown/range rejection; RGBA byte alpha vs optional 0-100 opacity; nullable battery inheritance; atomic temporary-path persistence. |
| Win32 | Fake primary/secondary trees and stale identities; fail-closed MSAA and real UIA adapter paths; distinct transparent styles; independent opacity/region/visibility capture and restore; no mutation for uncertain bounds; destroyed Explorer identities never restored. |
| Runtime/CLI | Safe help/show/diagnose; command/JSON contracts; no implicit startup; fake single-instance/cleanup acknowledgment; battery/maximized/fullscreen/width/overlap/toolbar policy; fullscreen avoids accessibility polling/mutation while stop remains responsive. |
| Shell | Pure argument-safe LaunchSpec; startup status/enable/disable with fake adapters, ownership and OS failures; no upstream task migration; owned .lnk validation/refusal; manual pinning boundary; tray actions queued, no reentrant mutation or phantom cleanup. |
| Configurator | Shared form defaults/validation and helpers; initialize/edit/reset/reload/save never run or register; Apply save -> stop acknowledgment -> run and abort on failure; explicit shell actions only; arguments with spaces; exact direct StartupStatus/shortcut JSON; fake process errors/timeouts visible. |
| Scripts | Required fmt/test/Clippy/release gates; six-package metadata; real artifact/profile checks; unique safe targets and override/path rejection; logs on failure; required packaged files, ZIP and SHA-256 evidence. |

Coverage must connect persisted settings to runtime consumers, select the actual requested measurement provider and propagate OS failures. Test these contracts with injected adapters rather than live desktop mutation.

## Script-only self-tests

`test-helpers.ps1` checks script parsing/helpers, fake metadata/build records, command failure propagation, JSON/artifact/path validation, all 31 sample fields and text-only ZIP/checksum fixtures. Such fixtures are not application binaries; helper PASS is separate from the Cargo/application gate. Fixtures/logs use unique `%TEMP%/TaskbarX-Rust-script-tests-<UTC>-<GUID>` directories, not application release evidence. Existing output remains untouched.

`test-distribution.ps1` separately uses inert PE fixtures to check architecture matching, archive layouts, Rainmeter metadata/footer, path rejection and output preservation. It never launches binaries, command files or skins. See [distribution fixture checks](DISTRIBUTION.md#offline-packaging-and-fixture-checks).

## Manual coverage matrix

This matrix distinguishes pending live checks from the narrow historical startup result. It is not v0.1.2 validation. Run future manual checks only on a disposable supported Windows 10 desktop with explicit authorization. Record OS build, monitor/DPI setup, settings, binary hashes, before/after observations and limitations; mocks and diagnostics cannot substitute for them.

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
| Startup | Owned enable/status/disable, delay/config path, failure handling and actual sign-in launch; Save/Apply untouched | Historical Enable/update and status only; disable/failure matrix and logon launch untested |
| Start shortcut | Owned .lnk/icon create/remove, Start action, refusal of unrelated files; user manually pins | NOT EXECUTED |
| Tray | Icon ownership/recreation, queued configure/restart/stop, cleanup without phantom-icon scanning | NOT EXECUTED |
| GUI | Complete controls, keyboard navigation, preview/hex/picker, feedback and explicit-action separation | NOT EXECUTED |
| Rainmeter | Installation, manual skin loading and explicit-click settings launch; no automatic engine/startup action | NOT EXECUTED |

Hard termination/power loss can bypass cleanup and are not equivalent to graceful stop. Do not force-kill as a verification workaround.

## Startup verification hotfix (2026-10-29)

Commit `ac69090` resolves account names returned by Task Scheduler to SIDs before
ownership comparison. Windows returned a bare username for the principal and a
DOMAIN\username trigger even though registration XML specified SIDs. The old
comparison incorrectly reported a successful registration as foreign.

After the fix: 30 shell tests, strict shell Clippy, and the locked release workspace
build passed. A live check successfully performed Enable/update and then read back
the task: registered, enabled, owned, delay 6 seconds, no warnings. This was a
separate startup repair, not part of the read-only release procedure or v0.1.2
verification. Actual launch at sign-in is still not tested.
