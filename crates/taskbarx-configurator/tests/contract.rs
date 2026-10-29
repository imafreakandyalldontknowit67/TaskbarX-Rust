use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    ffi::OsString,
    path::PathBuf,
    rc::Rc,
};
use taskbarx_configurator::{
    controller::{Action, Controller},
    process::{Process, ProcessReply, SystemProcess},
    protocol::{Arguments, CommandSpec, Mode, Operation, Options},
    settings::{Form, Settings, ANIMATIONS, STYLES},
};

#[derive(Default)]
struct Fake {
    calls: Rc<RefCell<Vec<CommandSpec>>>,
    replies: VecDeque<Result<ProcessReply, String>>,
}
impl Process for Fake {
    fn execute(&mut self, spec: &CommandSpec) -> Result<ProcessReply, String> {
        self.calls.borrow_mut().push(spec.clone());
        self.replies.pop_front().expect("unexpected engine command")
    }
}
fn ok(stdout: impl Into<Vec<u8>>) -> Result<ProcessReply, String> {
    Ok(ProcessReply {
        code: Some(0),
        stdout: stdout.into(),
        ..Default::default()
    })
}
fn failed() -> Result<ProcessReply, String> {
    Ok(ProcessReply {
        code: Some(23),
        stdout: b"partial output".to_vec(),
        stderr: b"permission denied".to_vec(),
        ..Default::default()
    })
}
fn timed_out() -> Result<ProcessReply, String> {
    Ok(ProcessReply {
        timed_out: true,
        stderr: b"restoration still pending".to_vec(),
        ..Default::default()
    })
}
fn running() -> Result<ProcessReply, String> {
    Ok(ProcessReply {
        pid: Some(42),
        ..Default::default()
    })
}
fn encode(s: &Settings) -> Vec<u8> {
    taskbarx_config::to_json(s).unwrap().into_bytes()
}
fn decode(bytes: &[u8]) -> Result<Settings, String> {
    taskbarx_config::from_json(std::str::from_utf8(bytes).map_err(|e| e.to_string())?)
}
fn options() -> Options {
    Options {
        engine: std::env::temp_dir().join("Taskbar X \u{6d4b}\u{8bd5} & stuff/taskbarx.exe"),
        config: Some(std::env::temp_dir().join("config folder/a & b.json")),
    }
}
fn controller(replies: Vec<Result<ProcessReply, String>>) -> Controller<Fake> {
    Controller::new(
        options(),
        Fake {
            replies: replies.into(),
            ..Default::default()
        },
    )
}
fn operations(c: &Controller<Fake>) -> Vec<String> {
    c.process
        .calls
        .borrow()
        .iter()
        .map(CommandSpec::label)
        .collect()
}

#[test]
fn fixed_schema_defaults_and_json_roundtrip() {
    let s = Settings::default();
    let json = encode(&s);
    assert_eq!(decode(&json).unwrap(), s);
    assert_eq!(Form::from(s.clone()).settings().unwrap(), s);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({
            "schema_version":1,"center":true,"primary":true,"secondary":true,"center_between":false,
            "primary_offset":0,"secondary_offset":0,"animation":"cubiceaseinout","animation_ms":300,
            "poll_ms":400,"style":"default","color":[0,0,0,128],
        "primary_opacity": null, "secondary_opacity": null, "corner_radius": 0,
        "taskbar_segments": false, "default_style_on_maximized": false,
        "battery_animation": null, "battery_poll_ms": null, "skip_resolutions": [],
        "pause_on_fullscreen": false, "hide_primary_start": false, "hide_secondary_start": false,
        "hide_primary_tray": false, "hide_secondary_tray": false, "revert_on_tray_overlap": false,
        "toolbar_compensation": true, "tray_icon": false, "startup_delay_secs": 6,
        "measurement": "msaa", "show_console": false
        })
    );
}

