# Validation status

Current checks are summarized in [BUILD-v0.1.2](BUILD-v0.1.2.md). The records below describe earlier validation and are retained for regression context.

## Historical workspace validation (2026-10-29 UTC)

The six-package workspace passed the native Windows x64 gate after reconciling
`Cargo.lock` and fixing obsolete-position restoration across a resolution skip.
The regression failed before the fix and passed afterward. Runtime coverage also
checks replacement and surviving identities while keeping appearance active.
The GUI retained deferred painting/escaped icons and displayed the independent
Rust package version.

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --locked`: **279 passed**, 0 failed; five subprocess
  fixture entry points are intentionally ignored as standalone tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Locked debug and release workspace builds: passed, both x64 EXEs validated.
- Script fixtures: **103 assertions passed**, separate from application tests.
- Read-only diagnostics: Windows 10 build 19045, one classic primary taskbar,
  safe proposed target, no warnings. No positioning/style mutation was requested.

The procedure did not start or stop the engine, change settings or exercise
scheduled tasks, shortcuts, Start or tray actions. No GUI was launched for manual
visual testing. Verification logs identify the tested source and artifacts;
the local release procedure reruns the checks and records executable/archive
hashes separately from the archive to avoid self-reference.

## Isolated shell/script checks (2026-10-29 UTC)

The `taskbarx-shell` crate was checked in an isolated temporary harness, without
modifying the workspace manifest/lockfile or application binaries. These checks
are **not workspace or release-package validation**.

| Check actually executed | Result |
| --- | --- |
| Harness `cargo fmt --all -- --check` | Passed |
| Harness `cargo test --locked` | 29 passed, 0 failed, 0 ignored; fakes/pure functions/temp fixtures only |
| Harness `cargo clippy --all-targets --locked -- -D warnings` | Passed |
| Harness `cargo build --release --lib --locked` | Passed; library only, no CLI/GUI packaging |
| `powershell.exe -NoProfile -File scripts/test-helpers.ps1` | 103 helper assertions passed |
| `git diff --check` | Passed |

Toolchain: `rustc 1.99.1`, `cargo 1.99.1`, native
`x86_64-pc-windows-msvc`; `windows 0.61.3`, `serde 1.0.229` in the harness.
The harness used its own generated lockfile. Non-Windows compilation was not
executed; public entry points there are cfg-gated unsupported implementations,
with a dedicated non-Windows test.

The isolated checks did not query/register real scheduled tasks, create real
shell links or tray icons, invoke Start, run engine commands/diagnostics, restart
Explorer or produce an application package. Script fixtures performed no Cargo,
application or desktop execution.

## Historical startup verification (2026-10-29)

The account-name-to-SID fix (`ac69090`) was followed by 30 passing shell tests,
strict shell Clippy and a passing locked release workspace build. A separate live
check verified Enable/update and status: registered, enabled and owned, six-second
delay, no warnings. **Actual launch at sign-in was not tested.** See the
[startup regression details](TESTING.md#startup-verification-hotfix-2026-10-29).

## New-candidate evidence and remaining scope

Required evidence:
- Format, workspace tests, mandatory Clippy, debug and release builds with locked dependencies.
- Compiler-reported x64 CLI/GUI artifacts and matching hashes in the portable folder and ZIP.
- Read-only `diagnose --json`, with unsupported layouts/skipped measurements reported honestly.
- Script-helper results kept distinct from application validation.
- Intentional compatibility and safety differences remain in [PARITY](PARITY.md).

**Live visual parity, taskbar mutations/restoration, Explorer restart, startup registration, shortcuts/pinning and tray interaction are not exercised by the release procedure.** The GUI and release binaries are not launched automatically. The separate historical startup result above does not change this boundary.

Support is limited to the **Windows 10 classic taskbar; Windows 11 is unsupported**.
Historical x86 checks ran under WOW64 on Windows 10 x64; native 32-bit Windows,
most visual effects and live Rainmeter interaction remain untested. ARM64 is
unavailable and unvalidated.

See [TESTING](TESTING.md) and [INTEGRATION](INTEGRATION.md) for the locked checks,
safe output directories and separation of local evidence from distribution packages.
