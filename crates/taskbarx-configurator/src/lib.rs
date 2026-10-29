//! The form and CLI protocol are independent of Win32 rendering and sibling crates.
pub mod controller;
pub mod process;
pub mod protocol;
pub mod settings;

/// Attribution for the upstream settings/control concepts and easing names.
/// Included in the executable and available through its Attribution button.
pub const UPSTREAM_LICENSE: &str = include_str!("upstream-license.txt");
