mod cli;
mod diagnostics;
mod lifecycle;
mod runtime;
mod services;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match cli::execute(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("taskbarx: {error}");
            ExitCode::FAILURE
        }
    }
}
