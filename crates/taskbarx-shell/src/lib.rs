//! Explicit shell operations, independent of settings, the engine and the GUI.
//!
//! Loading this library has no side effects. Consumers decide when to register
//! startup or create a tray icon; Save/Apply/run must never register implicitly.
//! All native effects have narrow adapter boundaries. Unit tests use only fakes.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod command_line;
#[cfg(windows)]
mod native;
mod shortcut;
mod start_menu;
mod startup;
mod tray;

pub use shortcut::{create_start_shortcut, remove_start_shortcut};
pub use start_menu::show_start_menu;
pub use startup::{disable_startup, enable_startup, startup_status};
pub use tray::{TrayAction, TrayIcon};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupStatus {
    pub registered: bool,
    pub enabled: bool,
    pub owned: bool,
    pub task_name: String,
    pub launch: Option<LaunchSpec>,
    pub delay_secs: Option<u32>,
    pub warnings: Vec<String>,
}

/// Render Windows argv for display, never for a command shell. This string is
/// NOT safe to interpolate into cmd.exe, PowerShell, or another shell language.
pub fn format_command_line(spec: &LaunchSpec) -> Result<String, String> {
    validate_launch(spec)?;
    let mut parts = vec![command_line::quote(path_text(&spec.executable)?)];
    parts.extend(spec.args.iter().map(|arg| command_line::quote(arg)));
    Ok(parts.join(" "))
}

fn text_valid(value: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err("NUL characters are not supported".into());
    }
    if value.encode_utf16().count() > 32766 {
        return Err("value exceeds the Windows UTF-16 length limit".into());
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str, String> {
    let text = path
        .to_str()
        .ok_or("path encoding is not supported: valid Unicode is required (no lossy conversion)")?;
    text_valid(text)?;
    if !path.is_absolute() {
        return Err("all shell paths must be absolute".into());
    }
    Ok(text)
}

fn validate_launch(spec: &LaunchSpec) -> Result<(), String> {
    path_text(&spec.executable)?;
    path_text(&spec.working_directory)?;
    for arg in &spec.args {
        text_valid(arg)?;
    }
    let length = command_line::quote(path_text(&spec.executable)?)
        .encode_utf16()
        .count()
        + spec
            .args
            .iter()
            .map(|a| command_line::quote(a).encode_utf16().count() + 1)
            .sum::<usize>();
    if length > 32766 {
        return Err("command line exceeds the Windows UTF-16 length limit".into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn unsupported() -> String {
    "native shell integration is unsupported on this platform (and disabled in unit tests)".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn spec() -> LaunchSpec {
        let root = if cfg!(windows) {
            "C:/Program Files/TaskbarX-Rust"
        } else {
            "/fixture/TaskbarX-Rust"
        };
        LaunchSpec {
            executable: Path::new(root).join("taskbarx.exe"),
            args: vec![
                "run".into(),
                "--background".into(),
                "--config".into(),
                Path::new(root)
                    .join("config file.json")
                    .to_str()
                    .unwrap()
                    .into(),
            ],
            working_directory: root.into(),
        }
    }

    #[test]
    fn absolute_unicode_and_nul_validation() {
        let mut launch = spec();
        assert!(format_command_line(&launch).is_ok());
        launch.args.push("\u{1f680} & < > \\\"".into());
        assert!(format_command_line(&launch).unwrap().contains('\u{1f680}'));
        launch.args.push("bad\0argument".into());
        assert!(format_command_line(&launch).is_err());
        launch = spec();
        launch.working_directory = "relative".into();
        assert!(format_command_line(&launch).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_unpaired_surrogates_without_lossy_conversion() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        let mut launch = spec();
        launch.executable = OsString::from_wide(&[67, 58, 92, 0xd800]).into();
        assert!(format_command_line(&launch)
            .unwrap_err()
            .contains("encoding"));
    }

    #[cfg(not(windows))]
    #[test]
    fn native_apis_explicitly_reject_unsupported_platforms() {
        let launch = spec();
        let path = launch.working_directory.join("fixture.lnk");
        for result in [
            startup_status().map(|_| ()),
            enable_startup(&launch, 6).map(|_| ()),
            disable_startup().map(|_| ()),
            create_start_shortcut(&path, &launch, None),
            remove_start_shortcut(&path),
            show_start_menu(),
        ] {
            assert!(result.unwrap_err().contains("unsupported"));
        }
        assert!(TrayIcon::new().is_err());
    }
}
