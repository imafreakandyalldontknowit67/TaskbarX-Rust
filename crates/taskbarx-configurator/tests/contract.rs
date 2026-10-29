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
fn options() -> Options {
    Options {
        engine: std::env::temp_dir().join("Taskbar X 测试 & stuff/taskbarx.exe"),
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
    let json = s.to_json().unwrap();
    assert_eq!(Settings::from_json(&json).unwrap(), s);
    assert_eq!(Form::from(s.clone()).settings().unwrap(), s);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&json).unwrap(),
        serde_json::json!({
            "schema_version":1,"center":true,"primary":true,"secondary":true,"center_between":false,
            "primary_offset":0,"secondary_offset":0,"animation":"cubiceaseinout","animation_ms":300,
            "poll_ms":400,"style":"default","color":[0,0,0,128]
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
        ANIMATIONS.into_iter().map(str::to_owned).collect()
    );
    assert_eq!(ANIMATIONS.len(), 42);
    assert_eq!(
        STYLES,
        ["default", "transparent", "blur", "acrylic", "opaque"]
    );
    for animation in ANIMATIONS {
        for style in STYLES {
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
            assert_eq!(Settings::from_json(&s.to_json().unwrap()).unwrap(), s);
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
fn malformed_partial_unknown_or_newer_json_never_defaults() {
    for json in [
        b"{".as_slice(),
        b"{}",
        b"[]",
        b"null",
        b"warning\n{}",
        b"{} {}",
        b"\xff",
    ] {
        assert!(Settings::from_json(json).is_err());
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
            Settings::from_json(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{field}"
        );
    }
    for key in good.as_object().unwrap().keys() {
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(
            Settings::from_json(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "missing {key}"
        );
    }
    let duplicate = String::from_utf8(Settings::default().to_json().unwrap())
        .unwrap()
        .replacen('{', "{\"center\":false,", 1);
    assert!(Settings::from_json(duplicate.as_bytes()).is_err());
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
            "config dir/颜色 & x.json",
        ]
        .map(OsString::from),
        &exe,
        &cwd,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(explicit.engine, cwd.join("engine dir/taskbarx.exe"));
    assert_eq!(explicit.config, Some(cwd.join("config dir/颜色 & x.json")));
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
    let json = Settings::default().to_json().unwrap();
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
        ok(Settings::default().to_json().unwrap()),
        ok("saved"),
        ok(Settings::default().to_json().unwrap()),
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
    assert_eq!(Settings::from_json(&imported).unwrap().primary_offset, -321);
}

#[test]
fn invalid_load_preserves_form_and_explicit_reset_is_only_local() {
    let mut c = controller(vec![ok("{corrupt"), failed(), ok("{}")]);
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
    let mut reply = ok(Settings::default().to_json().unwrap()).unwrap();
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
