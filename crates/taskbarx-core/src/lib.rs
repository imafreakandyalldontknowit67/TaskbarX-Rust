// Portions adapted from TaskbarX, Copyright (c) 2021 Chris Andriessen.
// See the complete upstream MIT notice in UPSTREAM_LICENSE.txt.

//! Pure, platform-neutral taskbar centering and animation calculations.
//!
//! The caller measures geometry, chooses the primary/secondary taskbar's offset,
//! schedules animation frames, and performs all window operations. If geometry
//! returns `None`, the caller should restore any previously moved task list.
//! Every animation frame must be clamped to [`legal_position_range`], since back
//! and elastic curves deliberately overshoot their endpoints.
//!
//! Geometry and animation are adapted from TaskbarX's `TaskbarCenter.vb`,
//! `TaskbarAnimate.vb`, and `Easings.vb` (Copyright (c) 2021 Chris Andriessen).

#![forbid(unsafe_code)]

mod animation;
mod geometry;

pub use animation::{animated_position, easing, EASING_NAMES};
pub use geometry::{center_target, legal_position_range, AxisLayout};
