//! These process tests NEVER invoke run, stop, or configure against a desktop.
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "taskbarx-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_taskbarx"));
        command
            .current_dir(&self.0)
            .env("APPDATA", &self.0)
            .env("LOCALAPPDATA", &self.0)
            .env("XDG_CONFIG_HOME", &self.0)
            .env("HOME", &self.0)
            .env("USERPROFILE", &self.0);
        command
    }
    fn entries(&self) -> usize {
        std::fs::read_dir(&self.0).unwrap().count()
    }
    fn config(&self) -> PathBuf {
        self.0.join("nested").join("settings.json")
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn default_help_and_config_show_make_zero_files() {
    let sandbox = Sandbox::new();
    for args in [vec![], vec!["--help"], vec!["config", "show"]] {
        let output = sandbox.command().args(args).output().unwrap();
        success(&output);
        assert!(output.stderr.is_empty());
        assert_eq!(sandbox.entries(), 0);
    }
    let output = sandbox
        .command()
        .args(["config", "show", "--config"])
        .arg(sandbox.config())
        .output()
        .unwrap();
    success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(sandbox.entries(), 0);
}

#[test]
fn import_is_validated_saved_and_only_explicit_file_is_changed() {
    let sandbox = Sandbox::new();
    let path = sandbox.config();
    let s = taskbarx_config::Settings {
        primary_offset: -42,
        ..Default::default()
    };
    let input = taskbarx_config::to_json(&s).unwrap();
    let mut child = sandbox
        .command()
        .args(["config", "import", "--stdin", "--config"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    success(&output);
    assert!(output.stdout.is_empty());
    let before = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let show = sandbox
        .command()
        .args(["config", "show", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    success(&show);
    assert!(show.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&show.stdout).unwrap()["primary_offset"],
        -42
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    let mut child = sandbox
        .command()
        .args(["config", "import", "--stdin", "--config"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{invalid}").unwrap();
    let invalid = child.wait_with_output().unwrap();
    assert!(!invalid.status.success());
    assert!(invalid.stdout.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn import_rejects_oversize_and_invalid_utf8_without_writes() {
    let sandbox = Sandbox::new();
    for input in [vec![b' '; 65537], vec![0xff]] {
        let oversized = input.len() > 65536;
        let mut child = sandbox
            .command()
            .args(["config", "import", "--stdin", "--config"])
            .arg(sandbox.config())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&input).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        if oversized {
            assert!(String::from_utf8_lossy(&output.stderr).contains("64 KiB"));
        }
        assert!(output.stdout.is_empty());
        assert_eq!(sandbox.entries(), 0);
    }
}

#[test]
fn import_accepts_exactly_64_kib_of_valid_json() {
    let sandbox = Sandbox::new();
    let mut input = taskbarx_config::to_json(&taskbarx_config::Settings::default())
        .unwrap()
        .into_bytes();
    input.resize(65536, b' ');
    let mut child = sandbox
        .command()
        .args(["config", "import", "--stdin", "--config"])
        .arg(sandbox.config())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&input).unwrap();
    success(&child.wait_with_output().unwrap());
    taskbarx_config::load(&sandbox.config())
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn show_rejects_invalid_existing_config_without_modifying_it() {
    let sandbox = Sandbox::new();
    let path = sandbox.0.join("invalid.json");
    let invalid = br#"{"schema_version":0}"#;
    std::fs::write(&path, invalid).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let output = sandbox
        .command()
        .args(["config", "show", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), invalid);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(sandbox.entries(), 1);
}

#[test]
fn invalid_arguments_have_nonzero_exit_and_no_writes() {
    let sandbox = Sandbox::new();
    for args in [
        vec!["not-a-command"],
        vec!["config", "import"],
        vec!["diagnose", "--bogus"],
    ] {
        let output = sandbox.command().args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
        assert_eq!(sandbox.entries(), 0);
    }
}

#[test]
fn diagnose_json_is_read_only_and_uncontaminated() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .command()
        .args(["diagnose", "--json", "--config"])
        .arg(sandbox.config())
        .output()
        .unwrap();
    assert_eq!(sandbox.entries(), 0);
    if output.status.success() {
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value.is_object());
        assert_eq!(value["read_only"], true);
        assert!(value["os"].is_object());
        assert!(value["warnings"].is_array());
        assert!(output.stderr.is_empty());
    } else {
        // Unsupported hosts / unavailable COM are legitimate errors, not JSON logs.
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
