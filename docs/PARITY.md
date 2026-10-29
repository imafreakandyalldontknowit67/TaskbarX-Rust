# Upstream parity inventory

## Scope and evidence convention

Reviewed in full on 2026-10-29 UTC from the local reference:
`reference/TaskbarX-master`.

This inventory covers **every field in Settings.vb, every switch parsed by Main.vb, and every functional configurator control/action**, including hidden, disabled and dormant items. Decorative labels/images are grouped separately. Upstream source links below point to the public repository; line numbers and hashes identify the **local snapshot**, not a claim that a moving remote branch is identical. This review did not execute upstream binaries or mutate the desktop.

| Reference | Local lines | SHA-256 |
| --- | ---: | --- |
| [Settings.vb][settings] | 48 | `3c1690f272d33ece29609bd1579f07e6668b0ca240461c7d4a3e07d717ae0877` |
| [Main.vb][main] | 422 | `f68809f32a72ecf252737cfde327c79d45e3d8cd7a8a3eef93c9ff0f08aa2bc4` |
| [TaskbarStyle.vb][style] | 269 | `6a03fb32d2c0f0e55c74c1ee9cd7a592fe3c1326171381660b26aa2b34f8c048` |
| [TaskbarCenter.vb][center] | 1192 | `4e880f33c9f076c0b02a62ef6ed368735c2409250327ea79fa7abae3a57b2ac2` |
| [TrayIconBuster.vb][buster] | 355 | `408b3b73b348fb537cf1c69d6357a781e29a07094cbb6597a8eaef2cbb547adf` |
| [MainWindow.xaml][xaml] | 290 | `3bfd0116e91fe755aaf421933bf62e84f04d3373e2ad9c712888567799287035` |
| [MainWindow.xaml.vb][gui] | 2418 | `708d437b71d77f0846c8629d2b1437cb48ff47d61b36d63f30d23f8f7691259d` |

**Status labels:** **mapped** means a source/API/test location exists, not that live behavior was tested. Source integration was reviewed on 2026-10-29 against the six-package workspace; historical test results are recorded in [VALIDATION](VALIDATION.md). **excluded** is deliberate. **dormant upstream** means no functional consumer was found in the reviewed source/tree search. v0.1.2 verification is pending.

### Rust source/test destinations

- **C**: [shared Settings/defaults/validation](../crates/taskbarx-config/src/settings.rs), [config tests](../crates/taskbarx-config/tests/settings.rs).
- **G**: [GUI form](../crates/taskbarx-configurator/src/settings.rs), [controller](../crates/taskbarx-configurator/src/controller.rs), [protocol](../crates/taskbarx-configurator/src/protocol.rs), [native UI](../crates/taskbarx-configurator/src/native.rs), [fake-process contract tests](../crates/taskbarx-configurator/tests/contract.rs).
- **R**: [CLI](../src/cli.rs), [runtime](../src/runtime.rs), [lifecycle](../src/lifecycle.rs), [CLI tests](../tests/cli.rs), [cross-crate contracts](../tests/contracts.rs).
- **W**: [Win32 Session/API](../crates/taskbarx-win32/src/lib.rs), [native adapter](../crates/taskbarx-win32/src/platform.rs), [ownership/controller](../crates/taskbarx-win32/src/state.rs), [fake state tests](../crates/taskbarx-win32/src/state_tests.rs), [accessibility tests](../crates/taskbarx-win32/src/accessibility_tests.rs).
- **K**: [pure geometry](../crates/taskbarx-core/src/geometry.rs), [geometry tests](../crates/taskbarx-core/tests/geometry.rs), [easing tests](../crates/taskbarx-core/tests/animation.rs).
- **S**: [public shell types](../crates/taskbarx-shell/src/lib.rs) `LaunchSpec`/`StartupStatus` and `format_command_line`; [startup protocol/COM](../crates/taskbarx-shell/src/startup.rs) `startup_status/enable_startup/disable_startup`; [owned shortcuts](../crates/taskbarx-shell/src/shortcut.rs) `create_start_shortcut/remove_start_shortcut`; [Start action](../crates/taskbarx-shell/src/start_menu.rs) `show_start_menu`; [tray](../crates/taskbarx-shell/src/tray.rs) `TrayIcon::new/poll_actions/close` and `TrayAction::{OpenSettings,Restart,Stop}`. These APIs and fake tests are **mapped in source**; CLI/runtime/GUI wiring is present and covered by injected-service, runtime and hidden-control tests. [SHELL.md](SHELL.md) links native adapters, test names and limitations.

