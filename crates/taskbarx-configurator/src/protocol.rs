use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

pub const HELP: &str = "TaskbarX Configurator\n\nUsage: taskbarx-configurator.exe [--engine PATH] [--config PATH]\n\nThe default engine is taskbarx.exe beside this executable, never from PATH.\nWithout --config, the engine chooses its default config location.\nSave only persists settings. Apply/start saves, stops, then starts the engine.\nClosing this window does not stop a separately running engine.\nNo startup registration or scheduled tasks are created.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub engine: PathBuf,
    pub config: Option<PathBuf>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Arguments {
    Help,
    Options(Options),
}
impl Options {
    /// `exe` is supplied by current_exe(), not argv[0]. Relative explicit paths
    /// are made absolute so even --engine taskbarx.exe cannot search PATH.
    pub fn parse(
        args: impl IntoIterator<Item = OsString>,
        exe: &Path,
        cwd: &Path,
    ) -> Result<Arguments, String> {
        let mut engine = None;
        let mut config = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--help" || arg == "-h" {
                return Ok(Arguments::Help);
            }
            let slot = if arg == "--engine" {
                &mut engine
            } else if arg == "--config" {
                &mut config
            } else {
                return Err(format!(
                    "Unknown argument: {}\n\n{HELP}",
                    arg.to_string_lossy()
                ));
            };
            if slot.is_some() {
                return Err(format!("{} was specified twice", arg.to_string_lossy()));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{} needs a path", arg.to_string_lossy()))?;
            if value.is_empty() || value.to_string_lossy().starts_with("--") {
                return Err(format!("{} needs a nonempty path", arg.to_string_lossy()));
            }
            let value = PathBuf::from(value);
            *slot = Some(if value.is_absolute() {
                value
            } else {
                cwd.join(value)
            });
        }
        let engine = engine.unwrap_or(
            exe.parent()
                .ok_or("Cannot locate the configurator executable directory")?
                .join("taskbarx.exe"),
        );
        if !engine.is_absolute() {
            return Err("The engine path must resolve to an absolute path".into());
        }
        Ok(Arguments::Options(Self { engine, config }))
    }

    pub fn command(&self, operation: Operation) -> CommandSpec {
        let (mut args, stdin, mode, timeout) = match operation {
            Operation::Show => (
                vec!["config".into(), "show".into()],
                None,
                Mode::Wait,
                Duration::from_secs(10),
            ),
            Operation::Import(json) => (
                vec!["config".into(), "import".into(), "--stdin".into()],
                Some(json),
                Mode::Wait,
                Duration::from_secs(10),
            ),
            Operation::Run => (
                vec!["run".into()],
                None,
                Mode::Launch,
                Duration::from_millis(400),
            ),
            Operation::Stop => (
                vec!["stop".into(), "--timeout-ms".into(), "5000".into()],
                None,
                Mode::Wait,
                Duration::from_secs(7),
            ),
            Operation::Diagnose => (
                vec!["diagnose".into(), "--json".into()],
                None,
                Mode::Wait,
                Duration::from_secs(10),
            ),
        };
        // Stop is global by protocol and must NOT receive --config.
        if args[0] != "stop" {
            if let Some(path) = &self.config {
                args.push("--config".into());
                args.push(path.as_os_str().to_owned());
            }
        }
        CommandSpec {
            program: self.engine.clone(),
            args,
            stdin,
            mode,
            timeout,
        }
    }
}

pub enum Operation {
    Show,
    Import(Vec<u8>),
    Run,
    Stop,
    Diagnose,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Wait,
    Launch,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub stdin: Option<Vec<u8>>,
    /// Launch's timeout is an observation period, NOT a runtime lifetime limit.
    pub mode: Mode,
    pub timeout: Duration,
}
impl CommandSpec {
    pub fn label(&self) -> String {
        self.args
            .iter()
            .take_while(|a| *a != "--config")
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