#[test]
fn every_upstream_easing_and_style_roundtrips() {
    let mut expected = HashSet::from(["none".to_string(), "linear".to_string()]);
    for family in [
        "quad", "cubic", "quart", "quint", "sine", "expo", "circ", "back", "elastic", "bounce",
    ] {
        for suffix in ["easein", "easeout", "easeinout", "easeoutin"] {
            expected.insert(format!("{family}{suffix}"));
        }
    }
    assert_eq!(
        expected,
        ANIMATIONS.iter().copied().map(str::to_owned).collect()
    );
    assert_eq!(ANIMATIONS.len(), 42);
    assert_eq!(
        STYLES,
        [
            "default",
            "transparent",
            "blur",
            "acrylic",
            "opaque",
            "transparent-gradient"
        ]
    );
    for &animation in ANIMATIONS {
        for &style in STYLES {
            let form = Form {
                animation: animation.into(),
                style: style.into(),
                primary_offset: "-10000".into(),
                secondary_offset: "10000".into(),
                animation_ms: "0".into(),
                poll_ms: "50".into(),
                color: ["0".into(), "255".into(), "126".into(), "255".into()],
                ..Form::default()
            };
            let s = form.settings().unwrap();
            assert_eq!(decode(&encode(&s)).unwrap(), s);
            assert_eq!(Form::from(s), form);
        }
    }
}

#[test]
fn selections_are_independent_and_zero_duration_is_valid() {
    for center in [false, true] {
        for primary in [false, true] {
            for secondary in [false, true] {
                for between in [false, true] {
                    let s = Settings {
                        center,
                        primary,
                        secondary,
                        center_between: between,
                        animation_ms: 0,
                        ..Settings::default()
                    };
                    assert_eq!(Form::from(s.clone()).settings().unwrap(), s);
                }
            }
        }
    }
}

#[test]
fn form_errors_are_field_specific_and_never_clamped() {
    for (field, bad) in [
        ("primary_offset", "-10001"),
        ("primary_offset", "10001"),
        ("primary_offset", "-"),
        ("secondary_offset", "2147483648"),
        ("secondary_offset", "0.5"),
        ("animation_ms", "-1"),
        ("animation_ms", "10001"),
        ("poll_ms", "49"),
        ("poll_ms", "10001"),
        ("poll_ms", ""),
        ("animation", "CubicEaseInOut"),
        ("style", "gradient"),
        ("red", "-1"),
        ("green", "256"),
        ("blue", "1.2"),
        ("alpha", ""),
    ] {
        let mut f = Form::default();
        *match field {
            "primary_offset" => &mut f.primary_offset,
            "secondary_offset" => &mut f.secondary_offset,
            "animation_ms" => &mut f.animation_ms,
            "poll_ms" => &mut f.poll_ms,
            "animation" => &mut f.animation,
            "style" => &mut f.style,
            "red" => &mut f.color[0],
            "green" => &mut f.color[1],
            "blue" => &mut f.color[2],
            "alpha" => &mut f.color[3],
            _ => unreachable!(),
        } = bad.into();
        assert_eq!(f.settings().unwrap_err().field, field);
    }
    let f = Form {
        primary_offset: " -12 ".into(),
        secondary_offset: "-10000".into(),
        animation_ms: "10000".into(),
        poll_ms: "10000".into(),
        ..Form::default()
    };
    assert_eq!(f.settings().unwrap().primary_offset, -12);
}

#[test]
fn malformed_unknown_or_newer_json_never_defaults_but_partial_objects_do() {
    for json in [
        b"{".as_slice(),
        b"[]",
        b"null",
        b"warning\n{}",
        b"{} {}",
        b"\xff",
    ] {
        assert!(decode(json).is_err());
    }
    let good = serde_json::to_value(Settings::default()).unwrap();
    for (field, value) in [
        ("schema_version", serde_json::json!(2)),
        ("schema_version", serde_json::json!(-1)),
        ("primary_offset", serde_json::json!(-10001)),
        ("animation_ms", serde_json::json!(10001)),
        ("poll_ms", serde_json::json!(0)),
        ("color", serde_json::json!([0, 0, 0, 256])),
        ("color", serde_json::json!([0, 0, 0])),
        ("center", serde_json::json!(1)),
        ("style", serde_json::json!("unknown")),
        ("animation", serde_json::json!("unknown")),
        ("extra", serde_json::json!(1)),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        assert!(
            decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{field}"
        );
    }
    for key in good.as_object().unwrap().keys() {
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(
            decode(&serde_json::to_vec(&bad).unwrap()).is_ok(),
            "missing {key}"
        );
    }
    let duplicate = String::from_utf8(encode(&Settings::default()))
        .unwrap()
        .replacen('{', "{\"center\":false,", 1);
    assert!(decode(duplicate.as_bytes()).is_err());
}

