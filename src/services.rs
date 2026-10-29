//! Side effects at the CLI boundary. Parser/dispatcher tests inject this entire
//! interface; no test needs to register a task, create a shortcut or run a shell.
use crate::lifecycle::RunContext;
use std::path::{Path, PathBuf};
use taskbarx_config::Settings;
use taskbarx_shell::{LaunchSpec, StartupStatus};

pub(crate) trait Services {
    fn current_exe(&mut self) -> Result<PathBuf, String>;
    fn absolute(&mut self, path: &Path) -> Result<PathBuf, String>;
    fn run(&mut self, settings: &Settings, context: &RunContext) -> Result<(), String>;
    fn stop(&mut self, timeout_ms: u32) -> Result<(), String>;
    fn diagnose(&mut self, settings: &Settings) -> Result<serde_json::Value, String>;
    fn configure(&mut self, context: &RunContext) -> Result<(), String>;
    fn startup_status(&mut self) -> Result<StartupStatus, String>;
    fn enable_startup(&mut self, launch: &LaunchSpec, delay: u32) -> Result<StartupStatus, String>;
    fn disable_startup(&mut self) -> Result<StartupStatus, String>;
    fn create_shortcut(
        &mut self,
        path: &Path,
        launch: &LaunchSpec,
        icon: Option<&Path>,
    ) -> Result<(), String>;
    fn remove_shortcut(&mut self, path: &Path) -> Result<(), String>;
    fn start_menu(&mut self) -> Result<(), String>;
}

pub(crate) struct NativeServices;
impl Services for NativeServices {
    fn current_exe(&mut self) -> Result<PathBuf, String> {
        std::env::current_exe().map_err(|e| e.to_string())
    }
    fn absolute(&mut self, path: &Path) -> Result<PathBuf, String> {
        std::path::absolute(path).map_err(|e| e.to_string())
    }
    fn run(&mut self, settings: &Settings, context: &RunContext) -> Result<(), String> {
        crate::lifecycle::run(settings, context)
    }
    fn stop(&mut self, timeout_ms: u32) -> Result<(), String> {
        crate::lifecycle::stop(timeout_ms)
    }
    fn diagnose(&mut self, settings: &Settings) -> Result<serde_json::Value, String> {
        crate::diagnostics::report(&mut crate::runtime::NativeBackend::new()?, settings)
    }
    fn configure(&mut self, context: &RunContext) -> Result<(), String> {
        crate::lifecycle::open_settings(context)
    }
    fn startup_status(&mut self) -> Result<StartupStatus, String> {
        taskbarx_shell::startup_status()
    }
    fn enable_startup(&mut self, launch: &LaunchSpec, delay: u32) -> Result<StartupStatus, String> {
        taskbarx_shell::enable_startup(launch, delay)
    }
    fn disable_startup(&mut self) -> Result<StartupStatus, String> {
        taskbarx_shell::disable_startup()
    }
    fn create_shortcut(
        &mut self,
        path: &Path,
        launch: &LaunchSpec,
        icon: Option<&Path>,
    ) -> Result<(), String> {
        taskbarx_shell::create_start_shortcut(path, launch, icon)
    }
    fn remove_shortcut(&mut self, path: &Path) -> Result<(), String> {
        taskbarx_shell::remove_start_shortcut(path)
    }
    fn start_menu(&mut self) -> Result<(), String> {
        taskbarx_shell::show_start_menu()
    }
}
