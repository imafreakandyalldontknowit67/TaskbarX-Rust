use crate::{
    process::{Process, ProcessReply},
    protocol::{Operation, Options},
    settings::{Form, Settings},
};

#[derive(Clone, Debug)]
pub enum Action {
    Initialize,
    Edit(Form),
    Reset,
    Reload,
    Save(Form),
    Apply(Form),
    Stop,
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
}
impl<P: Process> Controller<P> {
    pub fn new(options: Options, process: P) -> Self {
        Self {
            process,
            options,
            form: None,
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

    pub fn perform(&mut self, action: Action) -> Result<Outcome, String> {
        let mut replacement = None;
        let message = match action {
            Action::Initialize | Action::Reload => {
                let reply = self.checked(Operation::Show)?;
                let settings = Settings::from_json(&reply.stdout).map_err(|e| {
                    format!(
                        "{e}\nThe form was not replaced. No defaults were loaded or saved.{}",
                        reply.output()
                    )
                })?;
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
                let json = form.settings().map_err(|e| e.to_string())?.to_json()?;
                let reply = self.checked(Operation::Import(json))?;
                self.form = Some(form);
                format!(
                    "Settings saved only. The running engine and taskbar were not changed.{}",
                    reply.output()
                )
            }
            Action::Apply(form) => {
                let json = form.settings().map_err(|e| e.to_string())?.to_json()?;
                let saved = self.checked(Operation::Import(json))?;
                self.form = Some(form);
                let stopped = self.checked(Operation::Stop).map_err(|e| format!("Settings were saved, but stopping failed. No new runner was launched.\n{e}{}", saved.output()))?;
                let spec = self.options.command(Operation::Run);
                let launched = self.process.execute(&spec).map_err(|e| {
                    format!(
                        "Settings saved and stop completed, but the run request could not be confirmed: {e}{}{}",
                        saved.output(),
                        stopped.output()
                    )
                })?;
                if launched.timed_out || launched.pid.is_none() {
                    return Err(format!("Settings saved and stop completed, but run did not remain running (exit {:?}, timeout {}).{}{}{}", launched.code, launched.timed_out, saved.output(), stopped.output(), launched.output()));
                }
                format!("Settings saved; stop completed; engine process {} launched. Launch is NOT a support/health confirmation; use read-only diagnostics.\nClosing this configurator does not stop the engine.{}{}{}", launched.pid.unwrap(), saved.output(), stopped.output(), launched.output())
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
