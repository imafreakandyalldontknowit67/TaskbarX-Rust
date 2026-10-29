# Integration and local release checklist

This checklist applies to the **actual combined workspace**, not an obsolete branch count or a documentation-only build. It does not require new repositories, worktrees, commits, any orchestration CLI, publication or pushing.

## Before collecting evidence

- [ ] All six workspace packages exist: root `taskbarx-rust`, core, win32, config, configurator and **shell**. CLI and GUI binary targets are `taskbarx` and `taskbarx-configurator`.
- [ ] Native Windows x64, `x86_64-pc-windows-msvc` Rust/Cargo, rustfmt, **Clippy**, MSVC C++ tools and Windows SDK are available.
- [ ] Root `Cargo.lock` is the real reviewed integrated dependency graph. Verification/release does **not** generate or repair it. Dependency-resolving commands use `--locked`.
- [ ] Review tests/subprocesses for no desktop mutation. Use fake startup/shortcut/tray adapters, synthetic taskbars and temporary config files. No engine run/stop/configure, GUI launch/Apply, Start-menu invocation, startup changes, live shortcuts, tray actions, process killing or Explorer restart.
- [ ] Audit no-argument/help and `diagnose --json` paths before the first probe. Scripts do not sandbox unsafe code.
- [ ] Reconcile [PARITY](PARITY.md), README, sample JSON, shared validator, CLI, GUI and runtime. A persisted but unused field is not completed parity. Resolve pending labels against actual source/tests.
- [ ] Confirm Save/Apply/run/preference changes cannot register startup. Enable/Disable owns only this rewrite's task; shortcut remove/replacement rejects unrelated files. Upstream tasks remain untouched.
- [ ] Freeze source edits during final validation. Do not collect release evidence while other workers/builds change the candidate.

## Required gates

From the integrated checkout:

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
if ($LASTEXITCODE -ne 0) { throw 'Script helper tests failed' }
powershell.exe -NoProfile -File .\scripts\verify.ps1
if ($LASTEXITCODE -ne 0) { throw 'Verification failed' }
powershell.exe -NoProfile -File .\scripts\release.ps1
if ($LASTEXITCODE -ne 0) { throw 'Release failed' }
```

Verification requires `cargo fmt --all -- --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, the debug artifact build, and `cargo build --release --workspace --locked --message-format=json`. Clippy is **not optional**. The retained `-Clippy` switch is compatibility-only, not an opt-in gate.

Release repeats verification, rather than trusting older results, and packages verified compiler-reported release artifacts. Check actual logged argument arrays and hashes. Without an override, Cargo uses a unique `dist/cargo-target/<id>`. Explicit `CARGO_TARGET_DIR` may be outside the checkout, but must be an absolute new or empty directory outside the checkout's `target/` and `app/` trees. Relative/drive-relative/root-relative paths, files, occupied directories and reparse-point paths/ancestors are rejected. A create-new ownership marker prevents concurrent claims of the same empty directory. Original environment settings are restored.

On failure, retain logs, correct the owning source and rerun the full gate. Missing packages/lockfile or unsafe paths are failures, not skipped successes. Do not drop workspace members, weaken lint/tests, substitute upstream executables or manufacture placeholders.

## Output

Unique generated outputs are retained under `dist/`:

- `verification/<id>/`: command logs, `verification.json`, read-only `diagnose.json`.
- `release-evidence/<id>/`: final `release.json`, metadata, lockfile and dependency evidence.
- `TaskbarX-Rust/<id>/`: both Rust EXEs, README/LICENSE/notices, `settings.sample.json`, `docs/`, build metadata/evidence and `SHA256SUMS.txt`.
- `TaskbarX-Rust-<id>.zip` and its adjacent `.zip.sha256`.
- Isolated Cargo targets under `dist/cargo-target/<id>` by default, or the explicitly validated target directory.

Script-only helper fixtures/logs are separate: unique `%TEMP%/TaskbarX-Rust-script-tests-<UTC>-<GUID>` directories, not `dist/script-tests` or application release evidence. Historical outputs are left untouched.

Use paths returned by the scripts, not guessed `target/release` or a pre-existing `app/` deployment. The authoritative final packaging result is the external `release.json`; the packaged evidence is a pre-archive snapshot to avoid self-reference. Generated `dist/`/build outputs are not source to commit. Old packages must not be recursively removed or overwritten.

## Acceptance

- [ ] Actual verification and release JSON say `passed`; all mandatory command logs/exit codes exist. Helper-only PASS is insufficient.
- [ ] Read diagnostic findings, including unsupported layouts/skips; successful JSON does not prove centering works.
- [ ] Both EXEs were emitted by Cargo for the intended package/bin/profile and are x64 PE files; release executables were not launched.
- [ ] Both EXEs and required documents/sample occur in staging **and inside the ZIP**.
- [ ] Independently hash both EXEs and ZIP with `Get-FileHash -Algorithm SHA256 -LiteralPath <file>`; compare recorded checksums. Validate every payload checksum when transporting.
- [ ] Preserve compiler/Cargo/OS versions, source Git revision/status, root lockfile hash, dependency inventory and exact command logs. Evidence can contain local filesystem paths; review before sharing.
- [ ] Review dependency licenses and add required texts/notices under `docs/licenses/` before external distribution. Declared license expressions are inventory, not compliance certification. Repackage after notice changes.
- [ ] Reconcile final source/API/test links in PARITY. Explicitly retain any remaining integration/UI gaps.
- [ ] Report live positioning, styling, restoration, Explorer recovery, startup, shortcuts and tray actions as **NOT EXECUTED** unless separately authorized tests have their own evidence.
- [ ] Do not publish, sign, upload, push, enable startup, change `app/` or restart Explorer as part of local packaging.

This procedure is repeatable with locked resolution, not bit-for-bit reproducibility: timestamps, toolchain, linker/SDK and environment can affect output.
