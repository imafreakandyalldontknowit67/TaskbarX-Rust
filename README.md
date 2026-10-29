<p>
  <img src="docs/assets/taskbarx-banner.png" width="640" alt="Original TaskbarX banner by Chris Andriessen; upstream artwork, not this Rust build">
  <img src="docs/assets/rust-logo.svg" width="80" alt="Rust programming language logo, displayed separately">
</p>

# TaskbarX-Rust

An **independent Rust rewrite of Chris Andriessen's [TaskbarX](https://github.com/ChrisAnd1998/TaskbarX)** for taskbar positioning and appearance. Not an official upstream release.

**v0.1.0 is an unsigned first preview**, targeting **native Windows 10 x64 with the classic Explorer taskbar only**.

## Downloads / quick start

- **Recommended: [portable ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.0/TaskbarX-Rust-v0.1.0-windows-x64.zip)** · [v0.1.0 release page](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/tag/v0.1.0)
- Individual downloads: [taskbarx.exe](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.0/taskbarx.exe) and [taskbarx-configurator.exe](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.0/taskbarx-configurator.exe). **Both EXEs must be in the same folder.**

These are planned release links, not confirmation that publishing has occurred. This preview is unsigned; keep Windows security checks enabled.

1. Extract the ZIP to a stable folder.
2. Open `taskbarx-configurator.exe` or the packaged `Open Settings.cmd`. Opening the GUI **does not start the engine**.
3. Select your settings and press **Apply** to save and start/restart the engine. **Save** only persists JSON; **Apply never enables startup**.

Closing the GUI leaves the engine running. Use **Stop TaskbarX** to request graceful stop/restoration. Cleanup is not guaranteed after crashes or force termination, nor is exact restoration of every third-party effect. See the [user guide](docs/USER-GUIDE.md).

## Features

- Primary/secondary taskbar centering, signed offsets and centering between controls and tray.
- 42 easing choices, including instant positioning; configurable animation and polling.
- Background styles, RGBA color, separate opacity, rounding/segments and visibility controls.
- Native settings GUI, read-only diagnostics, optional tray icon and explicit startup/shortcut actions.

See [settings and defaults](docs/USER-GUIDE.md#settings) and [behavioral safeguards](docs/USER-GUIDE.md#safety-and-behavior). Availability depends on the supported shell and successful measurements; this is not a visual-parity or performance claim.

## Windows compatibility

Modern Windows 11 taskbars are **unsupported**. Shell patching and elevation are not supported workarounds. Appearance APIs are build/theme dependent; avoid competing taskbar utilities.

Source `ac69090` includes the live-verified startup enable/status account-name-to-SID fix. **Actual logon launch and most visual effects have not been live-tested.** Automated checks do not verify all features or restoration; see [verification scope](docs/USER-GUIDE.md#compatibility-and-evidence).

## Start icon files

Optional user-supplied `.ico` files can decorate Start-menu shortcuts; **pinning remains manual**. These icons are Chris Andriessen / upstream TaskbarX downloads, not bundled assets:

[start.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/start.ico) · [startdark.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/startdark.ico) · [startlight.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/startlight.ico) · [startcolor.ico](https://github.com/ChrisAnd1998/TaskbarX/releases/download/1.7.5.0/startcolor.ico)

See [shortcut instructions and icon limitations](docs/USER-GUIDE.md#start-shortcuts-icons-and-shell-limitations).

## Commandline examples

From PowerShell in the extracted folder:

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

**How do I update or remove it?** Stop first, preserve settings, and explicitly disable/update owned startup before moving executable/config paths. Close the GUI before replacing both EXEs or removing the folder. Remove owned shortcuts separately. Upstream tasks are **not** automatically migrated or removed. See [update/removal details](docs/USER-GUIDE.md#quick-use).

## Credits

Original TaskbarX by **Chris Andriessen**. **Copyright (c) 2021 Chris Andriessen**, preserved under the [MIT license](LICENSE); see [third-party notices](THIRD_PARTY_NOTICES.md).

Upstream ModernWPF, TaskScheduler and Emoji.Wpf references are .NET libraries, not this rewrite's Rust dependencies. Actual dependencies come from Cargo (including `clap`, `serde`, `serde_json` and `windows`); see [guide attribution](docs/USER-GUIDE.md#license-and-attribution).

The banner depicts upstream, not this build; separate artwork rights require review before public distribution. The separate Rust logo is by Rust project contributors (CC BY 4.0), without endorsement. See [artwork provenance](docs/assets/README.md).
