# Post-merge integration and local release checklist

## Mandatory handoff to the orchestrator

**AFTER merging all six branches** (root application, core, Win32, config, configurator, and documentation/release validation), the integration orchestrator must run the actual scripts against the actual combined workspace. This document does not require creating repositories/worktrees, invoking any particular orchestration CLI, publishing or pushing.

The documentation branch owns only `LICENSE`, `THIRD_PARTY_NOTICES.md`, `README.md`, `docs/` and `scripts/`. It deliberately does not create placeholder Cargo manifests/lockfiles or manufacture executables to make isolated verification appear successful. Resolve integration errors through the owning files/agents:

| Failure | Responsible ownership |
| --- | --- |
| Workspace membership, root package/binary, CLI/lifecycle/IPC | Root application owner |
| Geometry/easing API or tests | `crates/taskbarx-core` owner |
| Discovery, MSAA, movement/styles/restoration or tests | `crates/taskbarx-win32` owner |
| Schema, validation, persistence/sample compatibility | `crates/taskbarx-config` owner; coordinate sample documentation changes |
| GUI binary, engine invocation, form behavior | `crates/taskbarx-configurator` owner |
| Script bugs, inaccurate documentation/notices | Documentation/release owner |

Do not hide failures by dropping workspace members, skipping tests, weakening safety requirements, substituting upstream executables, or calling a partial build complete.

## Before running

- [ ] All six branches have been merged, not merely reported done.
- [ ] Root package is `taskbarx-rust`, producing `taskbarx.exe`; all four fixed `crates/taskbarx-*` members exist, with `taskbarx-configurator.exe` as the GUI target.
- [ ] Native Windows x64, `x86_64-pc-windows-msvc` Rust/Cargo/rustfmt, MSVC C++ tools and Windows SDK are available. Install Clippy only if choosing that optional gate. No npm or upstream .NET build is required.
- [ ] Review all automatic tests/build scripts for **no real taskbar mutation**. No engine run/stop/configure, GUI Apply, startup scheduling/registry writes or Explorer restarts. Tests of these paths use mocks/fakes.
- [ ] Review no-argument/help and `diagnose --json` implementations before the first real probe: they must be read-only. Do not assume the script can sandbox an unsafe executable.
- [ ] Check CLI/config/GUI agree with the fixed contract and schema in README and `docs/settings.sample.json`; do not silently change the contract to accommodate an integration bug.
- [ ] Avoid concurrent source edits/builds and cross-target Cargo overrides during evidence collection.

## Required executions

From the **integrated checkout**, not this isolated worktree:

```powershell
powershell.exe -NoProfile -File .\scripts\test-helpers.ps1
if ($LASTEXITCODE -ne 0) { throw 'Script helper tests failed' }
powershell.exe -NoProfile -File .\scripts\verify.ps1
if ($LASTEXITCODE -ne 0) { throw 'Combined verification failed' }
powershell.exe -NoProfile -File .\scripts\release.ps1
if ($LASTEXITCODE -ne 0) { throw 'Combined release failed' }
```

`release.ps1` deliberately calls verification again, then runs **`cargo build --release --workspace --locked --message-format=json`**. The duplicate gate ensures a standalone release invocation does not trust old evidence. Optional `-Clippy` on either entry point adds the lint gate.

When initially absent, verification runs `cargo generate-lockfile` in the complete root workspace. Every subsequent dependency-resolving verification/release command uses `--locked`. The root owner must review and commit Cargo's real generated `Cargo.lock`; do not hand-write it. Rerun release after final source/lockfile commits so the candidate's Git metadata identifies the intended revision. Formatting checks do not generate lockfiles or modify source.

If any command fails, preserve its logs, report the error to the owning component, integrate the correction and rerun the full gate. An isolated-worktree missing-manifest error is an expected refusal, **not validation success**.

## Output and acceptance

Output uses unique UTC/GUID directories and never removes an old package:

```text
dist/
  verification/<id>/                 # actual command logs and diagnose.json
  release-evidence/<id>/             # release.json, build logs, metadata, lockfile
  TaskbarX-Rust/<id>/                # portable payload; extract ZIP into its own folder
    taskbarx.exe
    taskbarx-configurator.exe
    LICENSE
    THIRD_PARTY_NOTICES.md
    README.md
    settings.sample.json
    docs/                           # includes supplemental notices if added
    build-metadata.json
    evidence/verification/
    evidence/build/                  # pre-archive snapshot, packaging still pending
    SHA256SUMS.txt
  TaskbarX-Rust-<id>.zip
  TaskbarX-Rust-<id>.zip.sha256
```

The exact artifact paths are printed and returned by the scripts. `release-evidence/<id>/release.json` is the authoritative final packaging status; the ZIP contains a pre-archive build snapshot, avoiding self-referential checksums. `SHA256SUMS.txt` hashes all payload files except itself. The adjacent `.zip.sha256` hashes the whole ZIP. All `dist/` material is generated output, **not source to stage/commit** (even if Git shows it as untracked).

Before claiming completion, the orchestrator must:

- [ ] Confirm actual verification and release statuses are `passed`, not helper-only or fake results. Review Cargo test output for coverage and any ignored tests; confirm formatting and workspace build gates ran.
- [ ] Confirm `diagnose.json` parses and review its shell/MSAA findings. Only read-only diagnostics were run; unsupported layouts or skipped measurements are not evidence of successful positioning.
- [ ] Confirm **both real release executables exist**, were reported by Cargo for the correct packages, and are x64 PE files. No upstream executables or placeholders may be used. Scripts check these conditions without launching release binaries.
- [ ] Confirm both portable executables and required documents/sample appear inside the ZIP as well as the staging folder. Independently recompute `Get-FileHash -Algorithm SHA256 -LiteralPath <file>` for the two EXEs and ZIP and compare to the recorded checksums. Check every payload entry when transporting the archive.
- [ ] Confirm compiler/Cargo versions, Git revision/status, root lockfile hash, command logs, dependency inventory and read-only/manual-test scope are retained. Build evidence may contain local filesystem paths; review before sharing it.
- [ ] Review dependency licenses and required notice texts using the resolved graph. Update `THIRD_PARTY_NOTICES.md` and `docs/licenses/` as necessary; metadata license expressions alone do not fulfill all notice obligations. Rebuild/package after changing notices.
- [ ] Report live positioning/styling/restoration/recovery and the entire manual matrix as **NOT EXECUTED** unless separately authorized tests really occurred and have their own evidence. A local package is not a claim of complete live feature parity.
- [ ] State that this is an independent local validation candidate. Do not publish, upload, sign with upstream credentials, push, enable startup or restart Explorer.

The procedure is reproducible as a sequence with locked dependency resolution, not a promise of identical binary or ZIP hashes across machines. Preserve toolchain and SDK/linker information when investigating differences; timestamps and local build environments can affect bytes.
