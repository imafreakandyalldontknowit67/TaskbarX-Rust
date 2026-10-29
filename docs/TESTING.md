# Testing and evidence boundaries

## Current status

**Combined application build: NOT EXECUTED in the documentation worktree.** The root application and four sibling crates are intentionally unavailable there. Missing crates must be reported as requiring integration, never counted as passing tests. Script syntax/helper tests are separate from Cargo tests and do not establish application correctness.

**Live positioning, styling, graceful restoration and Explorer recovery: NOT EXECUTED.** The allowed automated desktop operation is only the read-only debug executable's `diagnose --json`. Neither a successful diagnostic nor a PE-header check proves visible centering, styles or cleanup work. The release GUI is built and packaged but is not launched by the scripts.

Documentation-branch checks on 2026-10-29 UTC (2026-10-28 local), Windows PowerShell 5.1.19041.6456: **36 script-helper assertions passed**, including syntax parsing and text-only archive/checksum tests. Both `verify.ps1` and `release.ps1` returned exit code 1 with the required incomplete-workspace/integration message. These are successful refusal-path checks, **not** successful application verification/release runs. No Cargo command or application desktop probe was executed for these checks.

## Automated post-merge gate

Run from the integrated checkout using Windows PowerShell 5.1+ and native Windows x64 MSVC Rust:

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
powershell.exe -NoProfile -File .\scripts\verify.ps1
# Optional additional lint gate:
powershell.exe -NoProfile -File .\scripts\verify.ps1 -Clippy
```

`verify.ps1` resolves its repository root from its script directory and requires all five package manifests. It records Rust/Cargo versions, OS, PowerShell, Git commit/status, exact command argument arrays, exit codes and stdout/stderr. It generates the root lockfile through `cargo generate-lockfile` only when absent, then uses locked dependency resolution. This initial generation is a source change for the integration owner to review/commit, not something this documentation branch fabricates.

The actual commands are:

1. `cargo metadata --locked --format-version 1`: confirm expected workspace package locations and binary targets.
2. `cargo fmt --all -- --check`.
3. `cargo test --workspace --locked`.
4. `cargo build --workspace --locked --message-format=json`: verify compiler-reported artifact paths for both real binaries and their Windows x64 PE headers.
5. Optional: `cargo clippy --workspace --all-targets --locked -- -D warnings`.
6. **Only desktop probe:** `target/debug/taskbarx.exe diagnose --json`. Nonzero exit, empty output, malformed JSON or JSON null fails validation. The parsed diagnostic is retained as `diagnose.json`; unsupported-layout reports must be reviewed, not hidden.

Build output is pinned to the repository's `target` directory for these scripts, restoring the caller's `CARGO_TARGET_DIR` afterward. Cross-target configuration such as `CARGO_BUILD_TARGET` or Cargo `build.target` is not supported: unexpected artifact paths fail instead of accepting a stale `target/debug` executable. Do not change source files or run concurrent builds/releases while collecting a candidate's evidence.

Evidence goes into unique `dist/verification/<UTC-and-GUID>/` directories. `verification.json` is marked passed only after every required step succeeds; failures throw and exit nonzero when invoked with `powershell.exe -File`. Logs are preserved on failure. `only_read_only_desktop_diagnostics` describes the permitted desktop scope, while `diagnostics_completed` records whether the probe actually completed. It is not proof that an unaudited test suite obeyed the boundary below.

### Non-negotiable automated safety boundary

**No automatic test may mutate the real taskbar.** Before running the combined suite, the integration owner must review tests, examples, build scripts and test subprocesses for that property. Do not invoke engine `run`, `stop`, `configure`, GUI Apply/start, scheduled-task commands, registry startup writes, or Explorer restart as validation. Even `stop` mutates/restores state and is not a harmless cleanup command for an automated test.

Use pure functions, mocks, injected adapters, temporary configuration directories and synthetic handle/rectangle fixtures. Keep any opt-in live desktop tests out of the default `cargo test --workspace` suite; none is authorized by the verification scripts. The scripts run Cargo tests, not a sandbox, so test-owner review is required.

## Automated coverage expected from the owning packages

| Owner | Required non-mutating coverage |
| --- | --- |
| `taskbarx-core` | Horizontal/vertical geometry; negative monitor origins; parent-relative transforms; full span/free span; signed offsets; clamping and insufficient space; empty/invalid rectangles; all 42 easing names, endpoint/duration behavior and finite outputs. Overshooting easing curves must not be incorrectly tested as universally monotonic. |
| `taskbarx-win32` | Fake discovery trees; primary/secondary categorization; unsupported modern shell layouts; failed/empty MSAA measurements skip safely; style/color encoding; stale/destroyed handles; rediscovery; snapshot/restore plans and cleanup calls against a fake backend only. |
| `taskbarx-config` | Exact schema-1 defaults and sample compatibility; valid JSON round trip; malformed/wrong-type/unknown-version/range rejection; RGBA byte alpha; temporary-path save/import; missing-file reads do not create files; no desktop dependency. |
| Root `taskbarx-rust` | No-argument/help safety; all documented subcommands/options; negative offsets; mutually exclusive taskbar selectors; rejection of legacy switches; show/import/diagnose side-effect boundaries; single-instance and graceful stop/timeout via mocked IPC/lifecycle, never the user's running engine. |
| `taskbarx-configurator` | Form-to-config validation, shared defaults, Save versus Apply/start separation, engine/config path handling with spaces, generated argument arrays; fake engine/process adapter. No real Apply/start in tests. |

These are acceptance expectations, not a statement of existing coverage in absent branches. Integration must fix gaps through the responsible owner.

## Script-only self-tests

`scripts/test-helpers.ps1` parses all owned PowerShell scripts, tests root resolution, incomplete-workspace errors, JSON parsing/rejection, fake Cargo metadata and artifact records, nonzero exit propagation, log preservation, argument boundaries, PE rejection, complete sample defaults, and checksums/ZIP creation using text-only fixtures. It invokes only a harmless temporary PowerShell script for fake stdout/stderr/exit codes; it creates **no application binaries** and runs neither Cargo nor desktop diagnostics. Fixtures and logs remain under unique `dist/script-tests/` directories; no recursive cleanup is performed.

A PASS here means **script helpers only**, not successful compilation, a validated combined schema or a release candidate. The sample's complete compatibility still needs the config owner's test suite after integration.

## Manual matrix — NOT EXECUTED

Every row below is **NOT EXECUTED**. These are future, separately authorized interactive tests on a disposable Windows 10 x64 desktop, not instructions for the current automated validation. Capture Windows build, monitor arrangement/DPI, settings, binary hashes, before/after observations and restoration limits if later performed. Never change these statuses based solely on mocks or diagnostics.

| Scenario | Future observation / acceptance criterion | Status |
| --- | --- | --- |
| Multi-monitor / negative coordinates | Primary-only, secondary-only and both; monitors left/above origin; signed offsets and both centering modes stay in the intended taskbar | **NOT EXECUTED** |
| Vertical edges | Left and right vertical taskbars; top/bottom horizontal edges; no axis confusion or reserved-area overlap | **NOT EXECUTED** |
| DPI changes | Mixed DPI, scaling/resolution changes, monitor attach/detach; refreshed measurements and no stale coordinate space | **NOT EXECUTED** |
| Icon add/remove | Pin/unpin, app open/close, empty or crowded list; stable recentering, correct durations and immediate `none` | **NOT EXECUTED** |
| Tray resizing | Tray icon/clock/leading-control changes; free-span recomputation without clipping or overlap | **NOT EXECUTED** |
| Default style | Default appearance and restoration limits observed on supported shell | **NOT EXECUTED** |
| Transparent style | Color/alpha behavior, theme interactions and cleanup | **NOT EXECUTED** |
| Blur style | Color/alpha behavior, unsupported-composition reporting and cleanup | **NOT EXECUTED** |
| Acrylic style | Color/alpha behavior, build-dependent fallback/failure and cleanup | **NOT EXECUTED** |
| Opaque style | Color behavior and cleanup; record alpha limitations rather than assume visual equivalence | **NOT EXECUTED** |
| Graceful close/stop | Engine close and explicit stop restore owned state; single-instance contention/timeout; third-party appearance limitations recorded | **NOT EXECUTED** |
| Explorer recovery | Separately authorized Explorer restart; old handles discarded, new taskbars rediscovered, no crash or stale movement | **NOT EXECUTED** |

Hard termination/power loss can bypass cleanup and is not equivalent to graceful stop. Do not use it as a verification workaround.
