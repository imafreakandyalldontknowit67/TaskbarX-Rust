<p>
  <img src="docs/assets/taskbarx-rust-banner.png" width="640" alt="Composite of Chris Andriessen's TaskbarX banner with an embedded Rust badge near the bottom-right TaskbarX logo; upstream-derived artwork, not this Rust build">
</p>

# TaskbarX-Rust

An **independent Rust rewrite of Chris Andriessen's [TaskbarX](https://github.com/ChrisAnd1998/TaskbarX)** for taskbar positioning and appearance. Not an official upstream release.

**v0.1.1** provides complete, unsigned portable packages. The repository remains private.

## Downloads / quick start

Download one package, not separate EXEs. [Latest release](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/latest).

| Architecture | Portable ZIP | Optional Rainmeter launcher |
| --- | --- | --- |
| x64 (recommended for 64-bit Intel/AMD Windows) | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x64.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x64.rmskin) |
| x86 (32-bit; limited validation) | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x86.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x86.rmskin) |

**Windows 10 classic taskbar only.** x64 passed native build/test checks. x86 passed build/tests and read-only detection under WOW64 on Windows 10 x64; native 32-bit Windows and live effects were not tested. ARM64 is unavailable because its Microsoft build tools are missing. No ARM32 or neutral/AnyCPU build is provided. See [build evidence](docs/BUILD-v0.1.1.md) and [package details](docs/DISTRIBUTION.md).

1. Download the matching ZIP and **extract all files into a stable folder**, keeping both EXEs together. Keep Windows security checks enabled; the binaries are unsigned.
2. Open `Open Settings.cmd` or `taskbarx-configurator.exe`. Opening settings **does not start the engine**.
3. Select your settings and press **Apply** to save and start/restart the engine. **Save** only persists JSON; **Apply never enables startup**.

Each ZIP includes both EXEs, `Open Settings.cmd`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, root-level `settings.sample.json`, a concise `README.txt` and `docs/licenses/`. Repository Markdown guides, source, artwork and private developer evidence are not bundled. GitHub's automatic **Source code (zip)** and **Source code (tar.gz)** archives are source, **not ready-to-run downloads**.

Rainmeter is optional and separately installed: use an available matching `.rmskin`, manually load `Launcher.ini`, then explicitly click its settings launcher. Installation/loading/refresh does not launch the configurator, apply the engine or register TaskbarX startup. The click only opens settings; Apply and Startup remain separate actions. See the [Rainmeter workflow and official references](docs/DISTRIBUTION.md#optional-rainmeter-workflow).

Closing the GUI leaves the engine running. Use **Stop TaskbarX** to request graceful stop/restoration. Cleanup is not guaranteed after crashes or force termination, nor is exact restoration of every third-party effect. See the [user guide](docs/USER-GUIDE.md).

## Features

- Primary/secondary taskbar centering, signed offsets and centering between controls and tray.
- 42 easing choices, including instant positioning; configurable animation and polling.
- Transparent, transparent-gradient, blur, acrylic and opaque backgrounds, or Windows default.
- RGBA color, separate taskbar opacity, rounded corners, segments and Start/tray visibility.
- Battery overrides, fullscreen pause and resolution-based centering exceptions.
- Native settings GUI, read-only diagnostics, optional tray icon and explicit startup/shortcut actions.

See [settings and defaults](docs/USER-GUIDE.md#settings) and [behavioral safeguards](docs/USER-GUIDE.md#safety-and-behavior). Availability depends on the supported shell and successful measurements; this is not a visual-parity or performance claim.

## Windows compatibility

Modern Windows 11 taskbars are **unsupported**. Shell patching and elevation are not supported workarounds. Appearance APIs are build/theme dependent; avoid competing taskbar utilities.

Startup registration and status have been verified on Windows 10, including the account-name-to-SID fix. **Actual logon launch and most visual effects have not been live-tested.** Automated checks do not verify all features or restoration; see [verification scope](docs/USER-GUIDE.md#compatibility-and-evidence).

## Start icon files

Optional user-supplied `.ico` files can decorate Start-menu shortcuts; **pinning remains manual**. These icons are Chris Andriessen / upstream TaskbarX downloads, not bundled assets:

[start.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/start.ico) · [startdark.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/startdark.ico) · [startlight.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/startlight.ico) · [startcolor.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/startcolor.ico)

See [shortcut instructions and icon limitations](docs/USER-GUIDE.md#start-shortcuts-icons-and-shell-limitations).

## Commandline examples

From PowerShell in the extracted package folder containing both EXEs:

```powershell
.\taskbarx.exe --help
.\taskbarx.exe diagnose --json  # Read-only
.\taskbarx.exe run --primary-offset -20 --animation cubiceaseinout
# Run stays in the foreground; stop from another PowerShell window:
.\taskbarx.exe stop --timeout-ms 5000
```

`run` intentionally applies settings; its overrides are not saved. **Legacy flags such as `-tbs=1`, `-as=...` and `-stop` are unsupported.** See [all commands/options](docs/USER-GUIDE.md#commands).

## FAQ

**How do I start at sign-in?** Save your settings, then explicitly choose **Startup > Enable / update** using the saved configuration and selected delay (default **six seconds**). Editing the delay alone does not update registration. Save, Apply and `run` never register startup. **Disable / remove** removes only this rewrite's owned task.

**Where are settings stored?** `%APPDATA%\TaskbarX-Rust\settings.json`. Portable binaries do not imply portable settings; `--config PATH` selects another file.

**How do I update or remove it?** Stop first and confirm completion, back up settings, and explicitly disable/update owned startup before moving executable/config paths, including paths under Rainmeter. Close the GUI before replacing the package or removing its folder. Remove owned shortcuts separately. Upstream tasks are **not** automatically migrated or removed. See [update/removal details](docs/USER-GUIDE.md#quick-use).

## Credits

Original TaskbarX by **Chris Andriessen**. **Copyright (c) 2021 Chris Andriessen**, preserved under the [MIT license](LICENSE); see [third-party notices](THIRD_PARTY_NOTICES.md).

Upstream-like package choices do not reproduce upstream .NET internals: this native Rust rewrite does not need upstream ModernWPF, TaskScheduler or Emoji.Wpf DLLs or their resource directories. Never relabel upstream binaries as Rust builds. Actual dependencies come from Cargo (including `clap`, `serde`, `serde_json` and `windows`); this is not a claim that every system dependency is statically linked. See [guide attribution](docs/USER-GUIDE.md#license-and-attribution).

The composite combines Chris Andriessen's upstream TaskbarX banner with an embedded Rust badge; it depicts upstream, not this build, and is not bundled in distribution packages. Separate artwork rights require review before public distribution. Rust logo artwork is by Rust project contributors (CC BY 4.0); no Rust project or Foundation endorsement is implied. Package formats confer no endorsement. See [artwork provenance](docs/assets/README.md).