The baseline has named tests such as C's `fixed_defaults_and_json_keys`, `full_round_trip_and_individually_omitted_fields`, `every_supported_easing_and_style_is_exact_and_round_trips`; G's `initialize_edit_save_reset_reload_and_drop_never_run_or_stop` and `apply_has_save_stop_wait_launch_order_and_no_other_commands`; R's `default_help_and_config_show_make_zero_files` and `diagnose_json_is_read_only_and_uncontaminated`. S has fake scheduler rollback/ownership/status tests, fake link collision/cleanup tests, injected Start tests and bounded tray queue/cleanup/recovery tests listed in [SHELL](SHELL.md). Test existence does not mean a current gate was run. Extended settings/provider and cross-component shell tests are part of the integrated workspace gate.

## Settings and engine-switch inventory

All declarations are in [Settings.vb, lines 4-46][settings]. Switch parsing is [Main.vb, lines 36-153][main]. Historical switches are documented for translation only; the Rust CLI does **not** accept them as aliases.

| Upstream field(s) | Legacy input / UI | Behavior/source | Rust destination and status |
| --- | --- | --- | --- |
| `Pause` | Internal, no switch | Fullscreen loop toggles it; Center 395-421 | R fullscreen policy; **mapped**, not a persisted pause flag |
| `TaskbarStyle` | `-tbs=`; six style radios | Main 17, 51-53; Style 166-185 | C `style`, W `Style`/Session, R/G; five baseline styles **mapped**, distinct `transparent-gradient` **mapped** |
| `SecondaryTaskbarStyle` | No parser/control | Declaration only; no consumer found | **Dormant upstream**; no invented secondary-style option |
| `PrimaryTaskbarOffset` | `-ptbo=`; NumericUpDown1 | Main 65-67; Center 829-843 | C `primary_offset`, R/K/G; **mapped** |
| `SecondaryTaskbarOffset` | `-stbo=`; NumericUpDown2 | Main 68-70; Center 842 | C `secondary_offset`, R/K/G; **mapped** |
| `CenterPrimaryOnly` | `-cpo=`; CheckBox2 | Center 995-1009, 1043-1058 | C `primary/secondary`, R `--only-primary`; **mapped**, Rust selection also scopes appearance |
| `CenterSecondaryOnly` | `-cso=`; CheckBox3 | Center 1010-1025, 1059-1075 | C `primary/secondary`, R `--only-secondary`; **mapped**, selectors mutually exclusive in CLI |
| `AnimationStyle` | `-as=`; ComboBox1 | Main 22; Center 1103-1187; GUI 267-310 | C `animation`, K/R/G, all 42 names; **mapped** |
| `AnimationSpeed` | `-asp=`; NumericUpDown4 | Main 23, 165-168; Center animator | C `animation_ms`, R/K/G; **mapped**; Rust zero duration is instant rather than silently clamped to 1 |
| `LoopRefreshRate` | `-lr=`; NumericUpDown3 | Main 24; Center 519-523 | C `poll_ms`, R/G; **mapped**, validated limits |
| `CenterInBetween` | `-cib=`; CheckBox1 | Center 829-840 | C `center_between`, K/R/G; **mapped** with signed coordinates/reserved spans |
| `FixToolbarsOnTrayChange` | `-ftotc=`; CheckBox4 | Main 27; Center 46-50, 551-632 | C `toolbar_compensation=true`, R/W; **mapped**, no upstream redraw-suppression busy loop |
| `SkipResolution`, `SkipResolution2`, `SkipResolution3` | `-sr=`, `-sr2=`, `-sr3=`; three numeric boxes | Center 374-393 checks primary-screen width, resets/exits centering loop | C `skip_resolutions=[]`, R/G; **mapped**: suspend centering only and resume when width changes |
| `CheckFullscreenApp` | `-cfsa=`; Checkbox9 | Center 395-421 pauses loop for foreground monitor-filling bounds | C `pause_on_fullscreen=false`, R/W; **mapped**: suspend polling/mutation but allow shutdown |
| `DefaultTaskbarStyleOnWinMax` | `-dtbsowm=`; Checkbox10 | Style 82-155, 205-208 excludes taskbars on maximized-window monitors | C `default_style_on_maximized=false`, R/W; **mapped**, per-monitor policy |
| `DontCenterTaskbar` | `-dct=`; CheckBox11 | Main 241-245 skips center thread | C `center` (inverse), R `--no-center`; **mapped**, appearance independent |
| `HidePrimaryStartButton` | `-hps=`; Checkbox12 | Center 298-302 | C `hide_primary_start=false`, W capture/hide/restore; **mapped** |
| `HideSecondaryStartButton` | `-hss=`; Checkbox13 | Center 329-333 | C `hide_secondary_start=false`, W; **mapped** |
| `HidePrimaryNotifyWnd` | `-hpt=`; Checkbox14 | Center 304-310 alters TrayNotifyWnd visibility/style | C `hide_primary_tray=false`, W; **mapped**, preserve prior visibility/styles |
| `HideSecondaryNotifyWnd` | `-hst=`; Checkbox15 | Center 335-338 uses ClockButton | C `hide_secondary_tray=false`, W; **mapped**, missing requested controls fail explicitly without guessed writes |
| `ShowTrayIcon` | `-sti=`; **hidden** Checkbox16 | Main 131-134 forces zero; 210-237 contains unreachable-by-switch tray setup; 267-295 maps L restart/M config/R stop | C `tray_icon=false`, S queue/R lifecycle; **mapped**, new working implementation rather than claim upstream control worked |
| `TaskbarStyleOnMax` | `-tbsom=`; no UI | Main 135-137 assigns, no reader found | **Dormant upstream**, distinct from working `DefaultTaskbarStyleOnWinMax`; not exposed |
| `TaskbarStyleRed`, `TaskbarStyleGreen`, `TaskbarStyleBlue`, `TaskbarStyleAlpha` | `-color=R;G;B;A`; sliders/hex/picker | Style 187-188 multiplies percentage alpha by 2.55 | C `color:[u8;4]`, W accent encoding, R `--color R,G,B,A`, G; **mapped**; Rust alpha 0-255, not percentage |
| `ConsoleEnabled` | `-console=`; checkboxconsole emits `-console` | Main 47-50 allocates; GUI 1173-1175/2387-2389 mismatches parser; restart 1882-1884 erroneously uses tray checkbox | C `show_console=false`, R/G launch policy; **mapped**, do not reproduce parser/control bugs |
| `StickyStartButton` | `-stsb=`; no UI | Main 138-140 assigns, no reader found; Center 970-974 contains commented movement experiment | **Dormant upstream**; not equivalent to custom Start shortcut, no invented behavior |
| `TotalPrimaryOpacity` | `-tpop=`; tpop | Center 293-296; zero acts as unset in VB integer comparison | C `primary_opacity:null` or 0-100%, W; **mapped**; Rust zero is real transparent opacity |
| `TotalSecondaryOpacity` | `-tsop=`; tsop | Center 324-327 | C `secondary_opacity:null` or 0-100%, W; **mapped** |
| `RevertZeroBeyondTray` | `-rzbt=`; CheckBox4_Copy | Main default 1; [TaskbarAnimate.vb 19 onward][animate] consumes it | C `revert_on_tray_overlap=false`, R/K; **mapped**; preserve established Rust default, use owned original position instead of global reset |
| `TaskbarRounding` | `-tbr=`; tbrounding | Center 884-965 uses rounded regions | C `corner_radius=0`, W region ownership; **mapped** |
| `TaskbarSegments` | `-tbsg=`; tbsegments | Center 860-983; horizontal/vertical paths differ, several region combinations commented out | C `taskbar_segments=false`, W measured regions; **mapped**, unsupported/uncertain geometry skips safely |
| `UseUIA` | No switch; disabled provider radios | XAML 216-217 disables MSAA and UIAutomation; Center uses MSAA; declaration-only tree search | C `measurement="msaa"` or `"uia"`, W real provider; **mapped** is a new capability, not upstream parity evidence |
| `OnBatteryAnimationStyle` | `-obas=`; ComboBox2 | Main 28; Center 993-1040 | C `battery_animation=null`, R power policy/G; **mapped**, null inherits normal animation |
| `OnBatteryLoopRefreshRate` | `-oblr=`; NumericUpDown5 | Main 29; Center 519-523 | C `battery_poll_ms=null`, R/G; **mapped**, null inherits normal interval |
| No Settings field | `-stop` | Main 40-42, 201-205 resets/ends after killing other instances | R `stop`/lifecycle cleanup acknowledgment; **mapped**, no killing |
| No Settings field | `-showstartmenu` | Main 43-46 calls Win32.ShowStartMenu then exits | R `start-menu`/S explicit action; **mapped** |