#[test]
fn argument_parser_uses_executable_directory_not_cwd_or_path() {
    let exe = std::env::temp_dir().join("installed package/taskbarx-configurator.exe");
    let cwd = std::env::temp_dir().join("unrelated folder");
    let Arguments::Options(default) = Options::parse([], &exe, &cwd).unwrap() else {
        panic!()
    };
    assert_eq!(default.engine, exe.with_file_name("taskbarx.exe"));
    assert!(default.config.is_none());
    let Arguments::Options(explicit) = Options::parse(
        [
            "--engine",
            "engine dir/taskbarx.exe",
            "--config",
            "config dir/\u{989c}\u{8272} & x.json",
        ]
        .map(OsString::from),
        &exe,
        &cwd,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(explicit.engine, cwd.join("engine dir/taskbarx.exe"));
    assert_eq!(
        explicit.config,
        Some(cwd.join("config dir/\u{989c}\u{8272} & x.json"))
    );
    for args in [
        vec!["--engine"],
        vec!["--config", ""],
        vec!["--engine", "--config"],
        vec!["--engine", "x", "--engine", "y"],
        vec!["--unknown"],
        vec!["bare"],
    ] {
        assert!(Options::parse(args.into_iter().map(OsString::from), &exe, &cwd).is_err());
    }
    assert_eq!(
        Options::parse(["--help".into()], &exe, &cwd).unwrap(),
        Arguments::Help
    );
}

#[test]
fn all_command_arrays_and_stdin_are_exact_including_spaces() {
    let options = options();
    let json = encode(&Settings::default());
    for (op, expected, mode) in [
        (Operation::Show, vec!["config", "show"], Mode::Wait),
        (
            Operation::Import(json.clone()),
            vec!["config", "import", "--stdin"],
            Mode::Wait,
        ),
        (Operation::Run, vec!["run"], Mode::Launch),
        (
            Operation::Stop,
            vec!["stop", "--timeout-ms", "5000"],
            Mode::Wait,
        ),
        (Operation::Diagnose, vec!["diagnose", "--json"], Mode::Wait),
    ] {
        let spec = options.command(op);
        let mut args: Vec<OsString> = expected.iter().map(OsString::from).collect();
        if expected[0] != "stop" {
            args.push("--config".into());
            args.push(options.config.as_ref().unwrap().as_os_str().into());
        }
        assert_eq!(spec.program, options.engine);
        assert_eq!(spec.args, args);
        assert_eq!(spec.mode, mode);
        if expected.contains(&"--stdin") {
            assert_eq!(spec.stdin, Some(json.clone()));
        } else {
            assert!(spec.stdin.is_none());
        }
        assert!(!spec.timeout.is_zero());
    }
    let no_config = Options {
        config: None,
        ..options
    };
    assert_eq!(
        no_config.command(Operation::Show).args,
        vec![OsString::from("config"), OsString::from("show")]
    );
}

#[test]
fn initialize_edit_save_reset_reload_and_drop_never_run_or_stop() {
    let mut c = controller(vec![
        ok(encode(&Settings::default())),
        ok("saved"),
        ok(encode(&Settings::default())),
    ]);
    assert!(c.form.is_none());
    assert!(operations(&c).is_empty());
    c.perform(Action::Initialize).unwrap();
    let mut form = c.form.clone().unwrap();
    form.primary_offset = "-321".into();
    c.perform(Action::Edit(form.clone())).unwrap();
    assert_eq!(c.form.as_ref().unwrap().primary_offset, "-321");
    c.perform(Action::Save(form)).unwrap();
    c.perform(Action::Reset).unwrap();
    assert_eq!(c.form, Some(Form::default()));
    c.perform(Action::Reload).unwrap();
    assert_eq!(
        operations(&c),
        ["config show", "config import --stdin", "config show"]
    );
    let calls = c.process.calls.clone();
    drop(c);
    assert!(calls
        .borrow()
        .iter()
        .all(|s| s.args[0] != "stop" && s.args[0] != "run"));
    let imported = calls.borrow()[1].stdin.clone().unwrap();
    assert_eq!(decode(&imported).unwrap().primary_offset, -321);
}

#[test]
fn invalid_load_preserves_form_and_explicit_reset_is_only_local() {
    let mut c = controller(vec![ok("{corrupt"), failed(), ok("[]")]);
    let e = c.perform(Action::Initialize).unwrap_err();
    assert!(e.contains("No defaults"));
    assert!(c.form.is_none());
    c.perform(Action::Reset).unwrap();
    c.perform(Action::Edit(Form {
        primary_offset: "-72".into(),
        ..Form::default()
    }))
    .unwrap();
    assert!(c.perform(Action::Reload).is_err());
    assert!(c.perform(Action::Reload).is_err());
    assert_eq!(c.form.unwrap().primary_offset, "-72");
    assert!(c.process.calls.borrow().iter().all(|s| s.args[1] == "show"));
}

#[test]
fn apply_has_save_stop_wait_launch_order_and_no_other_commands() {
    let mut c = controller(vec![ok("save output"), ok("restored"), running()]);
    let result = c.perform(Action::Apply(Form::default())).unwrap();
    assert!(result.message.contains("NOT a support/health confirmation"));
    assert!(result.message.contains("save output"));
    assert!(result.message.contains("restored"));
    assert_eq!(
        operations(&c),
        ["config import --stdin", "stop --timeout-ms 5000", "run"]
    );
}

#[test]
fn failed_validation_cannot_save_stop_or_run() {
    for apply in [false, true] {
        let mut c = controller(vec![]);
        let f = Form {
            poll_ms: "49".into(),
            ..Form::default()
        };
        assert!(c
            .perform(if apply {
                Action::Apply(f)
            } else {
                Action::Save(f)
            })
            .is_err());
        assert!(operations(&c).is_empty());
    }
}

#[test]
fn every_failure_stage_stops_the_apply_sequence() {
    for failure in [failed(), timed_out(), Err("spawn: access denied".into())] {
        let mut c = controller(vec![failure.clone()]);
        assert!(c.perform(Action::Apply(Form::default())).is_err());
        assert_eq!(operations(&c), ["config import --stdin"]);
        let mut c = controller(vec![ok("persisted"), failure.clone()]);
        let err = c.perform(Action::Apply(Form::default())).unwrap_err();
        assert!(err.contains("No new runner"));
        assert!(err.contains("persisted"));
        assert_eq!(
            operations(&c),
            ["config import --stdin", "stop --timeout-ms 5000"]
        );
        let mut c = controller(vec![ok(""), ok(""), failure]);
        assert!(c.perform(Action::Apply(Form::default())).is_err());
        assert_eq!(operations(&c).len(), 3);
    }
    // Even exit 0 is NOT proof a foreground runtime started.
    let mut c = controller(vec![ok(""), ok(""), ok("unsupported taskbar")]);
    assert!(c
        .perform(Action::Apply(Form::default()))
        .unwrap_err()
        .contains("unsupported taskbar"));
}

#[test]
fn stop_only_stops_and_never_validates_or_imports_form() {
    let mut c = controller(vec![ok("restored"), timed_out()]);
    c.form = Some(Form {
        poll_ms: "garbage".into(),
        ..Form::default()
    });
    assert!(c
        .perform(Action::Stop)
        .unwrap()
        .message
        .contains("restored"));
    assert!(c.perform(Action::Stop).unwrap_err().contains("timed out"));
    assert_eq!(
        operations(&c),
        ["stop --timeout-ms 5000", "stop --timeout-ms 5000"]
    );
}

#[test]
fn stdout_stderr_exit_and_timeout_are_visible() {
    let mut c = controller(vec![failed(), timed_out(), Err("engine missing".into())]);
    let err = c.perform(Action::Reload).unwrap_err();
    for text in ["23", "partial output", "permission denied"] {
        assert!(err.contains(text));
    }
    assert!(c
        .perform(Action::Reload)
        .unwrap_err()
        .contains("restoration still pending"));
    assert!(c
        .perform(Action::Reload)
        .unwrap_err()
        .contains("engine missing"));
    let mut reply = ok(encode(&Settings::default())).unwrap();
    reply.stderr = b"warning: fallback unsupported".to_vec();
    let mut c = controller(vec![Ok(reply.clone()), Ok(reply)]);
    assert!(c
        .perform(Action::Initialize)
        .unwrap()
        .message
        .contains("warning: fallback unsupported"));
    assert!(c
        .perform(Action::Save(Form::default()))
        .unwrap()
        .message
        .contains("warning: fallback unsupported"));
    let mut c = controller(vec![ok(b"\xff".to_vec())]);
    assert!(c.perform(Action::Reload).is_err());
}

#[test]
fn diagnostics_are_read_only_and_unsupported_is_never_translated_into_success() {
    let mut unsupported = ok(r#"{"supported":false,"reason":"native Windows 11"}"#).unwrap();
    unsupported.stderr = b"not supported".to_vec();
    let mut c = controller(vec![Ok(unsupported.clone())]);
    let text = c.perform(Action::Diagnostics).unwrap().message;
    assert!(text.contains("\"supported\":false"));
    assert!(text.contains("not supported"));
    assert!(text.contains("NOT a configurator support verdict"));
    assert!(c.form.is_none());
    assert_eq!(operations(&c), ["diagnose --json"]);
    unsupported.code = Some(2);
    let mut c = controller(vec![Ok(unsupported), ok("[]"), ok("{} {}"), timed_out()]);
    assert!(c
        .perform(Action::Diagnostics)
        .unwrap_err()
        .contains("native Windows 11"));
    assert!(c
        .perform(Action::Diagnostics)
        .unwrap_err()
        .contains("protocol error"));
    assert!(c
        .perform(Action::Diagnostics)
        .unwrap_err()
        .contains("protocol error"));
    assert!(c
        .perform(Action::Diagnostics)
        .unwrap_err()
        .contains("timed out: true"));
}

#[test]
fn missing_engine_is_a_handled_error_without_a_placeholder_backend() {
    let mut process = SystemProcess::default();
    let mut spec = options().command(Operation::Show);
    spec.program = std::env::temp_dir().join(format!(
        "absent-taskbarx-configurator-test-{}.exe",
        std::process::id()
    ));
    assert!(process
        .execute(&spec)
        .unwrap_err()
        .contains("Engine missing"));
    spec.program = PathBuf::from("taskbarx.exe");
    assert!(process.execute(&spec).unwrap_err().contains("PATH lookup"));
}

fn startup(registered: bool) -> taskbarx_shell::StartupStatus {
    taskbarx_shell::StartupStatus {
        registered,
        enabled: registered,
        owned: registered,
        task_name: "TaskbarX Rust test task".into(),
        launch: registered.then(|| taskbarx_shell::LaunchSpec {
            executable: options().engine,
            args: vec![
                "run".into(),
                "--config".into(),
                options().config.unwrap().to_str().unwrap().into(),
            ],
            working_directory: std::env::temp_dir().join("Taskbar X \u{6d4b}\u{8bd5} & stuff"),
        }),
        delay_secs: registered.then_some(19),
        warnings: vec!["fixture warning".into()],
    }
}

#[test]
fn new_argv_contract_is_exact_and_config_is_strictly_allowlisted() {
    let opts = options();
    let path = std::env::temp_dir().join("Start \u{989c}\u{8272} & name.lnk");
    let icon = std::env::temp_dir().join("icon dir/\u{989c}\u{8272}.ico");
    let cases = [
        (
            Operation::StartupStatus,
            vec!["startup".into(), "status".into(), "--json".into()],
        ),
        (
            Operation::StartupDisable,
            vec!["startup".into(), "disable".into(), "--json".into()],
        ),
        (
            Operation::StartupEnable { delay_secs: 86400 },
            vec![
                "startup".into(),
                "enable".into(),
                "--delay-secs".into(),
                "86400".into(),
                "--json".into(),
                "--config".into(),
                opts.config.clone().unwrap().into_os_string(),
            ],
        ),
        (Operation::OpenStart, vec!["start-menu".into()]),
        (
            Operation::ShortcutCreate {
                path: path.clone(),
                icon: Some(icon.clone()),
            },
            vec![
                "shortcut".into(),
                "create".into(),
                "--path".into(),
                path.clone().into_os_string(),
                "--icon".into(),
                icon.into_os_string(),
                "--json".into(),
            ],
        ),
        (
            Operation::ShortcutRemove { path: path.clone() },
            vec![
                "shortcut".into(),
                "remove".into(),
                "--path".into(),
                path.clone().into_os_string(),
                "--json".into(),
            ],
        ),
        (
            Operation::ShortcutCreate {
                path: path.clone(),
                icon: None,
            },
            vec![
                "shortcut".into(),
                "create".into(),
                "--path".into(),
                path.into_os_string(),
                "--json".into(),
            ],
        ),
    ];
    for (operation, args) in cases {
        let command = opts.command(operation);
        assert_eq!(command.args, args);
        assert_eq!(command.program, opts.engine);
        assert_eq!(command.mode, Mode::Wait);
        assert!(command.stdin.is_none());
        assert!(command.timeout <= std::time::Duration::from_secs(10));
    }
    let no_config = Options {
        config: None,
        ..opts
    };
    assert_eq!(
        no_config
            .command(Operation::StartupEnable { delay_secs: 0 })
            .args,
        ["startup", "enable", "--delay-secs", "0", "--json"].map(OsString::from)
    );
    assert_eq!(no_config.start_menu_launch().args, ["start-menu"]);
}

#[test]
fn startup_actions_are_explicit_separate_and_never_save_or_run() {
    let enabled = startup(true);
    let disabled = startup(false);
    let mut c = controller(vec![
        ok(serde_json::to_vec(&disabled).unwrap()),
        ok(serde_json::to_vec(&enabled).unwrap()),
        ok(serde_json::to_vec(&disabled).unwrap()),
    ]);
    c.form = Some(Form {
        poll_ms: "incomplete edit".into(),
        startup_delay_secs: "19".into(),
        ..Form::default()
    });
    let form = c.form.clone();
    c.perform(Action::StartupRefresh).unwrap();
    assert_eq!(c.startup.as_ref(), Some(&disabled));
    let outcome = c.perform(Action::StartupEnable { delay_secs: 19 }).unwrap();
    assert!(outcome.message.contains("form NOT saved"));
    assert!(outcome.message.contains("Registered executable:"));
    assert!(outcome.message.contains(options().engine.to_str().unwrap()));
    assert!(outcome
        .message
        .contains(options().config.unwrap().to_str().unwrap()));
    assert!(outcome.message.contains("fixture warning"));
    assert_eq!(c.form, form);
    assert_eq!(c.startup, Some(enabled));
    c.perform(Action::StartupDisable).unwrap();
    assert_eq!(c.startup, Some(disabled));
    assert_eq!(c.form, form);
    assert!(c.saved.is_none());
    assert_eq!(
        operations(&c),
        [
            "startup status --json",
            "startup enable --delay-secs 19 --json",
            "startup disable --json"
        ]
    );
}

#[test]
fn startup_failures_do_not_chain_actions_and_invalidate_old_status() {
    for failure in [
        failed(),
        timed_out(),
        Err("worker failed".into()),
        ok("{}"),
        ok("{} {}"),
        ok("not json"),
    ] {
        let mut c = controller(vec![failure]);
        c.startup = Some(startup(true));
        assert!(c.perform(Action::StartupEnable { delay_secs: 6 }).is_err());
        assert!(c.startup.is_none());
        assert_eq!(operations(&c), ["startup enable --delay-secs 6 --json"]);
    }
    let mut c = controller(vec![]);
    assert!(c
        .perform(Action::StartupEnable { delay_secs: 86401 })
        .is_err());
    assert!(operations(&c).is_empty());
}

#[test]
fn restart_uses_saved_configuration_and_never_saves_the_current_form() {
    let mut c = controller(vec![ok("restored"), running()]);
    c.form = Some(Form {
        poll_ms: "invalid".into(),
        ..Form::default()
    });
    let form = c.form.clone();
    assert!(c
        .perform(Action::Restart)
        .unwrap()
        .message
        .contains("current form was NOT saved"));
    assert_eq!(operations(&c), ["stop --timeout-ms 5000", "run"]);
    assert_eq!(c.form, form);
    for failure in [failed(), timed_out(), Err("cleanup unavailable".into())] {
        let mut c = controller(vec![failure]);
        assert!(c
            .perform(Action::Restart)
            .unwrap_err()
            .contains("No new runner"));
        assert_eq!(operations(&c), ["stop --timeout-ms 5000"]);
    }
}

#[test]
fn shortcut_generation_is_local_mutations_are_explicit_and_protocol_checked() {
    let path = std::env::temp_dir().join("Start \u{989c}\u{8272} & name.lnk");
    let reply = serde_json::json!({ "ok": true, "path": path }).to_string();
    let mut c = controller(vec![ok(reply.clone()), ok(reply), ok("")]);
    let report = c.perform(Action::GenerateShortcut).unwrap();
    assert!(report.message.contains("Display only"));
    assert!(report.message.contains("start-menu"));
    assert!(!report.message.contains("--config"));
    assert!(operations(&c).is_empty());
    c.perform(Action::ShortcutCreate {
        path: path.clone(),
        icon: None,
    })
    .unwrap();
    c.perform(Action::ShortcutRemove { path: path.clone() })
        .unwrap();
    c.perform(Action::OpenStart).unwrap();
    assert!(c.form.is_none());
    assert!(c
        .process
        .calls
        .borrow()
        .iter()
        .all(|s| !s.args.iter().any(|arg| arg == "--config")));
    assert_eq!(c.process.calls.borrow().len(), 3);
    for failure in [
        failed(),
        timed_out(),
        ok(r#"{"ok":false,"path":"x"}"#),
        ok(r#"{"ok":true,"path":42}"#),
        ok("{}"),
        ok("[]"),
        ok("not json"),
    ] {
        let mut c = controller(vec![failure]);
        assert!(c
            .perform(Action::ShortcutCreate {
                path: path.clone(),
                icon: None
            })
            .is_err());
        assert_eq!(c.process.calls.borrow().len(), 1);
    }
}

#[test]
fn every_new_field_survives_load_edit_save_apply_and_reload() {
    let s = taskbarx_config::from_json(
        r#"{
        "primary_opacity":0,"secondary_opacity":100,"corner_radius":10000,
        "taskbar_segments":true,"default_style_on_maximized":true,
        "battery_animation":"none","battery_poll_ms":50,"skip_resolutions":[1,1920,65535],
        "pause_on_fullscreen":true,"hide_primary_start":true,"hide_secondary_start":true,
        "hide_primary_tray":true,"hide_secondary_tray":true,"revert_on_tray_overlap":true,
        "toolbar_compensation":false,"tray_icon":true,"startup_delay_secs":86400,
        "measurement":"uia","show_console":true,"color":[1,2,3,4]
    }"#,
    )
    .unwrap();
    let mut changed = s.clone();
    changed.primary_offset = -42;
    let mut c = controller(vec![
        ok(encode(&s)),
        ok(""),
        ok(""),
        ok(""),
        running(),
        ok(encode(&changed)),
    ]);
    c.perform(Action::Initialize).unwrap();
    let mut form = c.form.clone().unwrap();
    form.primary_offset = "-42".into();
    c.perform(Action::Edit(form.clone())).unwrap();
    c.perform(Action::Save(form.clone())).unwrap();
    c.perform(Action::Apply(form)).unwrap();
    c.perform(Action::Reset).unwrap();
    assert_eq!(c.form, Some(Form::default()));
    c.perform(Action::Reload).unwrap();
    assert_eq!(c.form.unwrap().settings().unwrap(), changed);
    for spec in c
        .process
        .calls
        .borrow()
        .iter()
        .filter(|s| s.stdin.is_some())
    {
        assert_eq!(decode(spec.stdin.as_ref().unwrap()).unwrap(), changed);
    }
    assert_eq!(c.saved, Some(changed));
    assert!(c
        .process
        .calls
        .borrow()
        .iter()
        .all(|s| s.args[0] != "startup"));
}

#[test]
fn saved_snapshot_survives_failed_apply_cleanup() {
    let mut c = controller(vec![ok(""), failed()]);
    let form = Form {
        corner_radius: "321".into(),
        ..Form::default()
    };
    assert!(c.perform(Action::Apply(form.clone())).is_err());
    assert_eq!(c.saved, Some(form.settings().unwrap()));
    assert_eq!(
        operations(&c),
        ["config import --stdin", "stop --timeout-ms 5000"]
    );
}

#[test]
fn hex_editing_preserves_alpha_and_rejects_incomplete_or_ambiguous_values() {
    use taskbarx_configurator::settings::{hex_rgba, rgba_hex};
    assert_eq!(hex_rgba("#12aBef", 7).unwrap(), [18, 171, 239, 7]);
    assert_eq!(hex_rgba("#12abEF80", 7).unwrap(), [18, 171, 239, 128]);
    assert_eq!(rgba_hex([18, 171, 239, 128]), "#12ABEF80");
    for bad in [
        "",
        "#",
        "#123",
        "#1234567",
        "123456",
        "#GG0000",
        "#123456789",
        "#\u{e9}12345",
        "#12 456",
    ] {
        assert_eq!(hex_rgba(bad, 7).unwrap_err().field, "color_hex");
    }
}

#[test]
fn local_previews_cover_every_curve_inheritance_none_zero_and_completion() {
    let c = controller(vec![]);
    for &easing in ANIMATIONS {
        for battery in [false, true] {
            for duration in [0, 300] {
                let form = Form {
                    animation: easing.into(),
                    animation_ms: duration.to_string(),
                    battery_animation: if battery {
                        easing.into()
                    } else {
                        String::new()
                    },
                    poll_ms: "incomplete unrelated edit".into(),
                    ..Form::default()
                };
                let preview = form.preview(battery).unwrap();
                assert_eq!(preview.animation, easing);
                for elapsed in [0, 75, 150, 300, 10000] {
                    let (position, done) = preview.sample(elapsed).unwrap();
                    assert!((0..=1000).contains(&position));
                    assert_eq!(done, easing == "none" || elapsed >= duration);
                    if done {
                        assert_eq!(position, 1000);
                    }
                }
            }
        }
    }
    let mut form = Form {
        animation: "linear".into(),
        ..Form::default()
    };
    assert_eq!(form.preview(true).unwrap().animation, "linear");
    form.animation = "bounceeaseout".into();
    assert_eq!(form.preview(true).unwrap().animation, "bounceeaseout");
    form.battery_animation = "none".into();
    assert_eq!(form.preview(true).unwrap().sample(0).unwrap(), (1000, true));
    form.animation_ms = "10001".into();
    assert!(form.preview(false).is_err());
    assert!(operations(&c).is_empty());
}

#[test]
fn new_text_fields_use_shared_validation_without_clamping_or_losing_inheritance() {
    for (field, text) in [
        ("primary_opacity", "101"),
        ("secondary_opacity", "-1"),
        ("corner_radius", "10001"),
        ("battery_animation", "unknown"),
        ("battery_poll_ms", "49"),
        ("skip_resolutions", "65536"),
        ("startup_delay_secs", "86401"),
        ("measurement", "UIA"),
    ] {
        let mut f = Form::default();
        *match field {
            "primary_opacity" => &mut f.primary_opacity,
            "secondary_opacity" => &mut f.secondary_opacity,
            "corner_radius" => &mut f.corner_radius,
            "battery_animation" => &mut f.battery_animation,
            "battery_poll_ms" => &mut f.battery_poll_ms,
            "skip_resolutions" => &mut f.skip_resolutions[2],
            "startup_delay_secs" => &mut f.startup_delay_secs,
            "measurement" => &mut f.measurement,
            _ => unreachable!(),
        } = text.into();
        assert_eq!(f.settings().unwrap_err().field, field);
    }
    let f = Form {
        primary_opacity: "100".into(),
        secondary_opacity: "0".into(),
        battery_poll_ms: " ".into(),
        skip_resolutions: ["".into(), "1920".into(), "".into()],
        ..Form::default()
    };
    let s = f.settings().unwrap();
    assert_eq!(s.primary_opacity, Some(100));
    assert_eq!(s.secondary_opacity, Some(0));
    assert!(s.battery_animation.is_none());
    assert!(s.battery_poll_ms.is_none());
    assert_eq!(s.skip_resolutions, [1920]);
}
