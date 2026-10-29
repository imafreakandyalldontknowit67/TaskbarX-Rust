// Adapted from TaskbarX/TaskbarX/Easings.vb and TaskbarAnimate.vb.
// Copyright (c) 2021 Chris Andriessen. MIT license: UPSTREAM_LICENSE.txt.

use std::f64::consts::{FRAC_PI_2, TAU};

/// All 42 supported animation selections, using upstream's lower-case names.
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

/// Samples a normalized upstream easing curve.
///
/// Finite input progress is clamped to `[0, 1]`; endpoints return exactly `0` and
/// `1`. Output is **not** clamped: back/elastic overshoot is intentional. `none`
/// is a step: `0` at the start and `1` for every positive progress value.
///
/// For compatibility, upstream's unusual `sineeaseinout` is preserved: it uses
/// the same out-then-in curve as `sineeaseoutin`, not the usual cosine S-curve.
///
/// # Errors
/// Returns an error for unknown/case-mismatched names or non-finite progress.
pub fn easing(name: &str, progress: f64) -> Result<f64, String> {
    let selection = Selection::parse(name)?;
    if !progress.is_finite() {
        return Err("easing progress must be finite".to_owned());
    }
    Ok(selection.sample(progress.clamp(0.0, 1.0)))
}

/// Calculates one animation frame, without scheduling or side effects.
///
/// `none` and zero duration return `to` immediately, even at zero elapsed time.
/// Other selections start exactly at `from` and complete exactly at `to` when
/// `elapsed_ms >= duration_ms`. Names are validated even for instantaneous frames.
///
/// Interpolates in either direction using a widened signed delta. Intermediate
/// positions round to the nearest pixel (halfway away from zero). Overshoot is
/// retained except at the limits of `i32`, where the position saturates instead
/// of wrapping. The caller must additionally clamp every frame to its layout's
/// legal position range to protect reserved controls.
///
/// # Errors
/// Returns an error for an unknown/case-mismatched animation name.
pub fn animated_position(
    from: i32,
    to: i32,
    elapsed_ms: u64,
    duration_ms: u32,
    name: &str,
) -> Result<i32, String> {
    let selection = Selection::parse(name)?;
    if matches!(selection, Selection::None)
        || duration_ms == 0
        || elapsed_ms >= u64::from(duration_ms)
    {
        return Ok(to);
    }
    if elapsed_ms == 0 || from == to {
        return Ok(from);
    }

    // Here elapsed < duration <= u32::MAX, so conversion to f64 is exact.
    let progress = elapsed_ms as f64 / f64::from(duration_ms);
    let delta = i64::from(to) - i64::from(from);
    let position = f64::from(from) + delta as f64 * selection.sample(progress);
    let rounded = position
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    // Every curve is finite on [0, 1]. Bounds above make the float-to-i64 cast
    // lossless; a checked final conversion prevents accidental integer wrapping.
    i32::try_from(rounded as i64).map_err(|_| "animation position is out of range".to_owned())
}

#[derive(Clone, Copy)]
enum Selection {
    None,
    Linear,
    Curve(Family, Mode),
}

#[derive(Clone, Copy)]
enum Family {
    Quad,
    Cubic,
    Quart,
    Quint,
    Sine,
    Expo,
    Circ,
    Back,
    Elastic,
    Bounce,
}

#[derive(Clone, Copy)]
enum Mode {
    In,
    Out,
    InOut,
    OutIn,
}

impl Selection {
    fn parse(name: &str) -> Result<Self, String> {
        let unknown = || format!("unknown easing name: {name:?}");
        match name {
            "none" => return Ok(Self::None),
            "linear" => return Ok(Self::Linear),
            _ => {}
        }
        let (family, mode) = name.split_once("ease").ok_or_else(unknown)?;
        let family = match family {
            "quad" => Family::Quad,
            "cubic" => Family::Cubic,
            "quart" => Family::Quart,
            "quint" => Family::Quint,
            "sine" => Family::Sine,
            "expo" => Family::Expo,
            "circ" => Family::Circ,
            "back" => Family::Back,
            "elastic" => Family::Elastic,
            "bounce" => Family::Bounce,
            _ => return Err(unknown()),
        };
        let mode = match mode {
            "in" => Mode::In,
            "out" => Mode::Out,
            "inout" => Mode::InOut,
            "outin" => Mode::OutIn,
            _ => return Err(unknown()),
        };
        Ok(Self::Curve(family, mode))
    }