## Complete configurator control/action inventory

Source: [MainWindow.xaml][xaml], codebehind [MainWindow.xaml.vb][gui]. Names below retain upstream spelling/casing for searchability. Display-only labels are not separate settings.

| Tab/control(s), XAML lines | Upstream action/source | Rust mapping / status |
| --- | --- | --- |
| Style: RadioButton1/2/3/4, RadioButtontc, RadioButtonoq (76-79, 96-97) | Default/transparent/blur/acrylic/transparent-gradient/opaque; GUI serialization 1053-1070 | C/G six style values; gradient addition **mapped** |
| Style: Checkbox10 (80) | Default on maximized window | `default_style_on_maximized`; **mapped** |
| Style: sRed/sGreen/sBlue/sAlpha; tRed/tGreen/tBlue/tAlpha; colorprev (81-94) | Sliders and numeric/percentage labels, preview update 2159-2203 | RGBA form/preview; byte-alpha difference explicit; UI preview **mapped** |
| Style: hexcolorbox (99) | RGB hex conversion 2206-2231, TextChanged 2400-2403; alpha separate | G hex RGB editing/validation **mapped**; not another setting |
| Style: eyedropper button, Button_Click_12 (95) | CopyFromScreen one pixel at cursor until click, GUI 204-211, 2233-2260 | Screen eyedropper **mapped**; never run in automated desktop validation |
| Style: tpop/tsop, tpopla/tsopla (100-106) | Total opacity with labels, GUI 2405-2412; GUI defaults 100 | C/G optional opacity percentages; **mapped**, null preserves previous state |
| Style: tbrounding/tbsegments (107-110) | Radius/segments | C/G/W; **mapped** |
| Animation: ComboBox1/ComboBox2, NumericUpDown4 (120-126) | Normal/battery easing and duration; both lists populated GUI 267-355 | Normal mapped; nullable battery **mapped** |
| Animation: disabled Slider1, Preview, next/previous buttons (127-131) | Visualization only; GUI 1967-2156; next/previous adjusts selected easing | K/G pure preview and selection helpers **mapped**, never move actual taskbar to preview |
| Position: NumericUpDown1/2 (142-147) | Independent signed offsets | C/G/R/K **mapped** |
| Position: CheckBox1/CheckBox11 (148-149) | Free-span compensation / don't center | C/G/R/K **mapped** |
| Position: NumericUpDown7, NumericUpDown7_Copy, NumericUpDown7_Copy1 (150-157) | Three width exceptions | C/G `skip_resolutions`; **mapped**, list replaces fixed three slots |
| Position: CheckBox4_Copy (158) | Revert beyond tray | C/G/R policy **mapped** |
| Startup: NumericUpDown6 (168-170) | Delay preference default six seconds | C/G `startup_delay_secs=6`; **mapped**, save never registers |
| Startup: Remove/Create, Button_Click_3/33 (171-172) | Delete/register upstream named tasks; GUI 1269-1470 | R/S startup disable/enable, separate status; **mapped**, rewrite-owned task only |
| Startup: TextboxLink + Generate (175-178) | Compose legacy engine invocation string, GUI 2262-2398 | S `LaunchSpec` safe argument serialization; registered startup invocation is displayed in status; Start Button page generates a Start-menu command, not a raw unsaved engine-argument string **mapped**; do not reinterpret user data as shell code |
| Extra: CheckBox2/3 (188-189) | Primary-only/secondary-only | C/G/R selection **mapped** |
| Extra: CheckBox4, Checkbox9 (190-192) | Toolbar compensation, fullscreen pause | C/G/R/W **mapped** |
| Extra: Checkbox12/13/14/15 (193-197) | Start/tray visibility by taskbar category | C/G/W **mapped** |
| Extra: Checkbox16 (196) | Hidden tray preference; engine forcibly disables | C/G/S `tray_icon` **mapped**, explicit newly working feature |
| Mechanics: NumericUpDown3/5 (208-213) | Normal/battery polling | Normal mapped; nullable battery **mapped** |
| Mechanics: disabled MSAA/UIAutomation radios, question-mark button (216-218) | Radios have no functional selection; help opens Microsoft's accessibility comparison (GUI 2415-2417) | C/G provider selection and W alternate provider **mapped**; provider help is documented here rather than a question-mark browser-launch button |
| Start Button: startbutton_shortcut and six instruction steps (229-238) | String generated GUI 361-368; user creates shortcut/icon/pins manually | R/S owned `.lnk` create/remove + `start-menu`; optional icon; **mapped**. Pinning remains manual OS operation |
| About: versiontb; author/site/GitHub/issues/email/Reddit/reference boxes; vers (247-265) | Read-only provenance and version text | README/notices attribution **mapped**; independent Rust version/about presentation **mapped**; external provenance links live in README/notices rather than duplicate social-network UI controls, no upstream version masquerading |
| About: Check (bb), Button_Click_4 (262) | Fetch upstream VERSION at load 376-408 and explicit check 1482-1509; Store hides it | **Excluded as Rust updater**. Upstream releases do not describe Rust builds; no automatic upstream network version probe |
| Uninstall: Button_Click_8 (274-276) | Confirms, deletes upstream tasks, kills engine; Store Remove-AppxPackage or file-removal guidance, GUI 1899-1965 | **Deliberate replacement**: explicit owned startup/shortcut removal and graceful stop, then user removes portable files. No Store/unrelated task deletion; combined GUI workflow **mapped** |
| Apply, Button_Click_1 (282) | Kills, resets, runs, recreates startup task (GUI 1028-1232) | G Apply uses import -> graceful stop -> run; **mapped** baseline sequence, shell independence **mapped**. Never creates startup |
| Close, Button_Click (283) | Closes only configurator (GUI 1024-1026) | G closes editor only; **mapped** |
| Stop TaskbarX, Button_Click_2 (284) | Kills then launches reset (GUI 1238-1260) | G/R graceful stop with error/timeout; **mapped**, no force-kill fallback |
| Reset, Button_Click_6 (285) | Local defaults until Apply, GUI 1703-1737 | G local Reset/shared defaults **mapped**; extended defaults **mapped** |
| Reload, Button_Click_5 (286) | Read settings from scheduled task, GUI 1511-1701 | G config show reload **mapped**; not from upstream task |
| Restart TaskbarX, Button_Click_7 (287) | Kill/reset/relaunch with current form, GUI 1739-1897 | G/R graceful restart **mapped**; failed stop blocks launch and startup remains untouched |
| Show Console: checkboxconsole (288) | Debug preference; upstream parser/restart bugs above | C/G/R `show_console` **mapped**, explicit launch behavior |
| ListBox1, TabControl1, animation_Tab (20-67) | Navigation, GUI 1015-1022; Windows 11 tab removal experiment commented at 621-626 | G navigation/layout choice; no requirement to copy fixed WPF geometry or disabled Win11 workaround |
| placeholder, grid.png, emoji/TextBlock labels, preview marker (18, 74, 129 and throughout) | Decorative shell, checkerboard, headings/help labels | Independent native UI; upstream images/icons/fonts **excluded** without separate provenance |

