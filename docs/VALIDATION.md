# Validation status

## Integrated feature validation (2026-10-29 UTC)

The combined workspace passed the native Windows x64 gate after reconciling
`Cargo.lock` and fixing obsolete-position restoration across a resolution skip.
The new failing-before/passing-after regression is retained in the local
`reference/regression-before.log` and `reference/regression-after.log` evidence.
A runtime regression also covers replacement and surviving identities while
keeping appearance active. The GUI retains deferred painting/escaped icons and
now displays the independent Rust package version.

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --locked`: **279 passed**, 0 failed; five subprocess
  fixture entry points are intentionally ignored as standalone tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Locked debug and release workspace builds: passed, both x64 EXEs validated.
- Script fixtures: **103 assertions passed**, separate from application tests.
- Read-only diagnostics: Windows 10 build 19045, one classic primary taskbar,
  safe proposed target, no warnings. No positioning/style mutation was requested.

Evidence: `dist/verification/20261029T072050982Z-c5dd96e709544bf99cadada0632859ee/`.
This run records the integrated merge revision plus the exact pre-commit dirty
file list; the release procedure reruns every gate against the committed candidate
and records its revision and portable/ZIP hashes in `dist/release-evidence/`.
The final package's `evidence/verification/verification.json` and build metadata
identify that release run without a self-referential source revision/hash.

The pre-existing running engine (PID 1472, started 2026-10-28 22:54:03 local) was
left running. The settings hash remained
`4316F31E14372028598467566F2CE55735ADEB401C3AEFBB14BDEA7F2F3B2CF0`.
Startup was not enabled; real scheduled tasks, shortcuts, Start and tray actions
were not exercised. No GUI was launched for manual visual testing.

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

## Release evidence and remaining manual scope

Required evidence:
- Format, workspace tests, mandatory Clippy, debug and release builds with locked dependencies.
- Compiler-reported x64 CLI/GUI artifacts and matching hashes in the portable folder and ZIP.
- Read-only `diagnose --json`, with unsupported layouts/skipped measurements reported honestly.
- Script-helper results kept distinct from application validation.
- Intentional compatibility and safety differences remain in [PARITY](PARITY.md).

**Live visual parity, taskbar mutations/restoration, Explorer restart, startup registration, shortcuts/pinning and tray interaction: NOT EXECUTED by the release procedure.** Prior configurator inspection or earlier build evidence does not validate this changed candidate. The GUI and release binaries are not launched automatically.

See [TESTING](TESTING.md) and [INTEGRATION](INTEGRATION.md). Do not overwrite an existing `app/` deployment, register startup, publish, push or restart Explorer as part of validation.
