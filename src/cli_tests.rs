//! Mutating CLI commands are dispatched only into fake services, NEVER child
//! processes. Temp files are explicit test configs, not the user's settings.
use super::*;
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use taskbarx_shell::StartupStatus;

#[derive(Default)]
struct Fake {
    calls: Vec<&'static str>,
    launch: Option<LaunchSpec>,
    delay: Option<u32>,
    icon: Option<PathBuf>,
    run: Option<(Settings, RunContext)>,
    fail: bool,
}
fn status() -> StartupStatus {
    StartupStatus {
        registered: true,
        enabled: true,
        owned: true,
        task_name: "owned task".into(),
        launch: None,
        delay_secs: Some(6),
        warnings: vec!["sample warning".into()],
    }
}
fn executable() -> PathBuf {
    std::env::temp_dir()
        .join("Program Files")
        .join("\u{96ea}")
        .join("taskbarx.exe")
}
impl Fake {
    fn call(&mut self, name: &'static str) -> Result<(), String> {
        self.calls.push(name);
        if self.fail {
            Err("injected service failure".into())
        } else {
            Ok(())
        }
    }
}
impl Services for Fake {
    fn current_exe(&mut self) -> Result<PathBuf, String> {
        Ok(executable())
    }
    fn absolute(&mut self, path: &Path) -> Result<PathBuf, String> {
        std::path::absolute(path).map_err(|e| e.to_string())
    }
    fn run(&mut self, s: &Settings, context: &RunContext) -> Result<(), String> {
        self.run = Some((s.clone(), context.clone()));
        self.call("run")
    }
    fn stop(&mut self, _: u32) -> Result<(), String> {
        self.call("stop")
    }
    fn diagnose(&mut self, _: &Settings) -> Result<serde_json::Value, String> {
        self.call("diagnose")?;
        Ok(serde_json::json!({"read_only":true}))
    }
    fn configure(&mut self, _: &RunContext) -> Result<(), String> {
        self.call("configure")
    }
    fn startup_status(&mut self) -> Result<StartupStatus, String> {
        self.call("status")?;
        Ok(status())
    }
    fn enable_startup(&mut self, launch: &LaunchSpec, delay: u32) -> Result<StartupStatus, String> {
        self.launch = Some(launch.clone());
        self.delay = Some(delay);
        self.call("enable")?;
        Ok(status())
    }
    fn disable_startup(&mut self) -> Result<StartupStatus, String> {
        self.call("disable")?;
        Ok(status())
    }
    fn create_shortcut(
        &mut self,
        _: &Path,
        launch: &LaunchSpec,
        icon: Option<&Path>,
    ) -> Result<(), String> {
        self.launch = Some(launch.clone());
        self.icon = icon.map(Path::to_owned);
        self.call("create")
    }
    fn remove_shortcut(&mut self, _: &Path) -> Result<(), String> {
        self.call("remove")
    }
    fn start_menu(&mut self) -> Result<(), String> {
        self.call("start-menu")
    }
}
struct ConfigFile(PathBuf);
impl ConfigFile {
    fn new(s: &Settings) -> Self {
        static SERIAL: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "taskbarx-runtime-cli-{}-{} space \u{96ea}.json",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let file = Self(path);
        std::fs::write(&file.0, config::to_json(s).unwrap()).unwrap();
        file
    }
}
impl Drop for ConfigFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn dispatch(args: &[&str], fake: &mut Fake) -> Result<Vec<u8>, String> {
    let cli = Cli::try_parse_from(std::iter::once("taskbarx").chain(args.iter().copied()))
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    execute_with(cli, fake, &b""[..], &mut out)?;
    Ok(out)
}

#[test]
fn startup_enable_resolves_context_and_delay_without_running_or_saving() {
    let file = ConfigFile::new(&Settings {
        startup_delay_secs: 25,
        ..Settings::default()
    });
    let before = std::fs::read(&file.0).unwrap();
    let modified = std::fs::metadata(&file.0).unwrap().modified().unwrap();
    for delay in [None, Some("86400")] {
        let mut fake = Fake::default();
        let mut args = vec![
            "startup",
            "enable",
            "--config",
            file.0.to_str().unwrap(),
            "--json",
        ];
        if let Some(delay) = delay {
            args.extend(["--delay-secs", delay]);
        }
        let out = dispatch(&args, &mut fake).unwrap();
        assert_eq!(
            serde_json::from_slice::<StartupStatus>(&out).unwrap(),
            status()
        );
        assert_eq!(fake.calls, ["enable"]);
        let spec = fake.launch.unwrap();
        assert_eq!(spec.executable, executable());
        assert_eq!(spec.working_directory, executable().parent().unwrap());
        assert_eq!(
            spec.args,
            ["run", "--background", "--config", file.0.to_str().unwrap()]
        );
        assert_eq!(fake.delay, Some(if delay.is_some() { 86400 } else { 25 }));
        assert_eq!(std::fs::read(&file.0).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&file.0).unwrap().modified().unwrap(),
            modified
        );
    }
}