## Lifecycle and safety divergences

| Upstream source | Observed behavior | Rust decision / evidence destination |
| --- | --- | --- |
| Main 155-163; GUI Apply/Stop/Restart/Uninstall | Kill processes named TaskbarX | **Excluded**: R instance coordination, cleanup acknowledgment and no unrelated process interference |
| Center 156-208 | Global zero reset, show controls, hard-coded styles, kill AcrylicPanel | **Excluded** as global reset: W restores only successful owned changes and prior positions/visibility/regions/styles; failed restoration stays reportable |
| Main 357-398 | Theme messages and rectangular region reset | W independent cleanup, preserve capturable prior region; third-party accent may only get Windows-default fallback |
| Main 400-404 | RestartExplorer kills Explorer | **Excluded**; no automatic Explorer restart |
| Center 214-262, 527-537 | Preference/display/session or lost-handle restart | R/W rediscovery and identity checks; live recovery **not tested** |
| Main 194-198 | Detects modern Win11 bridge and disables centering | **Unsupported** native Windows 11, not a supported style-only mode |
| GUI 603-1013 | Detect/migrate/delete old TaskbarX tasks automatically | **Excluded**: no upstream task discovery/migration/removal side effect |
| Main 210-237, 267-295 | Tray icon, mouse actions, Store configurator launch | S queue + R explicit actions; owned icon only; no Store activation |
| TrayIconBuster 37-115, 179-316 | Enumerate toolbar buttons, allocate/read remote process memory, delete phantom entries | **Excluded** entirely. Native owned notification-icon cleanup is not phantom cleanup |
| Main 406-410 | Force GC / working-set trim | Not a user-facing parity requirement; Rust resource ownership, no ritual memory trim |
| GUI Store branches (181-198, 361-368, 1207-1223, 1933-1948) | Store URI/app activation and uninstall | **Excluded**, native portable Windows 10 only |

