#[cfg(windows)]
use crate::runtime::NativeBackend;
use crate::runtime::{self, Backend, Control};
use std::{path::PathBuf, process::Command};
use taskbarx_config::Settings;
use taskbarx_shell::{TrayAction, TrayIcon};

mod protocol;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunContext {
    pub engine: PathBuf,
    pub config: PathBuf,
    pub background: bool,
}

pub(crate) fn open_settings(context: &RunContext) -> Result<(), String> {
    let sibling = context.engine.with_file_name("taskbarx-configurator.exe");
    Command::new(&sibling)
        .arg("--engine")
        .arg(&context.engine)
        .arg("--config")
        .arg(&context.config)
        .current_dir(context.engine.parent().ok_or("engine has no directory")?)
        .spawn()
        .map_err(|e| format!("launching {}: {e}", sibling.display()))?;
    Ok(())
}

/// The lease outlives restoration, COM/backend Drop, tray close/Drop, and
/// notification cleanup. There is never a self-stop IPC request on this thread.
trait Coordinator {
    type Lease: Control;
    fn claim(&mut self) -> Result<Option<Self::Lease>, String>;
    fn stop_existing(&mut self, timeout_ms: u32) -> Result<(), String>;
}

trait Tray {
    fn poll(&mut self) -> Result<Vec<TrayAction>, String>;
    fn close(&mut self) -> Result<(), String>;
}
impl Tray for TrayIcon {
    fn poll(&mut self) -> Result<Vec<TrayAction>, String> {
        self.poll_actions()
    }
    fn close(&mut self) -> Result<(), String> {
        TrayIcon::close(self)
    }
}
trait RuntimeShell {
    type Tray: Tray;
    fn prepare_console(&mut self, background: bool, show: bool) -> Result<(), String>;
    fn create_tray(&mut self) -> Result<Self::Tray, String>;
    fn open_settings(&mut self, context: &RunContext) -> Result<(), String>;
}
#[cfg(windows)]
struct NativeShell;
#[cfg(windows)]
impl RuntimeShell for NativeShell {
    type Tray = TrayIcon;
    fn prepare_console(&mut self, background: bool, show: bool) -> Result<(), String> {
        windows::configure_console(background, show)
    }
    fn create_tray(&mut self) -> Result<TrayIcon, String> {
        TrayIcon::new()
    }
    fn open_settings(&mut self, context: &RunContext) -> Result<(), String> {
        open_settings(context)
    }
}

struct TrayGuard<T: Tray>(Option<T>);
impl<T: Tray> TrayGuard<T> {
    fn finish(mut self) -> Vec<String> {
        let result = self.0.as_mut().map(Tray::close).transpose();
        // On failure Drop may retry, but a successful retry cannot turn the
        // failed cleanup into a successful stop acknowledgement or restart.
        if result.is_ok() {
            self.0.take();
        }
        result
            .err()
            .into_iter()
            .map(|e| format!("tray cleanup: {e}"))
            .collect()
    }
}
impl<T: Tray> Drop for TrayGuard<T> {
    fn drop(&mut self) {
        if let Some(tray) = &mut self.0 {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tray.close())) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => runtime::warn(&format!("tray cleanup: {e}")),
                Err(_) => runtime::warn("tray cleanup panicked"),
            }
        }
    }
}