#[test]
fn config_free_commands_reject_config_and_json_serializes_status_directly() {
    for command in ["status", "disable"] {
        assert!(Cli::try_parse_from([
            "taskbarx",
            "startup",
            command,
            "--config",
            "do not load.json"
        ])
        .is_err());
        let mut fake = Fake::default();
        let out = dispatch(&["startup", command, "--json"], &mut fake).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&out).unwrap(),
            serde_json::to_value(status()).unwrap()
        );
        assert_eq!(fake.calls, [command]);
    }
    for args in [
        vec!["start-menu"],
        vec!["shortcut", "create", "--path", "x.lnk"],
        vec!["shortcut", "remove", "--path", "x.lnk"],
    ] {
        let mut args = args;
        args.extend(["--config", "invalid.json"]);
        assert!(Cli::try_parse_from(std::iter::once("taskbarx").chain(args)).is_err());
    }
}

#[test]
fn shortcut_uses_exact_argv_and_never_executes_start_menu() {
    let path = "folder with spaces/\u{96ea}.lnk";
    let mut fake = Fake::default();
    let out = dispatch(
        &[
            "shortcut",
            "create",
            "--path",
            path,
            "--icon",
            "icon space.ico",
            "--json",
        ],
        &mut fake,
    )
    .unwrap();
    assert_eq!(fake.calls, ["create"]);
    assert_eq!(fake.launch.as_ref().unwrap().args, ["start-menu"]);
    assert_eq!(fake.launch.as_ref().unwrap().executable, executable());
    assert_eq!(fake.icon, Some("icon space.ico".into()));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out).unwrap(),
        serde_json::json!({"ok":true,"path":path})
    );
    fake.calls.clear();
    let out = dispatch(&["shortcut", "remove", "--path", path, "--json"], &mut fake).unwrap();
    assert_eq!(fake.calls, ["remove"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out).unwrap()["path"],
        path
    );
    fake.calls.clear();
    assert!(dispatch(&["start-menu"], &mut fake).unwrap().is_empty());
    assert_eq!(fake.calls, ["start-menu"]);
}

#[test]
fn failure_has_no_success_shaped_output() {
    let file = ConfigFile::new(&Settings::default());
    for args in [
        vec!["startup", "status", "--json"],
        vec!["startup", "disable", "--json"],
        vec![
            "startup",
            "enable",
            "--config",
            file.0.to_str().unwrap(),
            "--json",
        ],
        vec!["shortcut", "create", "--path", "x.lnk", "--json"],
        vec!["shortcut", "remove", "--path", "x.lnk", "--json"],
        vec!["start-menu"],
    ] {
        let cli = Cli::try_parse_from(std::iter::once("taskbarx").chain(args)).unwrap();
        let mut out = Vec::new();
        let mut fake = Fake {
            fail: true,
            ..Fake::default()
        };
        assert!(execute_with(cli, &mut fake, &b""[..], &mut out)
            .unwrap_err()
            .contains("injected"));
        assert!(out.is_empty()); // main's existing Err path writes stderr/nonzero.
    }
}

#[test]
fn help_noargs_config_and_diagnose_have_no_mutating_service_access() {
    let file = ConfigFile::new(&Settings {
        tray_icon: true,
        primary_opacity: Some(0),
        ..Settings::default()
    });
    for args in [
        vec![],
        vec!["config", "show", "--config", file.0.to_str().unwrap()],
        vec!["diagnose", "--config", file.0.to_str().unwrap()],
    ] {
        let mut fake = Fake::default();
        dispatch(&args, &mut fake).unwrap();
        assert!(fake.calls.iter().all(|v| *v == "diagnose"));
    }
    for args in [
        vec!["--help"],
        vec!["run", "--help"],
        vec!["startup", "enable", "--help"],
        vec!["shortcut", "create", "--help"],
        vec!["start-menu", "--help"],
    ] {
        assert_eq!(
            Cli::try_parse_from(std::iter::once("taskbarx").chain(args))
                .err()
                .unwrap()
                .kind(),
            clap::error::ErrorKind::DisplayHelp
        );
    }
    let mut fake = Fake::default();
    let mut out = Vec::new();
    let cli = Cli::try_parse_from([
        "taskbarx",
        "config",
        "import",
        "--stdin",
        "--config",
        file.0.to_str().unwrap(),
    ])
    .unwrap();
    let text = config::to_json(&Settings::default()).unwrap();
    execute_with(cli, &mut fake, text.as_bytes(), &mut out).unwrap();
    assert!(fake.calls.is_empty() && out.is_empty());
}

