// Settings/control concepts adapted from TaskbarX (Chris Andriessen).
// See upstream-license.txt for the retained MIT notice.
//! UI text adapters only. Defaults, JSON and semantic validation belong to
//! taskbarx-config; previews use taskbarx-core and never invoke the runtime.
use std::fmt;
use std::str::FromStr;
pub use taskbarx_config::{Settings, EASING_NAMES, MEASUREMENT_BACKENDS, SUPPORTED_STYLES};
pub use taskbarx_config::{EASING_NAMES as ANIMATIONS, SUPPORTED_STYLES as STYLES};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub field: &'static str,
    pub message: String,
}
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}
impl std::error::Error for ValidationError {}
fn invalid(field: &'static str, message: impl Into<String>) -> ValidationError {
    ValidationError {
        field,
        message: message.into(),
    }
}
// Translate the shared validator's diagnostic into a focus target, not a
// second set of semantic rules. Long names precede their substrings.
fn semantic_error(message: String) -> ValidationError {
    let field = [
        "schema_version",
        "secondary_offset",
        "primary_offset",
        "battery_animation",
        "animation_ms",
        "animation",
        "battery_poll_ms",
        "poll_ms",
        "primary_opacity",
        "secondary_opacity",
        "corner_radius",
        "skip_resolutions",
        "startup_delay_secs",
        "measurement",
        "style",
    ]
    .into_iter()
    .find(|name| message.contains(name))
    .unwrap_or("settings");
    invalid(field, message)
}
fn number<T: FromStr>(text: &str, field: &'static str) -> Result<T, ValidationError> {
    text.trim()
        .parse()
        .map_err(|_| invalid(field, "enter a whole number in the field's range"))
}
fn optional_number<T: FromStr>(
    text: &str,
    field: &'static str,
) -> Result<Option<T>, ValidationError> {
    if text.trim().is_empty() {
        Ok(None)
    } else {
        number(text, field).map(Some)
    }
}