struct OwnerControl<'a, C, S: RuntimeShell> {
    lease: &'a mut C,
    tray: &'a mut Option<S::Tray>,
    shell: &'a mut S,
    context: &'a RunContext,
    stop: bool,
    restart: bool,
}
impl<C: Control, S: RuntimeShell> Control for OwnerControl<'_, C, S> {
    fn now_ms(&self) -> u64 {
        self.lease.now_ms()
    }
    fn stopped(&mut self) -> Result<bool, String> {
        self.stop |= self.lease.stopped()?;
        if self.stop {
            return Ok(true);
        }
        let actions = match self.tray.as_mut() {
            Some(tray) => tray.poll()?,
            None => vec![],
        };
        // Drain/inspect the whole batch before acting: [Restart, Stop] is STOP.
        self.stop |= actions.iter().any(|a| matches!(a, TrayAction::Stop));
        self.stop |= self.lease.stopped()?;
        if self.stop {
            return Ok(true);
        }
        self.restart |= actions.iter().any(|a| matches!(a, TrayAction::Restart));
        if !self.restart
            && actions
                .iter()
                .any(|a| matches!(a, TrayAction::OpenSettings))
        {
            self.shell.open_settings(self.context)?;
        }
        Ok(false)
    }
    fn restart_requested(&self) -> bool {
        self.restart
    }
    fn wait(&mut self, milliseconds: u64) -> Result<(), String> {
        // Bound even fake/non-Windows controls; messages wake the native waiter.
        self.lease.wait(milliseconds.min(50))
    }
}

fn start_with<C: Coordinator, B: Backend, S: RuntimeShell>(
    coordinator: &mut C,
    mut factory: impl FnMut() -> Result<B, String>,
    shell: &mut S,
    settings: &Settings,
    context: &RunContext,
) -> Result<(), String> {
    settings.validate()?;
    let mut lease = coordinator.claim()?.ok_or_else(|| {
        "taskbarx is already running for this user and session; use `taskbarx stop` first"
            .to_owned()
    })?;
    let result = (|| {
        shell.prepare_console(context.background, settings.show_console)?;
        loop {
            if lease.stopped()? {
                return Ok(());
            }
            let mut tray = TrayGuard(if settings.tray_icon {
                Some(shell.create_tray()?)
            } else {
                None
            });
            let outcome = if lease.stopped()? {
                Ok(runtime::Exit::Stop)
            } else {
                match factory() {
                    Ok(backend) => runtime::drive(
                        backend,
                        &mut OwnerControl {
                            lease: &mut lease,
                            tray: &mut tray.0,
                            shell,
                            context,
                            stop: false,
                            restart: false,
                        },
                        settings,
                    ),
                    Err(error) => Err(error),
                }
            };
            let outcome = outcome.and_then(|exit| {
                if exit == runtime::Exit::Restart {
                    // A stop may have queued while restore/COM Drop was running.
                    // Pump owner messages BEFORE inspecting the old tray queue:
                    // a native Stop callback may not have run during restoration.
                    let owner_stopped = lease.stopped()?;
                    let pending = tray
                        .0
                        .as_mut()
                        .map(Tray::poll)
                        .transpose()?
                        .unwrap_or_default();
                    if owner_stopped || pending.iter().any(|a| matches!(a, TrayAction::Stop)) {
                        return Ok(runtime::Exit::Stop);
                    }
                }
                Ok(exit)
            });
            // drive has ALREADY restored and dropped the old backend here.
            let outcome = runtime::combine_cleanup(outcome, tray.finish())?;
            if outcome == runtime::Exit::Stop || lease.stopped()? {
                return Ok(());
            }
            // Internal restart under the SAME lease with the SAME effective CLI
            // overrides and absolute config/engine context. No second runner,
            // process killing, config write/reload, or synchronous self-stop.
        }
    })();
    let completion = lease.complete(result.is_ok());
    runtime::combine_cleanup(result, completion.err().into_iter().collect())
}

pub(crate) fn run(settings: &Settings, context: &RunContext) -> Result<(), String> {
    #[cfg(windows)]
    {
        start_with(
            &mut windows::NamedCoordinator::new()?,
            NativeBackend::new,
            &mut NativeShell,
            settings,
            context,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (settings, context);
        Err("The taskbar runtime requires Windows.".into())
    }
}

pub(crate) fn stop(timeout_ms: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::NamedCoordinator::new()?.stop_existing(timeout_ms)
    }
    #[cfg(not(windows))]
    {
        let _ = timeout_ms;
        Err("The taskbar runtime requires Windows.".into())
    }
}

#[cfg(test)]
mod tests;
