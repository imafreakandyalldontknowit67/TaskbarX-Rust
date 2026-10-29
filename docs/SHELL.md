# Native shell integration

## Source status and integration boundary

The shell crate's public APIs, native adapters and fake tests listed here are present in source. **This is source mapping, not a claim that the final gate or live shell tests passed.** CLI/runtime/configurator wiring remains subject to the integration owner's final reconciliation in [PARITY](PARITY.md) and [VALIDATION](VALIDATION.md). Loading the crate has no side effects.

| API | Source and non-mutating tests |
| --- | --- |
| `LaunchSpec { executable, args, working_directory }`, `StartupStatus { registered, enabled, owned, task_name, launch, delay_secs, warnings }`, `format_command_line` | [lib.rs](../crates/taskbarx-shell/src/lib.rs): `absolute_unicode_and_nul_validation`, Windows `rejects_unpaired_surrogates_without_lossy_conversion` |
| Argument quoting/parsing | [command_line.rs](../crates/taskbarx-shell/src/command_line.rs): `quoting_preserves_argument_boundaries`, `exhaustive_small_argument_round_trips` |
| `startup_status`, `enable_startup`, `disable_startup` | [startup.rs](../crates/taskbarx-shell/src/startup.rs), [COM adapter](../crates/taskbarx-shell/src/startup_windows.rs); fake scheduler tests below |
| `create_start_shortcut`, `remove_start_shortcut` | [shortcut.rs](../crates/taskbarx-shell/src/shortcut.rs), [Shell Link adapter](../crates/taskbarx-shell/src/shortcut_windows.rs); fake link tests below |
| `show_start_menu` | [start_menu.rs](../crates/taskbarx-shell/src/start_menu.rs): `start_menu_uses_only_injected_adapter_and_propagates_failure` |
| `TrayIcon::new/poll_actions/close`, `TrayAction::{OpenSettings,Restart,Stop}` | [tray.rs](../crates/taskbarx-shell/src/tray.rs), [native tray](../crates/taskbarx-shell/src/tray_windows.rs); fake lifecycle tests below |
| Native effects/COM/token/operation-lock guards | [native.rs](../crates/taskbarx-shell/src/native.rs); native effect constructors are disabled in unit-test builds |

## Launch and path limitations

- Paths must be **absolute, valid Unicode and NUL-free**. Invalid UTF-16/unpaired surrogates are errors, not lossy replacement characters.
- Individual values and the rendered command line are bounded to 32766 UTF-16 units. Separate executable, argument vector and working directory preserve argument boundaries.
- `format_command_line` produces a Windows argv display string. It is **not safe shell-language source** to interpolate into cmd.exe or PowerShell.
- Task Scheduler XML rejects XML 1.0-invalid controls. XML escaping and numeric whitespace references preserve valid data through normalization. Possible `%...%` expansion in executable, working directory or combined arguments is rejected rather than silently expanded; the conservative guard rejects two or more percent characters in a field.
- Native Shell Link target, working-directory and optional-icon paths of **260 UTF-16 units or longer** are explicitly unsupported. These fields also reject percent characters to avoid environment expansion. No silent truncation is accepted.
- Shortcut destination must be an explicit absolute `.lnk` path whose parent exists. No default folder creation, Start-folder installation, execution or pinning occurs. Non-regular/reparse-point destinations and unrelated/modified links are refused.
- A supplied icon's existence, usable format, availability after moving files and redistribution rights are the user's responsibility. The API does not promise the shell will render it as expected.

## Startup ownership and failure semantics

The owned task is named `TaskbarX-Rust.Startup.v1.<current-user-SID>`. Its marker includes that SID. Mutation requires matching marker, principal SID, no group principal, interactive-token logon, least privilege and the current user's logon trigger. Foreign/conflicting tasks are not replaced or deleted; upstream TaskbarX tasks are not migrated.

The definition has one current-user logon trigger with delay **0..86400 seconds**, a separate executable/arguments/working-directory action, and battery-friendly start/continue settings. It does not demand elevation, credentials, idle/network availability or an immediate run. Registration triggers are suppressed; enable never calls Run/RunEx. Save/Apply/run/preferences must never call enable implicitly.

Status is read-only: it does not create scheduler folders/tasks, repair registrations or run an action. Only a genuine file-not-found result is absent. Access/COM failures report **inaccessible/unknown**, not `registered:false`. A disabled task **or disabled logon trigger** yields effective `enabled:false` and warnings. Stored malformed actions/delays are reported, not invented.

Before update, the full desired XML is schema/semantically validated and its canonical serialization retained. The prior record is re-read before mutation. Updates and creates use distinct modes, so a newly occupied name is not blindly overwritten. After writing, ownership/enabled state, launch and delay are verified.