## Coverage summary

All active feature mappings above have source implementations in the six-package workspace. This is source/automated-test coverage, not a claim of live visual equivalence. The disabled upstream features and safety replacements remain explicitly distinguished.

Additional regression coverage in `state_tests.rs` and `src/runtime/tests.rs` verifies resolution-skip restoration after task-list replacement: positively obsolete positions retire without writes, surviving positions restore, appearance remains active, and uncertain identity queries remain retryable errors. The integrated lockfile includes all six packages.

Historical locked test/lint/build results are recorded in [VALIDATION](VALIDATION.md), with architecture and packaging evidence in [BUILD-v0.1.1](BUILD-v0.1.1.md). A separate historical startup check verified Enable/update and status, not actual sign-in launch. Live taskbar effects/restoration, shortcuts/pinning, tray and Rainmeter interaction remain unestablished. None of this evidence validates v0.1.2.
[settings]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX/TaskbarX/Settings.vb
[main]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX/TaskbarX/Main.vb
[style]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX/TaskbarX/TaskbarStyle.vb
[center]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX/TaskbarX/TaskbarCenter.vb
[buster]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX/TaskbarX/TrayIconBuster.vb
[animate]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX/TaskbarX/TaskbarAnimate.vb
[xaml]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX%20Configurator/TaskbarX%20Configurator/MainWindow.xaml
[gui]: https://github.com/ChrisAnd1998/TaskbarX/blob/master/TaskbarX%20Configurator/TaskbarX%20Configurator/MainWindow.xaml.vb
