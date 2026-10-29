//! Validated, versioned JSON settings, independent of the UI and taskbar runtime.
//!
//! Loading and resolving the default path never write anything. Saving is an
//! explicit operation and does not register startup tasks or change a taskbar.
//! Use [`from_json`] and [`to_json`] rather than raw serde calls when validation
//! is required: the derived serde implementations enforce the data shape, while
//! these functions also enforce semantic limits.
//!
//! Defaults and easing names are adapted from Chris Andriessen's TaskbarX
//! (`Settings.vb`, `Main.vb`, `Easings.vb`, and the Configurator). Its MIT notice
//! is retained in the settings module.

mod persistence;
mod settings;

pub use persistence::{default_path, load, save_atomic};
pub use settings::{
    from_json, to_json, Settings, EASING_NAMES, MEASUREMENT_BACKENDS, SUPPORTED_STYLES,
};

#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod test_support;