/// Text survives incomplete edits. Empty optional text means inherit/no override.
/// Every persisted field is represented; mappings deliberately have no partial
/// struct-update fallback, so adding a shared field requires adding its binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub schema_version: u32,
    pub center: bool,
    pub primary: bool,
    pub secondary: bool,
    pub center_between: bool,
    pub taskbar_segments: bool,
    pub default_style_on_maximized: bool,
    pub pause_on_fullscreen: bool,
    pub hide_primary_start: bool,
    pub hide_secondary_start: bool,
    pub hide_primary_tray: bool,
    pub hide_secondary_tray: bool,
    pub revert_on_tray_overlap: bool,
    pub toolbar_compensation: bool,
    pub tray_icon: bool,
    pub show_console: bool,
    pub primary_offset: String,
    pub secondary_offset: String,
    pub animation: String,
    pub animation_ms: String,
    pub poll_ms: String,
    pub style: String,
    pub primary_opacity: String,
    pub secondary_opacity: String,
    pub corner_radius: String,
    pub battery_animation: String,
    pub battery_poll_ms: String,
    pub startup_delay_secs: String,
    pub measurement: String,
    pub color: [String; 4],
    pub skip_resolutions: [String; 3],
}
impl Default for Form {
    fn default() -> Self {
        Settings::default().into()
    }
}
impl From<Settings> for Form {
    fn from(s: Settings) -> Self {
        Self {
            schema_version: s.schema_version,
            center: s.center,
            primary: s.primary,
            secondary: s.secondary,
            center_between: s.center_between,
            taskbar_segments: s.taskbar_segments,
            default_style_on_maximized: s.default_style_on_maximized,
            pause_on_fullscreen: s.pause_on_fullscreen,
            hide_primary_start: s.hide_primary_start,
            hide_secondary_start: s.hide_secondary_start,
            hide_primary_tray: s.hide_primary_tray,
            hide_secondary_tray: s.hide_secondary_tray,
            revert_on_tray_overlap: s.revert_on_tray_overlap,
            toolbar_compensation: s.toolbar_compensation,
            tray_icon: s.tray_icon,
            show_console: s.show_console,
            primary_offset: s.primary_offset.to_string(),
            secondary_offset: s.secondary_offset.to_string(),
            animation: s.animation,
            animation_ms: s.animation_ms.to_string(),
            poll_ms: s.poll_ms.to_string(),
            style: s.style,
            primary_opacity: s.primary_opacity.map(|v| v.to_string()).unwrap_or_default(),
            secondary_opacity: s
                .secondary_opacity
                .map(|v| v.to_string())
                .unwrap_or_default(),
            corner_radius: s.corner_radius.to_string(),
            battery_animation: s.battery_animation.unwrap_or_default(),
            battery_poll_ms: s.battery_poll_ms.map(|v| v.to_string()).unwrap_or_default(),
            startup_delay_secs: s.startup_delay_secs.to_string(),
            measurement: s.measurement,
            color: s.color.map(|v| v.to_string()),
            skip_resolutions: std::array::from_fn(|i| {
                s.skip_resolutions
                    .get(i)
                    .map(|v| v.to_string())
                    .unwrap_or_default()
            }),
        }
    }
}
impl Form {
    pub fn rgba(&self) -> Result<[u8; 4], ValidationError> {
        let mut rgba = [0; 4];
        for (i, name) in ["red", "green", "blue", "alpha"].into_iter().enumerate() {
            rgba[i] = number(&self.color[i], name)?;
        }
        Ok(rgba)
    }
    /// Validate only the delay for explicit startup enable, not the unsaved form.
    pub fn startup_delay(&self) -> Result<u32, ValidationError> {
        let s = Settings {
            startup_delay_secs: number(&self.startup_delay_secs, "startup_delay_secs")?,
            ..Settings::default()
        };
        s.validate().map_err(semantic_error)?;
        Ok(s.startup_delay_secs)
    }
    pub fn settings(&self) -> Result<Settings, ValidationError> {
        let s = Settings {
            schema_version: self.schema_version,
            center: self.center,
            primary: self.primary,
            secondary: self.secondary,
            center_between: self.center_between,
            taskbar_segments: self.taskbar_segments,
            default_style_on_maximized: self.default_style_on_maximized,
            pause_on_fullscreen: self.pause_on_fullscreen,
            hide_primary_start: self.hide_primary_start,
            hide_secondary_start: self.hide_secondary_start,
            hide_primary_tray: self.hide_primary_tray,
            hide_secondary_tray: self.hide_secondary_tray,
            revert_on_tray_overlap: self.revert_on_tray_overlap,
            toolbar_compensation: self.toolbar_compensation,
            tray_icon: self.tray_icon,
            show_console: self.show_console,
            primary_offset: number(&self.primary_offset, "primary_offset")?,
            secondary_offset: number(&self.secondary_offset, "secondary_offset")?,
            animation: self.animation.clone(),
            animation_ms: number(&self.animation_ms, "animation_ms")?,
            poll_ms: number(&self.poll_ms, "poll_ms")?,
            style: self.style.clone(),
            primary_opacity: optional_number(&self.primary_opacity, "primary_opacity")?,
            secondary_opacity: optional_number(&self.secondary_opacity, "secondary_opacity")?,
            corner_radius: number(&self.corner_radius, "corner_radius")?,
            battery_animation: if self.battery_animation.is_empty() {
                None
            } else {
                Some(self.battery_animation.clone())
            },
            battery_poll_ms: optional_number(&self.battery_poll_ms, "battery_poll_ms")?,
            startup_delay_secs: number(&self.startup_delay_secs, "startup_delay_secs")?,
            measurement: self.measurement.clone(),
            color: self.rgba()?,
            skip_resolutions: self
                .skip_resolutions
                .iter()
                .map(|v| optional_number(v, "skip_resolutions"))
                .collect::<Result<Vec<Option<u32>>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
        };
        s.validate().map_err(semantic_error)?;
        Ok(s)
    }
    pub fn preview(&self, battery: bool) -> Result<AnimationPreview, ValidationError> {
        let animation = if battery && !self.battery_animation.is_empty() {
            &self.battery_animation
        } else {
            &self.animation
        };
        // Only preview inputs are validated; unrelated incomplete edits stay put.
        let s = Settings {
            animation: animation.clone(),
            animation_ms: number(&self.animation_ms, "animation_ms")?,
            ..Settings::default()
        };
        s.validate().map_err(semantic_error)?;
        Ok(AnimationPreview {
            animation: s.animation,
            duration_ms: s.animation_ms,
        })
    }
}

/// Pure local playback, frozen at the time Preview was clicked.
#[derive(Clone, Debug)]
pub struct AnimationPreview {
    pub animation: String,
    pub duration_ms: u32,
}
impl AnimationPreview {
    pub fn sample(&self, elapsed_ms: u64) -> Result<(i32, bool), String> {
        let position = taskbarx_core::animated_position(
            0,
            1000,
            elapsed_ms,
            self.duration_ms,
            &self.animation,
        )?;
        Ok((
            position.clamp(0, 1000),
            self.animation == "none" || elapsed_ms >= u64::from(self.duration_ms),
        ))
    }
}

pub fn hex_rgba(text: &str, current_alpha: u8) -> Result<[u8; 4], ValidationError> {
    let digits = text
        .trim()
        .strip_prefix('#')
        .ok_or_else(|| invalid("color_hex", "use #RRGGBB or #RRGGBBAA"))?;
    if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid("color_hex", "use #RRGGBB or #RRGGBBAA"));
    }
    let mut rgba = [0, 0, 0, current_alpha];
    for (i, byte) in digits.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        // ASCII and exact pairs checked above; no slicing of arbitrary UTF-8.
        rgba[i] = u8::from_str_radix(std::str::from_utf8(byte).unwrap(), 16).unwrap();
    }
    Ok(rgba)
}
pub fn rgba_hex([r, g, b, a]: [u8; 4]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}
