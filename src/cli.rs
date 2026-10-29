use clap::{Args, CommandFactory, Parser, Subcommand};
use std::{io::Read, path::PathBuf, process::Command};
use taskbarx_config::{self as config, Settings};

#[derive(Parser)]
#[command(
    name = "taskbarx",
    version,
    about = "Reversible classic Windows taskbar centering",
    after_help = "No command makes no changes. Only `run` applies settings. Graceful shutdown restores touched taskbars; force-kill and power loss cannot be recovered automatically."
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
    #[arg(long)]
    center_between: bool,
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
        if self.center_between {
            s.center_between = true;
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

pub(crate) fn execute(cli: Cli) -> Result<(), String> {
    match cli.command {
        None => {
            Cli::command().print_help().map_err(|e| e.to_string())?;
            println!();
            Ok(())
        }
        Some(Commands::Run(args)) => crate::lifecycle::run(&args.settings()?),
        Some(Commands::Stop { timeout_ms }) => crate::lifecycle::stop(timeout_ms),
        Some(Commands::Diagnose { json, path }) => {
            let settings = path.load()?;
            let mut backend = crate::runtime::NativeBackend::new()?;
            let report = crate::diagnostics::report(&mut backend, &settings)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string(&report).map_err(|e| e.to_string())?
                );
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
                );
            }
            Ok(())
        }
        Some(Commands::Config {
            command: ConfigCommand::Show(path),
        }) => {
            println!("{}", config::to_json(&path.load()?)?);
            Ok(())
        }
        Some(Commands::Config {
            command: ConfigCommand::Import { path, .. },
        }) => import(std::io::stdin().lock(), &path),
        Some(Commands::Configure(path)) => {
            let engine = std::env::current_exe().map_err(|e| e.to_string())?;
            let sibling = engine.with_file_name("taskbarx-configurator.exe");
            let mut child = Command::new(&sibling);
            child.arg("--engine").arg(&engine);
            if let Some(path) = path.config {
                child.arg("--config").arg(path);
            }
            child
                .spawn()
                .map_err(|e| format!("launching {}: {e}", sibling.display()))?;
            Ok(())
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