    fn sample(self, t: f64) -> f64 {
        if t == 0.0 {
            return 0.0;
        }
        if t == 1.0 {
            return 1.0;
        }
        match self {
            Self::None => 1.0,
            Self::Linear => t,
            Self::Curve(family, mode) => match mode {
                Mode::In => family.ease_in(t),
                Mode::Out => family.ease_out(t),
                Mode::InOut => family.ease_in_out(t),
                Mode::OutIn => family.ease_out_in(t),
            },
        }
    }
}

impl Family {
    fn ease_in(self, t: f64) -> f64 {
        // Also normalize endpoints when composing half-curves (notably Elastic).
        if t == 0.0 || t == 1.0 {
            return t;
        }
        match self {
            Self::Quad => t * t,
            Self::Cubic => t.powi(3),
            Self::Quart => t.powi(4),
            Self::Quint => t.powi(5),
            Self::Sine => 1.0 - (t * FRAC_PI_2).cos(),
            Self::Expo => 2.0_f64.powf(10.0 * (t - 1.0)),
            Self::Circ => 1.0 - (1.0 - t * t).sqrt(),
            Self::Back => back_in(t, 1.70158),
            Self::Elastic => elastic_in(t, 0.3),
            Self::Bounce => 1.0 - bounce_out(1.0 - t),
        }
    }

    fn ease_out(self, t: f64) -> f64 {
        if t == 0.0 || t == 1.0 {
            return t;
        }
        match self {
            Self::Quad => -t * (t - 2.0),
            Self::Cubic => (t - 1.0).powi(3) + 1.0,
            Self::Quart => 1.0 - (t - 1.0).powi(4),
            Self::Quint => (t - 1.0).powi(5) + 1.0,
            Self::Sine => (t * FRAC_PI_2).sin(),
            Self::Expo => 1.0 - 2.0_f64.powf(-10.0 * t),
            Self::Circ => (1.0 - (t - 1.0).powi(2)).sqrt(),
            Self::Back => back_out(t, 1.70158),
            Self::Elastic => elastic_out(t, 0.3),
            Self::Bounce => bounce_out(t),
        }
    }

    fn ease_in_out(self, t: f64) -> f64 {
        match self {
            // This is intentionally NOT the conventional sine in-out formula.
            // The original Easings.vb uses sine-out then sine-in for both names.
            Self::Sine => self.ease_out_in(t),
            Self::Back if t < 0.5 => 0.5 * back_in(t * 2.0, 1.70158 * 1.525),
            Self::Back => 0.5 + 0.5 * back_out(t * 2.0 - 1.0, 1.70158 * 1.525),
            // Upstream uses a longer period for the in-out elastic selection.
            Self::Elastic if t < 0.5 => 0.5 * elastic_in(t * 2.0, 0.3 * 1.5),
            Self::Elastic => 0.5 + 0.5 * elastic_out(t * 2.0 - 1.0, 0.3 * 1.5),
            _ if t < 0.5 => 0.5 * self.ease_in(t * 2.0),
            _ => 0.5 + 0.5 * self.ease_out(t * 2.0 - 1.0),
        }
    }

    fn ease_out_in(self, t: f64) -> f64 {
        if t < 0.5 {
            0.5 * self.ease_out(t * 2.0)
        } else {
            0.5 + 0.5 * self.ease_in(t * 2.0 - 1.0)
        }
    }
}

fn back_in(t: f64, overshoot: f64) -> f64 {
    t * t * ((overshoot + 1.0) * t - overshoot)
}

fn back_out(t: f64, overshoot: f64) -> f64 {
    let t = t - 1.0;
    t * t * ((overshoot + 1.0) * t + overshoot) + 1.0
}

fn elastic_in(t: f64, period: f64) -> f64 {
    let t = t - 1.0;
    -2.0_f64.powf(10.0 * t) * ((t - period / 4.0) * TAU / period).sin()
}

fn elastic_out(t: f64, period: f64) -> f64 {
    2.0_f64.powf(-10.0 * t) * ((t - period / 4.0) * TAU / period).sin() + 1.0
}

fn bounce_out(t: f64) -> f64 {
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}
