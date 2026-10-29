#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;

fn main() {
    #[cfg(windows)]
    if let Err(error) = entry() {
        native::message(&error, true);
    }
    #[cfg(not(windows))]
    {
        eprintln!("taskbarx-configurator's native UI requires Windows 10 or later.");
    }
}

#[cfg(windows)]
fn entry() -> Result<(), String> {
    use taskbarx_configurator::protocol::{Arguments, Options, HELP};
    let exe = std::env::current_exe().map_err(|e| format!("Cannot locate this executable: {e}"))?;
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Cannot resolve explicit relative paths: {e}"))?;
    match Options::parse(std::env::args_os().skip(1), &exe, &cwd)? {
        Arguments::Help => {
            native::message(HELP, false);
            Ok(())
        }
        Arguments::Options(options) => native::run(options),
    }
}
