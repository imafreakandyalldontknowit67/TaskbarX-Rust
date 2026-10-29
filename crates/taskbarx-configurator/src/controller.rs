use crate::{
    process::{Process, ProcessReply},
    protocol::{Operation, Options},
    settings::{Form, Settings},
};
use std::path::PathBuf;
use taskbarx_config::{from_json, to_json};
use taskbarx_shell::StartupStatus;

#[derive(Clone, Debug)]
pub enum Action {
    Initialize,
    Edit(Form),
    Reset,
    Reload,
    Save(Form),
    Apply(Form),
    Stop,
    Restart,
    StartupRefresh,
    StartupEnable {
        delay_secs: u32,
    },
    StartupDisable,
    GenerateShortcut,
    ShortcutCreate {
        path: PathBuf,
        icon: Option<PathBuf>,
    },
    ShortcutRemove {
        path: PathBuf,
    },
    OpenStart,
    Diagnostics,
}
#[derive(Clone, Debug)]
pub struct Outcome {
    /// Some only when the caller should replace the visible form.
    pub form: Option<Form>,
    pub message: String,
}
pub struct Controller<P> {
    pub process: P,
    pub options: Options,
    pub form: Option<Form>,
    pub saved: Option<Settings>,
    pub startup: Option<StartupStatus>,
}
impl<P: Process> Controller<P> {
    pub fn new(options: Options, process: P) -> Self {
        Self {
            process,
            options,
            form: None,
            saved: None,
            startup: None,
        }
    }

    fn checked(&mut self, operation: Operation) -> Result<ProcessReply, String> {
        let spec = self.options.command(operation);
        let result = self.process.execute(&spec)?;
        if result.timed_out {
            return Err(format!(
                "{} timed out after {} ms. State is unknown; no further action was taken.{}",
                spec.label(),
                spec.timeout.as_millis(),
                result.output()
            ));
        }
        if !result.success() {
            return Err(format!(
                "{} failed (exit {:?}).{}",
                spec.label(),
                result.code,
                result.output()
            ));
        }
        Ok(result)
    }

