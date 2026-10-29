# Distribution and package selection

TaskbarX-Rust is an **independent native Rust rewrite** of Chris Andriessen's TaskbarX, not an official upstream release. The initial release is unsigned; keep Windows security checks enabled. This document defines the distribution contract, not a claim that a new build or upload exists.

## Choose a package

**Portable ZIPs are the primary choice.** Check the [private v0.1.0 release page](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/tag/v0.1.0) for actually attached assets; repository access is required while private. The planned names below are conditional, not direct-download links:

| Architecture | Primary portable ZIP, if available | Optional Rainmeter package, if available |
| --- | --- | --- |
| x64 | `TaskbarX-Rust_0.1.0_x64.zip` | `TaskbarX-Rust_0.1.0_x64.rmskin` |
| x86 | `TaskbarX-Rust_0.1.0_x86.zip` | `TaskbarX-Rust_0.1.0_x86.rmskin` |
| ARM64 | `TaskbarX-Rust_0.1.0_arm64.zip` | None |

Match the native Windows installation's architecture; do not assume emulation support. **x86 and ARM64 are build candidates, not runtime-verified platforms.** There is no neutral/AnyCPU native Rust executable, supported ARM32 package or ARM64 Rainmeter package in this contract. Architecture matching does not establish shell compatibility.

GitHub's automatic **Source code (zip)** and **Source code (tar.gz)** archives are source, not ready-to-run application packages. They are distinct from uploaded assets and local developer evidence ZIPs.

## Architecture and verification

These are four separate questions: **can the packager handle the architecture, did the application compile, what runtime behavior was verified, and was an asset uploaded?** A filename or PE header alone cannot answer all four.

| Architecture / PE Machine | Packaging contract | Successful application compilation | Runtime evidence | Uploaded availability |
| --- | --- | --- | --- | --- |
| x64 / `0x8664` | ZIP; optional `.rmskin` | Historical native x64 debug/release builds recorded in [VALIDATION](VALIDATION.md); not proof of a new candidate | Limited native Windows 10 classic-taskbar scope below | Check release attachments; root decides |
| x86 / `0x014c` | ZIP; optional `.rmskin` | Build candidate; no successful build established here | Not runtime verified | Check release attachments; root decides |
| ARM64 / `0xAA64` | ZIP only | Build candidate; no successful build established here | Not runtime verified | Check release attachments; root decides |