#[test]
fn all_runtime_overrides_roundtrip_through_shared_settings_and_never_save() {
    let file = ConfigFile::new(&Settings {
        primary_opacity: Some(99),
        battery_animation: Some("linear".into()),
        battery_poll_ms: Some(800),
        ..Settings::default()
    });
    let mut fake = Fake::default();
    dispatch(
        &[
            "run",
            "--config",
            file.0.to_str().unwrap(),
            "--background",
            "--primary-opacity",
            "default",
            "--secondary-opacity",
            "0",
            "--battery-animation",
            "inherit",
            "--battery-poll-ms",
            "inherit",
            "--corner-radius",
            "10000",
            "--taskbar-segments",
            "true",
            "--default-style-on-maximized",
            "true",
            "--pause-on-fullscreen",
            "true",
            "--hide-primary-start",
            "true",
            "--hide-secondary-start",
            "true",
            "--hide-primary-tray",
            "true",
            "--hide-secondary-tray",
            "true",
            "--revert-on-tray-overlap",
            "true",
            "--toolbar-compensation",
            "false",
            "--tray-icon",
            "true",
            "--show-console",
            "true",
            "--measurement",
            "uia",
            "--skip-resolution",
            "1920",
            "--skip-resolution",
            "2560",
            "--skip-resolution",
            "3840",
            "--center",
            "false",
            "--center-between",
            "false",
            "--style",
            "transparent-gradient",
        ],
        &mut fake,
    )
    .unwrap();
    assert_eq!(fake.calls, ["run"]);
    let (s, context) = fake.run.unwrap();
    assert!(context.background && context.config.is_absolute());
    assert_eq!(s.primary_opacity, None);
    assert_eq!(s.secondary_opacity, Some(0));
    assert_eq!(s.battery_animation, None);
    assert_eq!(s.battery_poll_ms, None);
    assert_eq!(s.skip_resolutions, [1920, 2560, 3840]);
    assert_eq!(s.corner_radius, 10000);
    assert!(
        s.taskbar_segments
            && s.default_style_on_maximized
            && s.pause_on_fullscreen
            && s.tray_icon
            && s.show_console
    );
    assert!(
        s.hide_primary_start
            && s.hide_secondary_start
            && s.hide_primary_tray
            && s.hide_secondary_tray
            && s.revert_on_tray_overlap
    );
    assert!(!s.toolbar_compensation && !s.center && !s.center_between);
    assert_eq!(s.measurement, "uia");
    assert_eq!(config::from_json(&config::to_json(&s).unwrap()).unwrap(), s);
    assert_eq!(config::load(&file.0).unwrap().primary_opacity, Some(99));
}

#[test]
fn override_validation_precedes_any_service_and_clearing_widths_is_explicit() {
    let file = ConfigFile::new(&Settings {
        skip_resolutions: vec![1920],
        ..Settings::default()
    });
    for extra in [
        vec!["--primary-opacity", "101"],
        vec!["--secondary-opacity", "-1"],
        vec!["--corner-radius", "10001"],
        vec!["--battery-poll-ms", "49"],
        vec!["--battery-animation", "invalid"],
        vec!["--tray-icon", "1"],
        vec!["--skip-resolution", "0"],
        vec!["--skip-resolution", "65536"],
        vec![
            "--skip-resolution",
            "1",
            "--skip-resolution",
            "2",
            "--skip-resolution",
            "3",
            "--skip-resolution",
            "4",
        ],
    ] {
        let mut args = vec!["run", "--config", file.0.to_str().unwrap()];
        args.extend(extra);
        let mut fake = Fake::default();
        assert!(dispatch(&args, &mut fake).is_err());
        assert!(fake.calls.is_empty());
    }
    let mut fake = Fake::default();
    dispatch(
        &[
            "run",
            "--config",
            file.0.to_str().unwrap(),
            "--clear-skip-resolutions",
        ],
        &mut fake,
    )
    .unwrap();
    assert!(fake.run.unwrap().0.skip_resolutions.is_empty());
    assert!(
        Cli::try_parse_from(["taskbarx", "startup", "enable", "--delay-secs", "86401"]).is_err()
    );
}

#[test]
fn argv_boundaries_are_not_prequoted_or_shell_interpreted() {
    // Deliberately include quotation/backslash boundaries in a pure launch-spec
    // test (quotes are not legal Windows file names). Escaping belongs to shell.
    let config = std::env::temp_dir().join("space \u{96ea} \\\" boundary\\");
    let spec = startup_launch(RunContext {
        engine: executable(),
        config: config.clone(),
        background: true,
    })
    .unwrap();
    assert_eq!(spec.args.len(), 4);
    assert_eq!(spec.args[3], config.to_str().unwrap());
    assert_eq!(spec.executable, executable());
}
