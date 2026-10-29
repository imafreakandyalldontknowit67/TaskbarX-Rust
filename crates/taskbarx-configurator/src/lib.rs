//! Native-independent form adapters and injectable CLI protocol.
//! The only persisted settings contract lives in taskbarx-config; shell status
//! DTOs and display-only command formatting come from taskbarx-shell.
pub mod controller;
pub mod process;
pub mod protocol;
pub mod settings;

/// Attribution for the upstream settings/control concepts and easing names.
/// Included in the executable and available through its Attribution button.
pub const UPSTREAM_LICENSE: &str = include_str!("upstream-license.txt");