Existing tested scope is **native Windows 10 x64 with the classic Explorer taskbar**, not comprehensive feature certification. Historical checks include pure/fake tests and read-only diagnostics. A separate startup repair verified Enable/update and status, including the account-name-to-SID fix, with a six-second delay. **Actual logon launch and most visual effects have not been live-tested.** Live positioning/restoration, visual parity, performance, real shortcuts/pinning and tray interaction are not established by those checks. See [user-guide evidence](USER-GUIDE.md#compatibility-and-evidence) and [testing boundaries](TESTING.md).

Modern Windows 11 taskbars remain **unsupported**. Shell patching and elevation are not promised workarounds. Appearance APIs remain build/theme dependent. Packaging does not broaden operating-system support, and no live Rainmeter installation test is established here.

## Portable ZIP layout

The two matching EXEs are beside each other **at the archive root**, with no enclosing application directory:

```text
taskbarx.exe
taskbarx-configurator.exe
Open Settings.cmd
LICENSE
THIRD_PARTY_NOTICES.md
settings.sample.json
README.txt
docs/
  licenses/  (dependency license texts and their README)
```

This is the complete portable payload contract. `README.txt` is a concise package-specific guide, **not the repository README renamed or copied**. Only `docs/licenses/` is bundled from `docs/`; the repository README, user guide, release notes and this distribution document stay in the repository. Their presence is not required inside a package. The sample is `settings.sample.json` at the payload root; `docs/settings.sample.json` remains correct only for a source checkout.

Excluded: the source tree and build scripts; developer evidence; private diagnostics, logs, dependency inventories and build metadata; artwork and icons; upstream EXEs, .NET DLLs and their resource directories. Links in retained notices to excluded repository documents require repository access; they do not imply those documents are bundled.

## Portable ZIP workflow

1. If the matching ZIP is attached, download it and extract **all files** into a stable folder. Do not run from within the ZIP or separate the two EXEs.
2. Open `Open Settings.cmd` or `taskbarx-configurator.exe`. Opening settings does not start the engine.
3. **Save** validates and persists JSON only. **Apply** saves and starts/restarts the engine without enabling startup.
4. If wanted, save settings and explicitly choose **Startup > Enable / update**. Registration uses saved settings and the selected delay, default **six seconds**. It does not save the form or immediately start the engine; changing the delay alone does not update registration.
5. Closing the GUI leaves a running engine running. **Stop TaskbarX** requests graceful restoration; crashes/force termination can bypass cleanup, and exact restoration of third-party effects is not guaranteed.

Settings default to `%APPDATA%\TaskbarX-Rust\settings.json`, **not the portable folder**; `--config PATH` selects another location. The bundled sample is not automatically imported. See [GUI actions](USER-GUIDE.md#configurator-actions) and [package-root CLI examples](USER-GUIDE.md#examples).

## Optional Rainmeter workflow

Rainmeter is optional and must be installed separately by the user. The contract is a genuine, format-compatible `.rmskin` archive with `RMSKIN.ini` and the RMSKIN footer, **not just a renamed portable ZIP**. For x64 or x86, its layout is:

```text
RMSKIN.ini
Skins/
  TaskbarX-Rust-<arch>/
    Launcher.ini
    @Resources/
      TaskbarX-Rust/
        (the complete portable payload listed above)
```

The footer is archive-format data, not another payload file. The format follows Rainmeter's skin-package model; see the [official distributing-skins documentation](https://docs.rainmeter.net/manual/distributing-skins/). Format compatibility does **not** validate the bundled EXEs' processor architecture, prove authenticity, grant endorsement or establish live Rainmeter testing.

1. Choose an actually available matching x64/x86 `.rmskin`. With Rainmeter separately installed, open the package in its Skin Installer, review the contents and choose Install. Rainmeter documents double-click installation for a normal installation and using `SkinInstaller.exe` for a portable installation. See the [official installation documentation](https://docs.rainmeter.net/manual/installing-skins/).
2. **Manually load `TaskbarX-Rust-<arch>/Launcher.ini`** in Rainmeter. This package's workflow does not depend on automatic loading after installation.
3. **Explicitly click the skin's settings launcher.** That click only opens the configurator from `@Resources/TaskbarX-Rust/`.
4. Use **Apply** later to save and start/restart the engine. Use **Startup > Enable / update** separately if wanted, with saved settings and the default six-second delay unless changed.

**Installation, loading and refresh do not launch the configurator, apply the engine or register TaskbarX startup.** Rainmeter's own launch-at-sign-in setting is not TaskbarX engine startup. Settings still default to `%APPDATA%\TaskbarX-Rust\settings.json`; the skin resources directory does not become a portable settings directory. No live Rainmeter installation or launcher test is claimed here.

## Update, move or remove

Stop TaskbarX and confirm completion, back up settings, then close the GUI before replacing/removing files. Update the whole payload rather than mixing engine/configurator versions. Explicitly disable the owned startup task **before** moving/removing registered executable or config paths, then use **Enable / update** from the new location if desired. Keep registered paths stable.

This also applies before replacing/removing a Rainmeter skin or relocating its `@Resources/TaskbarX-Rust/` payload. Removing/unloading a skin is not a TaskbarX stop or startup-removal action. Remove owned custom shortcuts separately; the application does not delete its folder. Upstream TaskbarX tasks are never automatically migrated or removed. See [update/removal guidance](USER-GUIDE.md#quick-use).

## Offline packaging and fixture checks

`scripts/package-distribution.ps1` has three mandatory arguments: `-BinaryDirectory`, `-Architecture x64|x86|arm64` and `-OutputDirectory`. Optional `-Version` defaults to `0.1.0`. It consumes an **already-built matching pair** named `taskbarx.exe` and `taskbarx-configurator.exe`, checks their actual PE Machine values against the selected architecture, and packages offline **without running either executable**. It requires a nonexistent or empty output directory. It is not a compiler, runtime test, signer or publisher.

Illustrative commands from a source checkout once the packaging scripts are integrated; replace the placeholder paths with reviewed prebuilt pairs and fresh outputs. Run only the architecture commands for which real matching binaries are available:

```powershell
powershell.exe -NoProfile -File .\scripts\package-distribution.ps1 `
  -BinaryDirectory 'C:\path\to\prebuilt\x64' -Architecture x64 `
  -OutputDirectory 'C:\path\to\fresh-output\x64' -Version 0.1.0
powershell.exe -NoProfile -File .\scripts\package-distribution.ps1 `
  -BinaryDirectory 'C:\path\to\prebuilt\x86' -Architecture x86 `
  -OutputDirectory 'C:\path\to\fresh-output\x86' -Version 0.1.0
powershell.exe -NoProfile -File .\scripts\package-distribution.ps1 `
  -BinaryDirectory 'C:\path\to\prebuilt\arm64' -Architecture arm64 `
  -OutputDirectory 'C:\path\to\fresh-output\arm64' -Version 0.1.0

# Archive-structure fixtures only, not an application build or runtime test:
powershell.exe -NoProfile -File .\scripts\test-distribution.ps1
```

`scripts/test-distribution.ps1` uses **inert fake PE fixtures and never launches the app**. Passing it is archive-structure evidence, not a successful application build, working native binary or runtime verification. Never publish its fixtures. These command examples are not executed-test results and do not establish that the proposed assets exist.

## Separate local build and verification workflows

This repository is a native Rust/Cargo workspace, with no `package.json` or npm build. The existing `scripts/verify.ps1` and `scripts/release.ps1` are **x64-oriented local workflows**, not general cross-architecture distribution packagers. They require the native x64 MSVC toolchain and locked Cargo gates. Their desktop probe is read-only `diagnose --json`; they do not apply the engine, launch the GUI or test real startup registration.

The existing release workflow copies broad documentation and local verification/build evidence into its candidate ZIP, including potentially private paths, diagnostics and metadata. **Do not upload its output unchanged or bulk-upload a `dist/` directory.** `scripts/package-preview.ps1` is an older x64-oriented repackager of an explicitly supplied checksummed candidate; it has a different payload/name and is not the architecture distribution contract above. Keep developer evidence private and package only reviewed binaries and the allowlisted payload. See [local build instructions](USER-GUIDE.md#build-verify-and-package-locally).

## Publication gates (root/release owner only)

Root/release owner owns builds, actual asset selection and publication. Before any upload:

1. Establish the exact source revision, compiler-reported artifacts, matching CLI/GUI pair and hashes for each architecture selected. PE Machine checks establish architecture, not provenance or successful compilation. Never substitute upstream binaries or fake fixtures.
2. Reconcile release labels with the actual build. **This checkout's workspace version is `0.1.1`, while the configurator manifest is `0.1.0`.** The packager's default `-Version 0.1.0` labels archives; it does not rewrite embedded versions or prove a v0.1.0 build. Root must resolve this before deciding what belongs on the v0.1.0 release; renaming an archive is not resolution.
3. Retain real build/test evidence separately for each selected architecture. Keep compilation, fixture tests and authorized live runtime tests distinct. Do not transfer x64 evidence to x86/ARM64 or infer live Rainmeter behavior; withhold an asset or label its candidate/unverified scope honestly.
4. Inspect the actual archives against the exact names/layouts above and review licenses/notices for the resolved dependencies. Exclude all private evidence, artwork and unrelated material. Preserve `LICENSE`, `THIRD_PARTY_NOTICES.md` and `docs/licenses/`; inventories/checksums are not license clearance, signatures or authenticity guarantees.
5. Select only reviewed assets, retain unsigned/security and compatibility warnings, and confirm actual attachments on the release page after any separately authorized publication. Packaging does not upload, push, change tags or change repository visibility. No documentation table or historical build establishes availability.

## Why the layout differs from upstream

Upstream-like architecture and Rainmeter choices do not require upstream .NET internals. This native Rust rewrite does not need upstream ModernWPF, TaskScheduler or Emoji.Wpf DLLs or their resource directories; upstream binaries must never be relabeled as Rust builds. Actual dependencies are determined by Cargo, and this does **not** imply every system dependency is statically linked.

Preserve Chris Andriessen's attribution and Copyright (c) 2021 under the [MIT license](../LICENSE), plus [third-party notices](../THIRD_PARTY_NOTICES.md) and [dependency license texts](licenses/README.md). Repository artwork depicts upstream, not this Rust build, and is not bundled; see [artwork provenance](assets/README.md). Neither the project nor its package format implies upstream, Rainmeter, Rust project or Rust Foundation endorsement.