    fn restart(&mut self, prefix: &str, saved_output: &str) -> Result<String, String> {
        let stopped = self.checked(Operation::Stop).map_err(|e| {
            format!("{prefix}, but stopping failed. No new runner was launched.\n{e}{saved_output}")
        })?;
        let spec = self.options.command(Operation::Run);
        let launched = self.process.execute(&spec).map_err(|e| {
            format!(
                "{prefix}; stop completed, but run could not be confirmed: {e}{saved_output}{}",
                stopped.output()
            )
        })?;
        if launched.timed_out || launched.pid.is_none() {
            return Err(format!("{prefix}; stop completed, but run did not remain running (exit {:?}, timeout {}).{saved_output}{}{}", launched.code, launched.timed_out, stopped.output(), launched.output()));
        }
        Ok(format!("{prefix}; stop completed; engine process {} launched. Launch is NOT a support/health confirmation; use read-only diagnostics.\nClosing this configurator does not stop the engine. Startup was not changed.{saved_output}{}{}", launched.pid.unwrap(), stopped.output(), launched.output()))
    }
    fn startup_operation(&mut self, operation: Operation, message: &str) -> Result<String, String> {
        // A failed refresh/mutation must not leave a stale green status visible.
        self.startup = None;
        let reply = self.checked(operation)?;
        let status: StartupStatus = serde_json::from_slice(&reply.stdout)
            .map_err(|e| format!("Startup protocol error: {e}. Refresh status before assuming registration changed.{}", reply.output()))?;
        let report = startup_report(&status);
        self.startup = Some(status);
        Ok(format!("{message}.\n{report}{}", stderr_note(&reply)))
    }
    fn shortcut_operation(
        &mut self,
        operation: Operation,
        path: &std::path::Path,
        verb: &str,
    ) -> Result<String, String> {
        let reply = self.checked(operation)?;
        let value: serde_json::Value = serde_json::from_slice(&reply.stdout)
            .map_err(|e| format!("Shortcut protocol error: {e}{}", reply.output()))?;
        let reported_path = value
            .get("path")
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty());
        if value.get("ok").and_then(|v| v.as_bool()) != Some(true) || reported_path.is_none() {
            return Err(format!("Shortcut protocol error: expected ok:true and a nonempty path; state is unknown.{}", reply.output()));
        }
        // The shell authority may return a canonical/extended Windows path.
        // Do not treat valid normalization as failure after a successful mutation.
        Ok(format!("Shortcut {verb}: {}. Requested path: {}. Taskbar pinning remains a manual Windows operation.{}", reported_path.unwrap(), path.display(), stderr_note(&reply)))
    }

    pub fn perform(&mut self, action: Action) -> Result<Outcome, String> {
        let mut replacement = None;
        let message = match action {
            Action::Initialize | Action::Reload => {
                let reply = self.checked(Operation::Show)?;
                let settings = std::str::from_utf8(&reply.stdout)
                    .map_err(|e| format!("Settings are not UTF-8: {e}"))
                    .and_then(from_json)
                    .map_err(|e| {
                        format!(
                            "{e}\nThe form was not replaced. No defaults were loaded or saved.{}",
                            reply.output()
                        )
                    })?;
                self.saved = Some(settings.clone());
                let form = Form::from(settings);
                self.form = Some(form.clone());
                replacement = Some(form);
                format!(
                    "Settings loaded. Nothing was started, stopped, or applied.{}",
                    stderr_note(&reply)
                )
            }
            Action::Edit(form) => {
                self.form = Some(form);
                "Form edited locally; not saved or applied.".into()
            }
            Action::Reset => {
                let form = Form::default();
                self.form = Some(form.clone());
                replacement = Some(form);
                "Form reset to defaults ONLY. Save or Apply/start explicitly to replace persisted settings.".into()
            }
            Action::Save(form) => {
                let settings = form.settings().map_err(|e| e.to_string())?;
                let json = to_json(&settings)?.into_bytes();
                let reply = self.checked(Operation::Import(json))?;
                self.saved = Some(settings);
                self.form = Some(form);
                format!(
                    "Settings saved only. The running engine and taskbar were not changed.{}",
                    reply.output()
                )
            }
            Action::Apply(form) => {
                let settings = form.settings().map_err(|e| e.to_string())?;
                let json = to_json(&settings)?.into_bytes();
                let saved = self.checked(Operation::Import(json))?;
                self.saved = Some(settings);
                self.form = Some(form);
                self.restart("Settings were saved", &saved.output())?
            }
            Action::Restart => {
                self.restart("Using saved settings; current form was NOT saved", "")?
            }
            Action::StartupRefresh => self.startup_operation(
                Operation::StartupStatus,
                "Startup status refreshed (read-only)",
            )?,
            Action::StartupEnable { delay_secs } => {
                Settings {
                    startup_delay_secs: delay_secs,
                    ..Settings::default()
                }
                .validate()?;
                self.startup_operation(Operation::StartupEnable { delay_secs }, "Explicit startup enable/update completed; form NOT saved and engine NOT restarted")?
            }
            Action::StartupDisable => self.startup_operation(
                Operation::StartupDisable,
                "Explicit startup removal completed; settings and running engine preserved",
            )?,
            Action::GenerateShortcut => {
                format!("Display only; nothing created or launched.\n{}\nTaskbar pinning is a manual Windows operation.", taskbarx_shell::format_command_line(&self.options.start_menu_launch())?)
            }
            Action::ShortcutCreate { path, icon } => self.shortcut_operation(
                Operation::ShortcutCreate {
                    path: path.clone(),
                    icon,
                },
                &path,
                "created",
            )?,
            Action::ShortcutRemove { path } => self.shortcut_operation(
                Operation::ShortcutRemove { path: path.clone() },
                &path,
                "removed (ownership checked by engine)",
            )?,
            Action::OpenStart => {
                let reply = self.checked(Operation::OpenStart)?;
                format!("Explicit Open Start request completed.{}", reply.output())
            }
            Action::Stop => {
                let reply = self.checked(Operation::Stop)?;
                format!(
                    "Stop and restore command completed successfully.{}",
                    reply.output()
                )
            }
            Action::Diagnostics => {
                // Even an unsupported/nonzero result must remain visible, not
                // be turned into a green 'supported' label.
                let spec = self.options.command(Operation::Diagnose);
                let reply = self.process.execute(&spec)?;
                let value: Result<serde_json::Value, _> = serde_json::from_slice(&reply.stdout);
                let valid_object = value.as_ref().is_ok_and(|v| v.is_object());
                let report = format!("Read-only diagnostics; no settings or taskbar changes.\nExit: {:?}; timed out: {}.\nOutput is the engine's report, NOT a configurator support verdict.{}", reply.code, reply.timed_out, reply.output());
                if !reply.success() {
                    return Err(report);
                }
                if !valid_object {
                    return Err(format!(
                        "Diagnostic protocol error: expected exactly one JSON object.\n{report}"
                    ));
                }
                report
            }
        };
        Ok(Outcome {
            form: replacement,
            message,
        })
    }
}
fn stderr_note(reply: &ProcessReply) -> String {
    if reply.stderr.is_empty() {
        String::new()
    } else {
        format!("\nstderr:\n{}", String::from_utf8_lossy(&reply.stderr))
    }
}

/// Actual registration is kept separate from the form's delay preference.
pub fn startup_report(status: &StartupStatus) -> String {
    let launch = status.launch.as_ref().map(|spec| {
        let config = spec.args.windows(2).find(|pair| pair[0] == "--config").map(|pair| pair[1].as_str()).unwrap_or("No explicit --config; that executable's default configuration");
        format!("Registered executable: {}\nRegistered configuration: {config}\nRegistered command: {}\nWorking directory: {}", spec.executable.display(), taskbarx_shell::format_command_line(spec).unwrap_or_else(|e| format!("Cannot display command: {e}")), spec.working_directory.display())
    }).unwrap_or_else(|| "No registered launch specification.".into());
    format!(
        "Task: {}\nRegistered: {}; enabled: {}; owned: {}\nRegistered delay: {}\n{launch}\n{}",
        status.task_name,
        status.registered,
        status.enabled,
        status.owned,
        status
            .delay_secs
            .map(|n| format!("{n} seconds"))
            .unwrap_or_else(|| "not available".into()),
        status.warnings.join("\n")
    )
}
