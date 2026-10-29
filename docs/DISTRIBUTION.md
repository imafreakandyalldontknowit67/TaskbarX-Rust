# Distribution and package selection

TaskbarX-Rust is an **independent native Rust rewrite** of Chris Andriessen's TaskbarX, not an official upstream release. This document describes the unsigned v0.1.3 packages. Keep Windows security checks enabled.

## Choose a package

**Portable ZIPs are the primary choice.** Download the matching package from the [v0.1.3 release](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/tag/v0.1.3).

| Architecture | Portable ZIP | Optional Rainmeter launcher |
| --- | --- | --- |
| x64 (64-bit Intel/AMD Windows) | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x64.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x64.rmskin) |
| x86 (32-bit; limited validation) | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x86.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.3/TaskbarX-Rust_0.1.3_x86.rmskin) |

**Windows 10 classic taskbar only; Windows 11 is unsupported.** ARM64 is unavailable and unvalidated. No ARM32 or neutral/AnyCPU build is provided. x64 and x86 evidence is summarized below.

GitHub's automatic **Source code (zip)** and **Source code (tar.gz)** archives are source, not ready-to-run application packages. They are distinct from uploaded assets and local developer evidence ZIPs.

## Architecture and verification

Packaging support, compilation, runtime verification and download availability are separate. A filename or PE header alone does not establish any of the latter three. See the [v0.1.3 validation summary](BUILD-v0.1.3.md).

| Architecture / PE Machine | Packaging support | build/runtime evidence |
| --- | --- | --- |
| x64 / `0x8664` | ZIP and optional `.rmskin` | Native workspace tests, strict Clippy, debug/release builds and read-only detection passed |
| x86 / `0x014c` | ZIP and optional `.rmskin` | Real x86 build, tests/Clippy and read-only detection passed under WOW64; native 32-bit Windows and live effects untested |
| ARM64 / `0xAA64` | ZIP format only; no available binary | No completed build or runtime validation |

See [v0.1.3 build evidence](BUILD-v0.1.3.md). The x64 and x86 release builds enable static MSVC CRT linkage. Windows system DLLs are still required.

