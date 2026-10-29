use crate::{
    lifecycle::RunContext,
    services::{NativeServices, Services},
};
use clap::{Args, CommandFactory, Parser, Subcommand};
use std::{
    io::{Read, Write},
    path::PathBuf,
    str::FromStr,
};
use taskbarx_config::{self as config, Settings};
use taskbarx_shell::LaunchSpec;

#[derive(Parser)]
#[command(
    name = "taskbarx",
    version,
    about = "Reversible classic Windows taskbar centering",
    after_help = "No command makes no changes. Only `run` applies taskbar settings. Startup and shortcuts require explicit commands. Graceful shutdown restores touched taskbars; force-kill and power loss cannot be recovered automatically."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run in the foreground until stopped or interrupted (overrides are not saved).
    Run(RunArgs),
    /// Request restoration and exit of this user's runtime in this session.
    Stop {
        #[arg(long, default_value_t = 5000, value_parser = clap::value_parser!(u32).range(1..=60000))]
        timeout_ms: u32,
    },
    /// Read-only topology, measurements, compatibility, and proposed positions.
    Diagnose {
        #[arg(long)]
        json: bool,
        #[command(flatten)]
        path: ConfigPath,
    },
    /// Read or import settings without applying them.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Open the sibling configurator (does not start the runtime).
    Configure(ConfigPath),
    /// Inspect or explicitly change startup registration (never starts the engine).
    Startup {
        #[command(subcommand)]
        command: StartupCommand,
    },
    /// Show the Start menu once; does not load settings or start the runtime.
    StartMenu,
    /// Manage an explicit Start-menu shortcut; never runs its target.
    Shortcut {
        #[command(subcommand)]
        command: ShortcutCommand,
    },
}

#[derive(Args, Default)]
struct ConfigPath {
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,
}

impl ConfigPath {
    fn resolve(&self) -> Result<PathBuf, String> {
        self.config
            .clone()
            .map(Ok)
            .unwrap_or_else(config::default_path)
    }

    fn load(&self) -> Result<Settings, String> {
        let path = self.resolve()?;
        // Missing files mean in-memory defaults, not a write or directory creation.
        // Do not hide permission errors or a directory supplied as a config file.
        let settings = match std::fs::metadata(&path) {
            Ok(_) => config::load(&path)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(format!("cannot inspect {}: {e}", path.display())),
        };
        settings.validate()?;
        Ok(settings)
    }
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print only validated settings JSON; do not create missing files.
    Show(ConfigPath),
    /// Validate up to 64 KiB of UTF-8 JSON and atomically save it; never apply it.
    Import {
        #[arg(long, required = true)]
        stdin: bool,
        #[command(flatten)]
        path: ConfigPath,
    },
}