On failed update, rollback restores prior XML only if the current record is still owned and its **complete XML exactly matches the validated prepared definition**. The prior record must be re-read and verified after rollback before claiming restoration. External edits, absent/inaccessible state, rollback errors or unverifiable restoration are honest failures. A failed new creation asks the user to inspect status rather than guessing cleanup. These checks minimize interference; they are not an atomic guarantee against every external writer.

Fake scheduler tests in `startup.rs`:

- `status_is_read_only_absent_disabled_conflicting_and_inaccessible`
- `sid_principal_and_marker_are_required_for_mutations`
- `delayed_logon_has_separate_fields_xml_escaping_and_no_implicit_run`
- `invalid_input_and_validation_failure_cannot_replace_task`
- `failed_update_restores_prior_definition_and_reports_failed_rollback`
- `failed_readback_restores_but_never_overclaims_rollback_or_clobbers_changed_xml`

`scheduler_duration_normalization` in the native adapter tests parsing only, not live registration.

## Owned Start shortcuts and Start-menu action

An owned shortcut must target exactly the `start-menu` argument. Its description marker binds the current user's SID to the executable/action/working-directory fingerprint. This detects accidental changes; it is **not authentication against another process running as the same user**.

Creation stages a same-directory temporary link, reads back the stored properties, rechecks the destination, then publishes atomically. New-link publication does not replace an existing collision. Replacement/removal is limited to a recognized owned link and rechecks for changed contents. Failed stage/publish paths clean their own temporary file and preserve unrelated destinations.

`show_start_menu` verifies classic shell controls and sends `WM_SYSCOMMAND/SC_TASKLIST` with a bounded 2000 ms timeout. Success confirms message delivery, **not visible Start-menu state**. It does not inject keys, launch a process, start the engine, reposition taskbars or unhide Start controls. Native modern Windows 11 is rejected. Pinning remains a manual OS shell action.

Fake link tests:

- `target_icon_working_directory_and_idempotency`
- `collision_changed_target_or_wrong_action_is_never_removed`
- `temporary_cleanup_on_com_and_publish_failure_preserves_old_link`
- `reject_non_start_or_relative_paths_before_effects`

Fixture files are plain fake-link data in temporary directories, not real live shortcuts.

## Tray ownership and queued actions

`TrayIcon` is owner-thread-only (`!Send`, `!Sync`). Callbacks enqueue intentions; the runtime decides when to open settings, restart or stop. Polling dispatches at most 64 messages for the icon's own window, not global/thread messages or another window's queue. Repeated actions are coalesced to at most three distinct actions; flooding does not displace Stop. Explorer notification-icon recovery is rate-limited.

The native tray uses Windows' shared **`IDI_APPLICATION`** icon and owns a `CopyIcon` copy. No upstream or runtime icon asset is needed. Cleanup is idempotent and independently tracks owned notification/menu/window/icon/class resources. Failed releases retain ownership for bounded retry; Drop is best effort, not guaranteed cleanup after hard termination. No remote Explorer memory access or phantom-icon cleanup is performed.

Fake tray tests include:

- `callbacks_only_enqueue_bounded_deduplicated_intentions`
- `duplicate_context_requests_show_one_menu`
- `menu_dismiss_restores_keyboard_focus_without_starving_actions`
- `explorer_recovery_and_duplicate_broadcasts_do_not_duplicate_icons`
- `failed_recovery_is_rate_limited_and_stop_is_not_starved`
- `close_retries_keeps_failed_ownership_and_is_idempotent`
- `all_cleanup_failures_are_retryable_and_independent`
- `every_partial_creation_path_cleans_owned_resources`
- `drop_releases_all_owned_resources`
- `terminal_cleanup_releases_local_resources_if_explorer_is_absent`
- `native_constructor_is_disabled_in_unit_tests`

Native-file tests `semantic_mouse_and_keyboard_activation_only` and `context_coordinates_preserve_signed_multi_monitor_positions` test interpretation, not actual mouse/keyboard interaction.

`native_apis_explicitly_reject_unsupported_platforms` is cfg-gated to non-Windows;
it is present in source but was not executed by the Windows-only harness.

API references used for the adapters: Microsoft's
[Task Scheduler registration contract](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nf-taskschd-itaskfolder-registertaskdefinition),
[ordered logon-trigger schema](https://learn.microsoft.com/en-us/windows/win32/taskschd/taskschedulerschema-logontriggertype-complextype),
[Shell Link GetPath flags](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ishelllinkw-getpath),
and [SC_TASKLIST system command](https://learn.microsoft.com/en-us/windows/win32/menurc/wm-syscommand).

**Live startup, links, Start-menu, tray and visual parity remain NOT EXECUTED by the automated release procedure.**