runtime checks used **Windows 10 x64 with the classic Explorer taskbar**, with x86 running under WOW64; native 32-bit Windows remains untested. A separate startup repair verified Enable/update and status, including the account-name-to-SID fix, with a six-second delay. **Actual logon launch and most visual effects have not been live-tested.** Live positioning/restoration, visual parity, performance, real shortcuts/pinning and tray interaction are not established by those checks. See [user-guide evidence](USER-GUIDE.md#compatibility-and-evidence) and [testing boundaries](TESTING.md).

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

Excluded: source and build scripts; local diagnostics, logs, dependency inventories and build metadata; artwork and icons; upstream EXEs, .NET DLLs and their resource directories. Links in retained notices refer to repository documents, not bundled files.

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

On PowerShell 5.1+, `scripts/package-distribution.ps1` has three mandatory arguments: `-BinaryDirectory`, `-Architecture x64|x86|arm64` and `-OutputDirectory`. It consumes an **already-built matching pair** named `taskbarx.exe` and `taskbarx-configurator.exe`, checks their actual PE Machine values against the selected architecture, and packages offline **without running either executable**. It is not a compiler, runtime test, signer or publisher.

Output must be nonexistent or empty, including hidden entries, and must not overlap the binary directory. Reparse points and reparse-point ancestors are rejected. Files are created exclusively; existing output is not overwritten, and partial archives are retained on failure.

The script's optional `-Version` currently defaults to `0.1.3`; the examples explicitly select `0.1.3`. This changes archive labels, not executable versions. The source versions, matching binaries and intended source tag must agree before packaging.

Illustrative commands from a source checkout; replace the placeholder paths with verified v0.1.3 pairs and fresh outputs. These examples do not establish that those binaries exist:

```powershell
powershell.exe -NoProfile -File .\scripts\package-distribution.ps1 `
  -BinaryDirectory 'C:\path\to\prebuilt\x64' -Architecture x64 `
  -OutputDirectory 'C:\path\to\fresh-output\x64' -Version 0.1.3
powershell.exe -NoProfile -File .\scripts\package-distribution.ps1 `
  -BinaryDirectory 'C:\path\to\prebuilt\x86' -Architecture x86 `
  -OutputDirectory 'C:\path\to\fresh-output\x86' -Version 0.1.3

# Archive-structure fixtures only, not an application build or runtime test:
powershell.exe -NoProfile -File .\scripts\test-distribution.ps1
```

`scripts/test-distribution.ps1` uses **inert fake PE fixtures and never launches the app**. Passing it is archive-structure evidence, not an application build or runtime verification; fixtures are not distribution binaries. ARM64 ZIP construction is supported by the packager, but no ARM64 binary or runtime validation is available.

## Separate local build and verification workflows

This repository is a Rust 2021 Cargo workspace. `scripts/verify.ps1` and `scripts/release.ps1` are **x64-oriented local workflows**, not general cross-architecture distribution packagers. They require the native x64 MSVC toolchain and locked Cargo checks. Their desktop probe is read-only `diagnose --json`; they do not apply the engine, launch the GUI or test real startup registration.

The local release workflow creates an **evidence package**, with broad documentation, local paths, diagnostics and build metadata. It is not a distribution archive. `scripts/package-preview.ps1` is an older x64-oriented repackager of a checksummed local candidate with a different payload/name. Use `package-distribution.ps1` for the allowlisted ZIP/Rainmeter layout above and retain build evidence separately. See [local build instructions](USER-GUIDE.md#build-verify-and-package-locally) and the [validation checklist](INTEGRATION.md).

## Package integrity

Retain the source revision, compiler-reported CLI/GUI artifacts and hashes for each architecture. Inspect archive entries against the layouts above and verify that embedded binaries match the supplied pair. PE checks establish declared architecture, not build provenance; x64 evidence does not validate x86 or ARM64.

Keep `LICENSE`, `THIRD_PARTY_NOTICES.md` and applicable `docs/licenses/` texts with the payload. Review notices against the resolved dependencies. Checksums detect byte differences; they are not signatures or license-compliance certification. Packaging preserves executable bytes and requires privacy checks on the input binaries and final archives. Build-time remapping removes workstation paths before packaging.

## Why the layout differs from upstream

Upstream-like architecture and Rainmeter choices do not require upstream .NET internals. This native Rust rewrite does not need upstream ModernWPF, TaskScheduler or Emoji.Wpf DLLs or their resource directories; upstream binaries must never be relabeled as Rust builds. Actual dependencies are determined by Cargo, and this does **not** imply every system dependency is statically linked.

Preserve Chris Andriessen's attribution and Copyright (c) 2021 under the [MIT license](../LICENSE), plus [third-party notices](../THIRD_PARTY_NOTICES.md) and [dependency license texts](licenses/README.md). The repository banner adapts the upstream TaskbarX collage with an integrated Rust-style badge, is not a screenshot of the Rust build, and is not bundled. Separate wallpaper/artwork redistribution rights have not been established; see [artwork provenance](assets/README.md). Neither the project nor its package format implies upstream, Rainmeter, Rust project or Rust Foundation endorsement.

## Distribution checks

Verification enables source-path remapping automatically. Before manual cross-compilation, run `. ./scripts/set-build-path-remapping.ps1` in the build shell. Packaging checks both executables and the completed archives for home-directory paths and credential-shaped content; detected findings stop packaging. These heuristic checks complement code review, not replace it.

Run the scanner independently with `./scripts/check-distribution-privacy.ps1 -Path <package>` and check `$LASTEXITCODE`. Fixture tests are in `scripts/test-distribution-privacy.ps1`.
