# Validation status

The final integrated feature/release gate has **not been claimed by this documentation update**. The integration owner must summarize the actual final source revision, command results and unique evidence paths here after all code and documentation changes are reconciled.

## Isolated shell/script checks (2026-10-29 UTC)

On branch `feature/parity-startup-shell-release-20261028-f2c91a`, only the new
`taskbarx-shell` crate was copied into an out-of-tree temporary harness. The root
workspace manifest/lockfile and application binaries were not modified. These
checks are **not integrated workspace or release-package validation**.

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
The harness has its own generated lockfile. Non-Windows compilation was not
executed (only the Windows target is installed); non-Windows public entry points
are cfg-gated unsupported implementations, with a dedicated non-Windows test.

Retained local-only evidence:

- `%TEMP%/taskbarx-shell-check-824cefea567444f29da58c23b3a3847b/evidence-8aa2fcffa256481d99febb7f67ac9aa3/`
  contains exact shell-only command argument arrays, exit codes, stdout/stderr
  and `summary.json`.
- `%TEMP%/TaskbarX-Rust-script-tests-20261029T070458706Z-2e367091c8ee47a5aee5da827bb81f5f/helper-tests.json`
  records the final helper-fixture run. No Cargo/application/desktop execution is
  performed by those fixtures.

No actual scheduler registration/query, real shell link, notification icon,
Start-menu action, engine command, `diagnose` probe, Explorer restart or application
package was executed by this worktree's validation. Root must add the workspace
member/dependencies, reconcile the sample and inventory with integrated code, run
the complete locked workspace gates, and perform final local packaging separately.

## Integrated evidence still required

Required evidence:
- Format, workspace tests, mandatory Clippy, debug and release builds with locked dependencies.
- Compiler-reported x64 CLI/GUI artifacts and matching hashes in the portable folder and ZIP.
- Read-only `diagnose --json`, with unsupported layouts/skipped measurements reported honestly.
- Script-helper results kept distinct from application validation.
- Remaining integration/UI gaps in [PARITY](PARITY.md), not hidden by changing the inventory.

**Live visual parity, taskbar mutations/restoration, Explorer restart, startup registration, shortcuts/pinning and tray interaction: NOT EXECUTED by the release procedure.** Prior configurator inspection or earlier build evidence does not validate this changed candidate. The GUI and release binaries are not launched automatically.

See [TESTING](TESTING.md) and [INTEGRATION](INTEGRATION.md). Do not overwrite an existing `app/` deployment, register startup, publish, push or restart Explorer as part of validation.