#[derive(Subcommand)]
enum StartupCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Enable {
        #[command(flatten)]
        path: ConfigPath,
        #[arg(long, value_parser = clap::value_parser!(u32).range(0..=86400))]
        delay_secs: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    Disable {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum ShortcutCommand {
    Create {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        icon: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    Remove {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        json: bool,
    },
}

/// Distinguish absence (keep config) from explicit inheritance/default (None).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Nullable<T> {
    Inherit,
    Value(T),
}
impl<T: FromStr> FromStr for Nullable<T>
where
    T::Err: std::fmt::Display,
{
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        if s == "inherit" || s == "default" {
            Ok(Self::Inherit)
        } else {
            s.parse()
                .map(Self::Value)
                .map_err(|e| format!("invalid override: {e}"))
        }
    }
}
impl<T: Clone> Nullable<T> {
    fn value(&self) -> Option<T> {
        match self {
            Self::Inherit => None,
            Self::Value(v) => Some(v.clone()),
        }
    }
}

#[derive(Args, Default)]
struct RunArgs {
    #[command(flatten)]
    path: ConfigPath,
    #[arg(long, allow_hyphen_values = true)]
    primary_offset: Option<i32>,
    #[arg(long, allow_hyphen_values = true)]
    secondary_offset: Option<i32>,
    #[arg(long)]
    animation: Option<String>,
    #[arg(long)]
    animation_ms: Option<u32>,
    #[arg(long)]
    poll_ms: Option<u32>,
    #[arg(long)]
    style: Option<String>,
    #[arg(long, value_name = "R,G,B,A", value_parser = parse_color)]
    color: Option<[u8; 4]>,
    #[arg(long)]
    no_center: bool,
    #[arg(long, conflicts_with = "only_secondary")]
    only_primary: bool,
    #[arg(long, conflicts_with = "only_primary")]
    only_secondary: bool,
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    center_between: Option<bool>,
    /// Detach this process from its console, unless show-console is true.
    #[arg(long)]
    background: bool,
    #[arg(long, conflicts_with = "no_center")]
    center: Option<bool>,
    #[arg(long, conflicts_with_all = ["only_primary", "only_secondary"])]
    primary: Option<bool>,
    #[arg(long, conflicts_with_all = ["only_primary", "only_secondary"])]
    secondary: Option<bool>,
    /// Percentage 0..100, or default/inherit to remove the override.
    #[arg(long, value_name = "PERCENT|default")]
    primary_opacity: Option<Nullable<u8>>,
    #[arg(long, value_name = "PERCENT|default")]
    secondary_opacity: Option<Nullable<u8>>,
    #[arg(long)]
    corner_radius: Option<u32>,
    #[arg(long)]
    taskbar_segments: Option<bool>,
    #[arg(long)]
    default_style_on_maximized: Option<bool>,
    #[arg(long, value_name = "EASING|inherit")]
    battery_animation: Option<Nullable<String>>,
    #[arg(long, value_name = "MS|inherit")]
    battery_poll_ms: Option<Nullable<u32>>,
    /// Replace configured widths; repeat up to three times. Uses primary monitor width.
    #[arg(long = "skip-resolution")]
    skip_resolutions: Vec<u32>,
    #[arg(long, conflicts_with = "skip_resolutions")]
    clear_skip_resolutions: bool,
    #[arg(long)]
    pause_on_fullscreen: Option<bool>,
    #[arg(long)]
    hide_primary_start: Option<bool>,
    #[arg(long)]
    hide_secondary_start: Option<bool>,
    #[arg(long)]
    hide_primary_tray: Option<bool>,
    #[arg(long)]
    hide_secondary_tray: Option<bool>,
    #[arg(long)]
    revert_on_tray_overlap: Option<bool>,
    #[arg(long)]
    toolbar_compensation: Option<bool>,
    #[arg(long)]
    tray_icon: Option<bool>,
    #[arg(long, value_parser = ["msaa", "uia"])]
    measurement: Option<String>,
    #[arg(long)]
    show_console: Option<bool>,
}

fn parse_color(text: &str) -> Result<[u8; 4], String> {
    let values: Result<Vec<u8>, _> = text.split(',').map(str::parse).collect();
    values
        .map_err(|_| "color must be four comma-separated integers in 0..255".to_owned())?
        .try_into()
        .map_err(|_| "color must have exactly four components: R,G,B,A".to_owned())
}

impl RunArgs {
    fn settings(&self) -> Result<Settings, String> {
        let mut s = self.path.load()?;
        if let Some(v) = self.primary_offset {
            s.primary_offset = v;
        }
        if let Some(v) = self.secondary_offset {
            s.secondary_offset = v;
        }
        if let Some(v) = &self.animation {
            s.animation = v.clone();
        }
        if let Some(v) = self.animation_ms {
            s.animation_ms = v;
        }
        if let Some(v) = self.poll_ms {
            s.poll_ms = v;
        }
        if let Some(v) = &self.style {
            s.style = v.clone();
        }
        if let Some(v) = self.color {
            s.color = v;
        }
        if self.no_center {
            s.center = false;
        }
        if self.only_primary {
            s.primary = true;
            s.secondary = false;
        }
        if self.only_secondary {
            s.primary = false;
            s.secondary = true;
        }
        macro_rules! assign {
            ($($field:ident),* $(,)?) => { $(if let Some(v) = &self.$field { s.$field = v.clone(); })* };
        }
        assign!(
            center_between,
            center,
            primary,
            secondary,
            corner_radius,
            taskbar_segments,
            default_style_on_maximized,
            pause_on_fullscreen,
            hide_primary_start,
            hide_secondary_start,
            hide_primary_tray,
            hide_secondary_tray,
            revert_on_tray_overlap,
            toolbar_compensation,
            tray_icon,
            measurement,
            show_console
        );
        if let Some(v) = &self.primary_opacity {
            s.primary_opacity = v.value();
        }
        if let Some(v) = &self.secondary_opacity {
            s.secondary_opacity = v.value();
        }
        if let Some(v) = &self.battery_animation {
            s.battery_animation = v.value();
        }
        if let Some(v) = &self.battery_poll_ms {
            s.battery_poll_ms = v.value();
        }
        if !self.skip_resolutions.is_empty() {
            s.skip_resolutions.clone_from(&self.skip_resolutions);
        }
        if self.clear_skip_resolutions {
            s.skip_resolutions.clear();
        }
        s.validate()?;
        Ok(s)
    }
}

fn import(mut input: impl Read, path: &ConfigPath) -> Result<(), String> {
    const LIMIT: u64 = 64 * 1024;
    let mut bytes = Vec::new();
    input
        .by_ref()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("reading stdin: {e}"))?;
    if bytes.len() as u64 > LIMIT {
        return Err("stdin exceeds the 64 KiB limit".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("stdin is not UTF-8: {e}"))?;
    let settings = config::from_json(text)?;
    settings.validate()?;
    config::save_atomic(&path.resolve()?, &settings)
}

fn context(
    services: &mut impl Services,
    path: &ConfigPath,
    background: bool,
) -> Result<RunContext, String> {
    Ok(RunContext {
        engine: services.current_exe()?,
        config: services.absolute(&path.resolve()?)?,
        background,
    })
}

fn launch(executable: PathBuf, args: Vec<String>) -> Result<LaunchSpec, String> {
    if !executable.is_absolute() {
        return Err("engine path must be absolute".into());
    }
    let working_directory = executable
        .parent()
        .ok_or("engine has no directory")?
        .to_owned();
    Ok(LaunchSpec {
        executable,
        args,
        working_directory,
    })
}

fn startup_launch(context: RunContext) -> Result<LaunchSpec, String> {
    if !context.config.is_absolute() {
        return Err("config path must be absolute".into());
    }
    // The frozen shell contract uses String, so reject unpaired UTF-16 rather
    // than silently register a different lossy path. Never prequote argv items.
    let config = context
        .config
        .to_str()
        .ok_or("config path is not valid Unicode")?
        .to_owned();
    launch(
        context.engine,
        vec![
            "run".into(),
            "--background".into(),
            "--config".into(),
            config,
        ],
    )
}

pub(crate) fn execute(cli: Cli) -> Result<(), String> {
    execute_with(
        cli,
        &mut NativeServices,
        std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
    )
}

fn print(out: &mut impl Write, text: String) -> Result<(), String> {
    writeln!(out, "{text}").map_err(|e| e.to_string())
}

fn execute_with(
    cli: Cli,
    services: &mut impl Services,
    input: impl Read,
    out: &mut impl Write,
) -> Result<(), String> {
    match cli.command {
        None => {
            Cli::command().write_help(out).map_err(|e| e.to_string())?;
            print(out, String::new())
        }
        Some(Commands::Run(args)) => {
            let settings = args.settings()?;
            let context = context(services, &args.path, args.background)?;
            services.run(&settings, &context)
        }
        Some(Commands::Stop { timeout_ms }) => services.stop(timeout_ms),
        Some(Commands::Diagnose { json, path }) => {
            let report = services.diagnose(&path.load()?)?;
            let text = if json {
                serde_json::to_string(&report)
            } else {
                serde_json::to_string_pretty(&report)
            };
            print(out, text.map_err(|e| e.to_string())?)
        }
        Some(Commands::Config {
            command: ConfigCommand::Show(path),
        }) => print(out, config::to_json(&path.load()?)?),
        Some(Commands::Config {
            command: ConfigCommand::Import { path, .. },
        }) => import(input, &path),
        Some(Commands::Configure(path)) => {
            let context = context(services, &path, false)?;
            services.configure(&context)
        }
        Some(Commands::Startup { command }) => {
            let (status, json) = match command {
                StartupCommand::Status { json } => (services.startup_status()?, json),
                StartupCommand::Disable { json } => (services.disable_startup()?, json),
                StartupCommand::Enable {
                    path,
                    delay_secs,
                    json,
                } => {
                    let settings = path.load()?;
                    let spec = startup_launch(context(services, &path, true)?)?;
                    (
                        services.enable_startup(
                            &spec,
                            delay_secs.unwrap_or(settings.startup_delay_secs),
                        )?,
                        json,
                    )
                }
            };
            // JSON is the frozen StartupStatus itself, not a success wrapper.
            let text = if json {
                serde_json::to_string(&status)
            } else {
                serde_json::to_string_pretty(&status)
            };
            print(out, text.map_err(|e| e.to_string())?)
        }
        Some(Commands::StartMenu) => services.start_menu(),
        Some(Commands::Shortcut { command }) => {
            let (path, json) = match command {
                ShortcutCommand::Create { path, icon, json } => {
                    let spec = launch(services.current_exe()?, vec!["start-menu".into()])?;
                    services.create_shortcut(&path, &spec, icon.as_deref())?;
                    (path, json)
                }
                ShortcutCommand::Remove { path, json } => {
                    services.remove_shortcut(&path)?;
                    (path, json)
                }
            };
            if json {
                print(
                    out,
                    serde_json::json!({"ok": true, "path": path}).to_string(),
                )
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_offsets_and_selection_contract() {
        let cli = Cli::try_parse_from([
            "taskbarx",
            "run",
            "--primary-offset",
            "-45",
            "--secondary-offset",
            "-9",
            "--only-primary",
        ])
        .unwrap();
        let Some(Commands::Run(args)) = cli.command else {
            panic!("not run")
        };
        assert_eq!(args.primary_offset, Some(-45));
        assert_eq!(args.secondary_offset, Some(-9));
        assert!(args.only_primary);
        assert!(
            Cli::try_parse_from(["taskbarx", "run", "--only-primary", "--only-secondary"]).is_err()
        );
    }

    #[test]
    fn color_is_strict() {
        assert_eq!(parse_color("0,1,255,128").unwrap(), [0, 1, 255, 128]);
        for invalid in ["1,2,3", "256,0,0,0", "-1,0,0,0", "0,0,0,0,0"] {
            assert!(parse_color(invalid).is_err());
        }
    }

    #[test]
    fn overrides_are_validated_without_creating_or_persisting_config() {
        let path = std::env::temp_dir().join(format!(
            "taskbarx-missing-override-{}.json",
            std::process::id()
        ));
        assert!(!path.exists());
        let cli = Cli::try_parse_from([
            "taskbarx",
            "run",
            "--config",
            path.to_str().unwrap(),
            "--primary-offset",
            "-42",
            "--secondary-offset",
            "-12",
            "--animation",
            "linear",
            "--animation-ms",
            "100",
            "--poll-ms",
            "500",
            "--style",
            "opaque",
            "--color",
            "1,2,3,4",
            "--only-secondary",
            "--no-center",
            "--center-between",
        ])
        .unwrap();
        let Some(Commands::Run(mut args)) = cli.command else {
            panic!("not run")
        };
        let s = args.settings().unwrap();
        assert!(!s.center && !s.primary && s.secondary && s.center_between);
        assert_eq!((s.primary_offset, s.secondary_offset), (-42, -12));
        assert_eq!((s.animation_ms, s.poll_ms), (100, 500));
        assert_eq!(s.animation, "linear");
        assert_eq!(s.style, "opaque");
        assert_eq!(s.color, [1, 2, 3, 4]);
        args.animation = Some("not-an-easing".into());
        assert!(args.settings().is_err());
        assert!(!path.exists());
    }

    #[test]
    fn clap_contract_is_consistent() {
        Cli::command().debug_assert();
    }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod dispatcher_tests;
