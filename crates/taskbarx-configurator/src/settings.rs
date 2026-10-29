// Settings concepts and easing names adapted from TaskbarX (Chris Andriessen).
// See upstream-license.txt for the retained MIT notice. No easing code is needed
// here: animation is performed exclusively by the engine.
use serde::{Deserialize, Serialize};
use std::fmt;

pub const ANIMATIONS: [&str; 42] = [
    "none",
    "linear",
    "quadeasein",
    "quadeaseout",
    "quadeaseinout",
    "quadeaseoutin",
    "cubiceasein",
    "cubiceaseout",
    "cubiceaseinout",
    "cubiceaseoutin",
    "quarteasein",
    "quarteaseout",
    "quarteaseinout",
    "quarteaseoutin",
    "quinteasein",
    "quinteaseout",
    "quinteaseinout",
    "quinteaseoutin",
    "sineeasein",
    "sineeaseout",
    "sineeaseinout",
    "sineeaseoutin",
    "expoeasein",
    "expoeaseout",
    "expoeaseinout",
    "expoeaseoutin",
    "circeasein",
    "circeaseout",
    "circeaseinout",
    "circeaseoutin",
    "backeasein",
    "backeaseout",
    "backeaseinout",
    "backeaseoutin",
    "elasticeasein",
    "elasticeaseout",
    "elasticeaseinout",
    "elasticeaseoutin",
    "bounceeasein",
    "bounceeaseout",
    "bounceeaseinout",
    "bounceeaseoutin",
];
pub const STYLES: [&str; 5] = ["default", "transparent", "blur", "acrylic", "opaque"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    pub center: bool,
    pub primary: bool,
    pub secondary: bool,
    pub center_between: bool,
    pub primary_offset: i32,
    pub secondary_offset: i32,
    pub animation: String,
    pub animation_ms: u32,
    pub poll_ms: u32,
    pub style: String,
    pub color: [u8; 4],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            center: true,
            primary: true,
            secondary: true,
            center_between: false,
            primary_offset: 0,
            secondary_offset: 0,
            animation: "cubiceaseinout".into(),
            animation_ms: 300,
            poll_ms: 400,
            style: "default".into(),
            color: [0, 0, 0, 128],
        }
    }
}

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

impl Settings {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != 1 {
            return Err(invalid("schema_version", "must be 1"));
        }
        for (field, value) in [
            ("primary_offset", self.primary_offset),
            ("secondary_offset", self.secondary_offset),
        ] {
            if !(-10000..=10000).contains(&value) {
                return Err(invalid(field, "must be -10000 to 10000 physical pixels"));
            }
        }
        if !ANIMATIONS.contains(&self.animation.as_str()) {
            return Err(invalid("animation", "select a supported easing"));
        }
        if self.animation_ms > 10000 {
            return Err(invalid(
                "animation_ms",
                "must be 0 to 10000 ms (0 is instant)",
            ));
        }
        if !(50..=10000).contains(&self.poll_ms) {
            return Err(invalid("poll_ms", "must be 50 to 10000 ms"));
        }
        if !STYLES.contains(&self.style.as_str()) {
            return Err(invalid("style", "select a supported style"));
        }
        Ok(())
    }

    /// No serde defaults: malformed, partial, or newer files must not be replaced
    /// by a successful-looking default configuration.
    pub fn from_json(json: &[u8]) -> Result<Self, String> {
        let settings: Self =
            serde_json::from_slice(json).map_err(|e| format!("Invalid settings JSON: {e}"))?;
        settings.validate().map_err(|e| e.to_string())?;
        Ok(settings)
    }

    pub fn to_json(&self) -> Result<Vec<u8>, String> {
        self.validate().map_err(|e| e.to_string())?;
        serde_json::to_vec_pretty(self).map_err(|e| e.to_string())
    }
}

/// Text is preserved while editing, including temporarily incomplete negatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub center: bool,
    pub primary: bool,
    pub secondary: bool,
    pub center_between: bool,
    pub primary_offset: String,
    pub secondary_offset: String,
    pub animation: String,
    pub animation_ms: String,
    pub poll_ms: String,
    pub style: String,
    pub color: [String; 4],
}
impl Default for Form {
    fn default() -> Self {
        Settings::default().into()
    }
}
impl From<Settings> for Form {
    fn from(s: Settings) -> Self {
        Self {
            center: s.center,
            primary: s.primary,
            secondary: s.secondary,
            center_between: s.center_between,
            primary_offset: s.primary_offset.to_string(),
            secondary_offset: s.secondary_offset.to_string(),
            animation: s.animation,
            animation_ms: s.animation_ms.to_string(),
            poll_ms: s.poll_ms.to_string(),
            style: s.style,
            color: s.color.map(|v| v.to_string()),
        }
    }
}
fn integer(text: &str, field: &'static str, min: i32, max: i32) -> Result<i32, ValidationError> {
    let value: i32 = text
        .trim()
        .parse()
        .map_err(|_| invalid(field, format!("enter a whole number from {min} to {max}")))?;
    if !(min..=max).contains(&value) {
        return Err(invalid(field, format!("must be {min} to {max}")));
    }
    Ok(value)
}
impl Form {
    pub fn rgba(&self) -> Result<[u8; 4], ValidationError> {
        let mut color = [0; 4];
        for (i, name) in ["red", "green", "blue", "alpha"].iter().enumerate() {
            color[i] = integer(&self.color[i], name, 0, 255)? as u8;
        }
        Ok(color)
    }

    pub fn settings(&self) -> Result<Settings, ValidationError> {
        let s = Settings {
            schema_version: 1,
            center: self.center,
            primary: self.primary,
            secondary: self.secondary,
            center_between: self.center_between,
            primary_offset: integer(&self.primary_offset, "primary_offset", -10000, 10000)?,
            secondary_offset: integer(&self.secondary_offset, "secondary_offset", -10000, 10000)?,
            animation: self.animation.clone(),
            animation_ms: integer(&self.animation_ms, "animation_ms", 0, 10000)? as u32,
            poll_ms: integer(&self.poll_ms, "poll_ms", 50, 10000)? as u32,
            style: self.style.clone(),
            color: self.rgba()?,
        };
        s.validate()?;
        Ok(s)
    }
}
