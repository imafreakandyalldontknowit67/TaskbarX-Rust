// Defaults and easing vocabulary adapted from TaskbarX:
// https://github.com/ChrisAnd1998/TaskbarX
//
// MIT License
//
// Copyright (c) 2021 Chris Andriessen
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use serde::{Deserialize, Serialize};

pub(crate) const MAX_JSON_BYTES: usize = 64 * 1024;

/// Exact, case-sensitive easing identifiers supported by the runtime.
pub const EASING_NAMES: &[&str] = &[
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

/// Exact, case-sensitive taskbar background styles.
pub const SUPPORTED_STYLES: &[&str] = &["default", "transparent", "blur", "acrylic", "opaque"];

/// Schema version 1. Missing JSON fields receive defaults; unknown fields fail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    pub center: bool,
    /// Select the primary taskbar for both centering and styling.
    pub primary: bool,
    /// Select secondary taskbars for both centering and styling.
    pub secondary: bool,
    pub center_between: bool,
    /// Signed physical pixels relative to the primary taskbar's center.
    pub primary_offset: i32,
    /// Signed physical pixels relative to secondary taskbars' centers.
    pub secondary_offset: i32,
    pub animation: String,
    /// Animation duration in milliseconds; zero means instant.
    pub animation_ms: u32,
    pub poll_ms: u32,
    pub style: String,
    /// Red, green, blue, alpha, each in 0..=255 (not percentage opacity).
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

impl Settings {
    /// Check semantic limits without normalizing or changing any values.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported schema_version {}; expected 1",
                self.schema_version
            ));
        }
        for (name, offset) in [
            ("primary_offset", self.primary_offset),
            ("secondary_offset", self.secondary_offset),
        ] {
            if !(-10_000..=10_000).contains(&offset) {
                return Err(format!(
                    "{name} must be between -10000 and 10000 physical pixels"
                ));
            }
        }
        if !EASING_NAMES.contains(&self.animation.as_str()) {
            return Err(format!(
                "unknown animation {:?}; use an EASING_NAMES value",
                self.animation
            ));
        }
        if self.animation_ms > 10_000 {
            return Err("animation_ms must be between 0 and 10000 milliseconds".into());
        }
        if !(50..=10_000).contains(&self.poll_ms) {
            return Err("poll_ms must be between 50 and 10000 milliseconds".into());
        }
        if !SUPPORTED_STYLES.contains(&self.style.as_str()) {
            return Err(format!(
                "unknown style {:?}; expected default, transparent, blur, acrylic, or opaque",
                self.style
            ));
        }
        // [u8; 4] enforces the RGBA shape and range, including during serde input.
        Ok(())
    }
}

/// Parse and validate at most 64 KiB of UTF-8 JSON. Invalid input is never reset.
pub fn from_json(text: &str) -> Result<Settings, String> {
    if text.len() > MAX_JSON_BYTES {
        return Err("settings JSON exceeds the 64 KiB (65536 byte) limit".into());
    }
    // Serde's derived struct visitor also accepts positional sequences; settings
    // files deliberately require a JSON object with the documented field names.
    if !text.trim_start().starts_with('{') {
        return Err("settings JSON must be an object".into());
    }
    let settings: Settings =
        serde_json::from_str(text).map_err(|error| format!("invalid settings JSON: {error}"))?;
    settings.validate()?;
    Ok(settings)
}

/// Validate and produce human-readable JSON with a final newline.
pub fn to_json(settings: &Settings) -> Result<String, String> {
    settings.validate()?;
    let mut text = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("could not serialize settings: {error}"))?;
    text.push('\n');
    Ok(text)
}
