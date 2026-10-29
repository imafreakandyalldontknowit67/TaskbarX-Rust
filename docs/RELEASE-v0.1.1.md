# TaskbarX-Rust v0.1.1

Complete portable packages, following the original TaskbarX ZIP / Rainmeter download layout.

## Downloads

| Architecture | Portable ZIP | Optional Rainmeter launcher |
| --- | --- | --- |
| x64 (recommended for 64-bit Intel/AMD Windows) | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x64.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x64.rmskin) |
| x86 (32-bit; limited validation) | [Download ZIP](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x86.zip) | [Download .rmskin](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/releases/download/v0.1.1/TaskbarX-Rust_0.1.1_x86.rmskin) |

Each ZIP includes **both EXEs**, **Open Settings.cmd**, sample settings, instructions and license notices. Extract everything into one stable folder and open **Open Settings.cmd** or **taskbarx-configurator.exe**.

No .NET resource folders or libraries are needed. The MSVC CRT is statically linked; Windows system DLLs are still required. SHA256SUMS.txt covers all four downloads. GitHub's automatic source archives are source code, not the app.

## Changes

- Genuine x64 and x86 builds, with architecture checks during packaging.
- Fixed x86 window-userdata types and packed tray-tooltip access.
- Complete ZIPs instead of loose executable downloads.
- Optional Rainmeter packages with a click-to-open-settings launcher.

## Compatibility and use

**Windows 10 classic taskbar only.** x64 passed native build/test checks. x86 passed build/tests and read-only detection under WOW64 on Windows 10 x64; native 32-bit Windows and live effects were not tested. ARM64 is unavailable because its Microsoft build tools are missing. No ARM32 or neutral/AnyCPU build is provided.

Opening settings does not start the engine. **Apply** saves and starts/restarts it. **Startup > Enable / update** separately enables sign-in startup (default delay: six seconds).

For Rainmeter, install the matching `.rmskin`, manually load `TaskbarX-Rust-<arch>/Launcher.ini`, then click **Open Settings**. Installation/loading does not apply the engine or enable startup. Package structure was tested; live Rainmeter installation was not.

Binaries are **unsigned**; keep Windows security checks enabled. Actual sign-in launch and most visual effects have not been live-tested. The repository remains **private**. See [build evidence](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.1/docs/BUILD-v0.1.1.md) and [user guide](https://github.com/imafreakandyalldontknowit67/TaskbarX-Rust/blob/v0.1.1/docs/USER-GUIDE.md).

Independent Rust rewrite of Chris Andriessen's [TaskbarX](https://github.com/ChrisAnd1998/TaskbarX), not an official upstream release. v0.1.0 and its tag remain unchanged.
