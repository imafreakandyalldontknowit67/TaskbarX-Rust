//! Lifecycle/centering behavior adapted from TaskbarX Main.vb, TaskbarCenter.vb,
//! TaskbarAnimate.vb and Settings.vb, not their process-killing or busy loops.
//!
//! MIT License
//! Copyright (c) 2021 Chris Andriessen
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy
//! of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights
//! to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.

use std::{
    collections::{HashMap, HashSet},
    io::Write,
};
use taskbarx_config::Settings;
use taskbarx_core::{
    animated_position, center_target_with_overlap, legal_position_range, AxisLayout,
};
use taskbarx_win32::{
    Discovery, EnvironmentSnapshot, MeasurementBackend, MonitorState, Point, Rect, Session, Style,
    TaskbarFeatures, TaskbarId, TaskbarSnapshot,
};

const FRAME_MS: u64 = 16;
const STYLE_MS: u64 = 2000;
// Environment observation never traverses MSAA/UIA. Independent of poll_ms so a
// 10-second accessibility interval cannot defer fullscreen, power or tray events.
const ENVIRONMENT_MS: u64 = 100;
pub(crate) const MODERN_TASKBAR_ERROR: &str = "The native modern Windows 11 taskbar is not supported. No changes were applied to it. This runtime requires the classic taskbar window hierarchy.";

pub(crate) trait Backend {
    fn environment(&mut self) -> Result<EnvironmentSnapshot, String>;
    fn set_measurement_backend(&mut self, provider: MeasurementBackend) -> Result<(), String>;
    fn discover(&mut self) -> Result<Discovery, String>;
    fn move_list(&mut self, id: &TaskbarId, position: Point) -> Result<(), String>;
    fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String>;
    fn apply_features(
        &mut self,
        id: &TaskbarId,
        features: &TaskbarFeatures,
    ) -> Result<bool, String>;
    fn restore_taskbar(&mut self, id: &TaskbarId) -> Vec<String>;
    fn restore(&mut self) -> Vec<String>;
}

pub(crate) struct NativeBackend(Session);
impl NativeBackend {
    pub(crate) fn new() -> Result<Self, String> {
        Session::new().map(Self)
    }
}
impl Backend for NativeBackend {
    fn environment(&mut self) -> Result<EnvironmentSnapshot, String> {
        self.0.environment()
    }
    fn set_measurement_backend(&mut self, provider: MeasurementBackend) -> Result<(), String> {
        self.0.set_measurement_backend(provider)
    }
    fn discover(&mut self) -> Result<Discovery, String> {
        self.0.discover()
    }
    fn move_list(&mut self, id: &TaskbarId, position: Point) -> Result<(), String> {
        self.0.move_task_list(id, position)
    }
    fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String> {
        self.0.restore_position(id)
    }
    fn apply_features(
        &mut self,
        id: &TaskbarId,
        features: &TaskbarFeatures,
    ) -> Result<bool, String> {
        self.0.apply_features(id, features)
    }
    fn restore_taskbar(&mut self, id: &TaskbarId) -> Vec<String> {
        self.0.restore_taskbar(id)
    }
    fn restore(&mut self) -> Vec<String> {
        self.0.restore()
    }
}

/// Monotonic clock plus interruptible waiting. The Windows implementation also
/// pumps owner-thread COM messages and observes shutdown notifications.
pub(crate) trait Control {
    fn now_ms(&self) -> u64;
    fn complete(&mut self, _success: bool) -> Result<(), String> {
        Ok(())
    }
    fn stopped(&mut self) -> Result<bool, String>;
    fn restart_requested(&self) -> bool {
        false
    }
    fn wait(&mut self, milliseconds: u64) -> Result<(), String>;
}

pub(crate) fn warn(message: &str) {
    // Logging during unwinding must not itself panic if stderr is closed.
    let _ = writeln!(std::io::stderr().lock(), "taskbarx: {message}");
}

struct RestoreGuard<B: Backend> {
    backend: B,
    armed: bool,
}
impl<B: Backend> RestoreGuard<B> {
    fn finish<T>(mut self, result: Result<T, String>) -> Result<T, String> {
        let failures = self.backend.restore();
        self.armed = false;
        combine_cleanup(result, failures)
    }
}
impl<B: Backend> Drop for RestoreGuard<B> {
    fn drop(&mut self) {
        if self.armed {
            // Session is also a last-resort RAII guard. Do not suppress the
            // original panic if an injected backend's cleanup panics.
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.backend.restore()))
            {
                Ok(errors) => {
                    for error in errors {
                        warn(&format!("restoration: {error}"));
                    }
                }
                Err(_) => warn("restoration panicked while unwinding"),
            }
        }
    }
}

pub(crate) fn combine_cleanup<T>(
    result: Result<T, String>,
    failures: Vec<String>,
) -> Result<T, String> {
    if failures.is_empty() {
        return result;
    }
    let cleanup = format!("restoration failed: {}", failures.join("; "));
    Err(match result {
        Ok(_) => cleanup,
        Err(original) => format!("{original}; {cleanup}"),
    })
}

fn valid_rect(r: &Rect) -> bool {
    r.left < r.right && r.top < r.bottom
}
fn bounds(r: &Rect, vertical: bool) -> (i32, i32) {
    if vertical {
        (r.top, r.bottom)
    } else {
        (r.left, r.right)
    }
}

pub(crate) fn selected(s: &TaskbarSnapshot, settings: &Settings) -> bool {
    if s.primary {
        settings.primary
    } else {
        settings.secondary
    }
}

/// All coordinates stay signed physical pixels, including negative monitors.
pub(crate) fn layout(s: &TaskbarSnapshot) -> Option<AxisLayout> {
    let content = s.content_rect.as_ref()?;
    for rect in [
        &s.taskbar_rect,
        &s.parent_rect,
        &s.list_rect,
        &s.available_rect,
        content,
    ] {
        if !valid_rect(rect) {
            return None;
        }
    }
    let (bar_start, bar_end) = bounds(&s.taskbar_rect, s.vertical);
    let (free_start, free_end) = bounds(&s.available_rect, s.vertical);
    let (parent_start, _) = bounds(&s.parent_rect, s.vertical);
    let (list_start, _) = bounds(&s.list_rect, s.vertical);
    let (content_start, content_end) = bounds(content, s.vertical);
    Some(AxisLayout {
        bar_start,
        bar_end,
        free_start,
        free_end,
        parent_start,
        content_offset: content_start.checked_sub(list_start)?,
        content_len: content_end.checked_sub(content_start)?,
    })
}

pub(crate) fn proposal(
    s: &TaskbarSnapshot,
    settings: &Settings,
) -> Option<(AxisLayout, i32, (i32, i32))> {
    if !settings.center || !selected(s, settings) {
        return None;
    }
    let layout = layout(s)?;
    let range = legal_position_range(&layout)?;
    if range.0 > range.1 {
        return None;
    }
    let offset = if s.primary {
        settings.primary_offset
    } else {
        settings.secondary_offset
    };
    let target = center_target_with_overlap(
        &layout,
        offset,
        settings.center_between,
        settings.revert_on_tray_overlap,
    )?;
    Some((layout, target, range))
}

fn style(name: &str) -> Result<Style, String> {
    match name {
        "default" => Ok(Style::Default),
        "transparent" => Ok(Style::Transparent),
        "transparent-gradient" => Ok(Style::TransparentGradient),
        "blur" => Ok(Style::Blur),
        "acrylic" => Ok(Style::Acrylic),
        "opaque" => Ok(Style::Opaque),
        _ => Err(format!("unsupported taskbar style: {name}")),
    }
}

pub(crate) fn measurement(name: &str) -> Result<MeasurementBackend, String> {
    match name {
        "msaa" => Ok(MeasurementBackend::Msaa),
        "uia" => Ok(MeasurementBackend::UiAutomation),
        _ => Err(format!("unsupported measurement provider: {name}")),
    }
}

fn effective_settings(settings: &Settings, env: &EnvironmentSnapshot) -> Settings {
    let mut effective = settings.clone();
    // Unknown power is NOT battery. Nullable overrides inherit ordinary values.
    if env.on_battery == Some(true) {
        if let Some(animation) = &settings.battery_animation {
            effective.animation.clone_from(animation);
        }
        effective.poll_ms = settings.battery_poll_ms.unwrap_or(settings.poll_ms);
    }
    // Unlike fullscreen pause, skip-resolution affects centering only. Upstream
    // compares PRIMARY monitor WIDTH, including for vertical/secondary taskbars;
    // it does not compare each selected taskbar's own width or long axis.
    if env.monitors.iter().any(|m| {
        m.primary
            && u32::try_from(i64::from(m.bounds.right) - i64::from(m.bounds.left))
                .is_ok_and(|width| settings.skip_resolutions.contains(&width))
    }) {
        effective.center = false;
    }
    effective
}

fn monitor_for<'a>(s: &TaskbarSnapshot, env: &'a EnvironmentSnapshot) -> Option<&'a MonitorState> {
    if !valid_rect(&s.taskbar_rect) {
        return None;
    }
    env.monitors
        .iter()
        .filter_map(|m| {
            if !valid_rect(&m.bounds) {
                return None;
            }
            let width = i64::from(s.taskbar_rect.right.min(m.bounds.right))
                - i64::from(s.taskbar_rect.left.max(m.bounds.left));
            let height = i64::from(s.taskbar_rect.bottom.min(m.bounds.bottom))
                - i64::from(s.taskbar_rect.top.max(m.bounds.top));
            let area = i128::from(width.max(0)) * i128::from(height.max(0));
            // Auto-hidden taskbars may sit just outside the monitor. Like
            // MonitorFromRect(NEAREST), fall back to the nearest monitor rectangle,
            // not blindly to the primary display. Widen squared distances too.
            let dx = i128::from(width.min(0));
            let dy = i128::from(height.min(0));
            Some(((area, -(dx * dx + dy * dy), m.primary == s.primary), m))
        })
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, m)| m)
}

fn features(
    s: &TaskbarSnapshot,
    settings: &Settings,
    env: Option<&EnvironmentSnapshot>,
) -> Result<TaskbarFeatures, String> {
    let maximized = settings.default_style_on_maximized
        && env
            .and_then(|env| monitor_for(s, env))
            .is_some_and(|m| m.maximized);
    Ok(TaskbarFeatures {
        style: if maximized {
            Style::Default
        } else {
            style(&settings.style)?
        },
        color: settings.color,
        opacity: if s.primary {
            settings.primary_opacity
        } else {
            settings.secondary_opacity
        },
        corner_radius: settings.corner_radius,
        segments: settings.taskbar_segments,
        hide_start: if s.primary {
            settings.hide_primary_start
        } else {
            settings.hide_secondary_start
        },
        hide_tray: if s.primary {
            settings.hide_primary_tray
        } else {
            settings.hide_secondary_tray
        },
        toolbar_compensation: settings.toolbar_compensation,
    })
}

// Do not require extra derives beyond the frozen backend contract.
fn same_features(a: &TaskbarFeatures, b: &TaskbarFeatures) -> bool {
    a.style == b.style
        && a.color == b.color
        && a.opacity == b.opacity
        && a.corner_radius == b.corner_radius
        && a.segments == b.segments
        && a.hide_start == b.hide_start
        && a.hide_tray == b.hide_tray
        && a.toolbar_compensation == b.toolbar_compensation
}

fn same_monitors(a: &EnvironmentSnapshot, b: &EnvironmentSnapshot) -> bool {
    a.monitors.len() == b.monitors.len()
        && a.monitors.iter().zip(&b.monitors).all(|(a, b)| {
            a.bounds == b.bounds
                && a.primary == b.primary
                && a.maximized == b.maximized
                && a.fullscreen == b.fullscreen
        })
}

#[derive(PartialEq, Eq)]
struct Geometry {
    axis: [i32; 7],
    parent: [i32; 4],
    vertical: bool,
    dpi: u32,
}
impl Geometry {
    fn reservations_changed(&self, other: &Self) -> bool {
        self.parent != other.parent
            || self.axis[..5] != other.axis[..5]
            || self.vertical != other.vertical
    }
    fn new(s: &TaskbarSnapshot, a: &AxisLayout) -> Self {
        Self {
            axis: [
                a.bar_start,
                a.bar_end,
                a.free_start,
                a.free_end,
                a.parent_start,
                a.content_offset,
                a.content_len,
            ],
            parent: [
                s.parent_rect.left,
                s.parent_rect.top,
                s.parent_rect.right,
                s.parent_rect.bottom,
            ],
            vertical: s.vertical,
            dpi: s.dpi,
        }
    }
}

struct Animation {
    geometry: Geometry,
    from: i32,
    target: i32,
    displayed: i32,
    cross: i32,
    range: (i32, i32),
    started: u64,
    stabilize: bool,
}

#[derive(Default)]
struct State {
    animations: HashMap<TaskbarId, Animation>,
    styles: HashMap<TaskbarId, (TaskbarFeatures, u64)>,
    moved: HashSet<TaskbarId>,
    environment: Option<EnvironmentSnapshot>,
    needs_measurement: bool,
    stabilize: HashSet<TaskbarId>,
}

impl State {
    fn refresh(
        &mut self,
        backend: &mut impl Backend,
        discovery: Discovery,
        settings: &Settings,
        now: u64,
    ) -> Result<bool, String> {
        // This check precedes ALL mutation, including style application.
        if discovery.native_windows11 {
            return Err(MODERN_TASKBAR_ERROR.into());
        }
        self.needs_measurement = false;
        let ids: HashSet<_> = discovery.taskbars.iter().map(|s| s.id).collect();
        // Never reuse an animation across an HWND/PID/creation-time replacement.
        // An absent identity is stale, not a target for a restoration write.
        // Session retains its own identity-checked restoration records. Empty
        // measurements of still-present taskbars freeze movement until recovery.
        self.animations.retain(|id, _| ids.contains(id));
        // Feature/movement ownership survives gaps too; the backend validates
        // identities before restoring. Only transient animation state is pruned.
        let empty = discovery.taskbars.is_empty();
        for s in discovery.taskbars {
            if !selected(&s, settings) {
                if self.styles.contains_key(&s.id) || self.moved.contains(&s.id) {
                    combine_cleanup(Ok(()), backend.restore_taskbar(&s.id))?;
                    self.styles.remove(&s.id);
                    self.moved.remove(&s.id);
                    self.needs_measurement = true;
                }
                self.animations.remove(&s.id);
                self.stabilize.remove(&s.id);
                continue;
            }
            let desired = features(&s, settings, self.environment.as_ref())?;
            if self.styles.get(&s.id).is_none_or(|(old, last)| {
                !same_features(old, &desired) || now.saturating_sub(*last) >= STYLE_MS
            }) {
                let changed = backend.apply_features(&s.id, &desired)?;
                self.styles.insert(s.id, (desired, now));
                if changed {
                    // Visibility/toolbars can invalidate every measured bound.
                    // Never calculate a frame from this pre-mutation snapshot.
                    self.animations.remove(&s.id);
                    self.stabilize.insert(s.id);
                    self.needs_measurement = true;
                    continue;
                }
            }
            let Some((axis, target, range)) = proposal(&s, settings) else {
                self.animations.remove(&s.id);
                // An absent/ambiguous measurement is not a request to undo
                // centering. Explorer can report one while an overlay opens.
                // Freeze writes, retain restoration ownership, and rediscover;
                // otherwise every gap causes an original-position/center jump.
                if !settings.center && self.moved.contains(&s.id) {
                    backend.restore_position(&s.id)?;
                    self.moved.remove(&s.id);
                }
                continue;
            };
            let (displayed, cross) = if s.vertical {
                (s.position.y, s.position.x)
            } else {
                (s.position.x, s.position.y)
            };
            let geometry = Geometry::new(&s, &axis);
            match self.animations.get_mut(&s.id) {
                Some(a) => {
                    // Use the latest actual displayed position, not the original
                    // animation start or a prediction of a frame never sent.
                    if a.geometry != geometry || a.target != target || a.displayed != displayed {
                        a.from = displayed;
                        a.started = now;
                    }
                    // Toolbar/reservation movement needs immediate stabilization;
                    // ordinary button-count/content changes still animate.
                    a.stabilize |=
                        settings.toolbar_compensation && a.geometry.reservations_changed(&geometry);
                    a.geometry = geometry;
                    a.target = target;
                    a.range = range;
                    a.displayed = displayed;
                    a.cross = cross;
                }
                None => {
                    self.animations.insert(
                        s.id,
                        Animation {
                            geometry,
                            from: displayed,
                            target,
                            displayed,
                            cross,
                            range,
                            started: now,
                            stabilize: self.stabilize.remove(&s.id),
                        },
                    );
                }
            }
        }
        Ok(empty || self.needs_measurement)
    }

    fn frame(
        &mut self,
        backend: &mut impl Backend,
        settings: &Settings,
        now: u64,
    ) -> Result<(), String> {
        if self.needs_measurement {
            return Ok(());
        }
        for (id, a) in &mut self.animations {
            let elapsed = now.saturating_sub(a.started);
            let next = if a.stabilize
                || settings.animation == "none"
                || elapsed >= u64::from(settings.animation_ms)
            {
                a.target
            } else {
                animated_position(
                    a.from,
                    a.target,
                    elapsed,
                    settings.animation_ms,
                    &settings.animation,
                )?
            }
            .clamp(a.range.0, a.range.1); // Overshooting easing curves must NEVER escape the safe interval.
            if next != a.displayed {
                let point = if a.geometry.vertical {
                    Point {
                        x: a.cross,
                        y: next,
                    }
                } else {
                    Point {
                        x: next,
                        y: a.cross,
                    }
                };
                self.moved.insert(*id); // An error may still mean a partial native mutation.
                backend.move_list(id, point)?;
                a.displayed = next;
            }
            if a.stabilize {
                a.from = a.target;
                a.stabilize = false;
            }
        }
        Ok(())
    }

    fn animating(&self, now: u64, settings: &Settings) -> bool {
        self.animations.values().any(|a| {
            a.displayed != a.target
                || (a.from != a.target
                    && settings.animation != "none"
                    && now.saturating_sub(a.started) < u64::from(settings.animation_ms))
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Exit {
    Stop,
    Restart,
}

pub(crate) fn drive<B: Backend>(
    backend: B,
    control: &mut impl Control,
    settings: &Settings,
) -> Result<Exit, String> {
    let mut guard = RestoreGuard {
        backend,
        armed: true,
    };
    let result = run_loop(&mut guard.backend, control, settings);
    guard.finish(result) // Backend/COM Drop precedes restart and lease completion.
}

fn run_loop(
    backend: &mut impl Backend,
    control: &mut impl Control,
    settings: &Settings,
) -> Result<Exit, String> {
    settings.validate()?;
    style(&settings.style)?;
    let mut state = State::default();
    let mut effective = settings.clone();
    let mut next_environment = 0;
    let mut next_poll = 0;
    let mut next_frame = 0;
    let mut retry_after = 0;
    let mut failures = 0u32;
    let mut paused = false;
    let mut provider_ready = false;
    let mut previous_warnings = Vec::new();
    loop {
        // Owner-thread control pumps tray messages. Stop ALWAYS wins, including
        // a stop arriving concurrently with a queued internal restart.
        if control.stopped()? {
            return Ok(Exit::Stop);
        }
        if control.restart_requested() {
            return Ok(Exit::Restart);
        }
        let now = control.now_ms();
        if now >= next_environment {
            let env = backend.environment()?;
            if state
                .environment
                .as_ref()
                .is_none_or(|old| old.warnings != env.warnings)
            {
                for warning in &env.warnings {
                    warn(warning);
                }
            }
            let next = effective_settings(settings, &env);
            let power_changed = state
                .environment
                .as_ref()
                .is_some_and(|old| old.on_battery != env.on_battery);
            let monitors_changed = state
                .environment
                .as_ref()
                .is_none_or(|old| !same_monitors(old, &env));
            let next_paused =
                settings.pause_on_fullscreen && env.monitors.iter().any(|m| m.fullscreen);
            if power_changed
                || effective.animation != next.animation
                || effective.center != next.center
                || paused != next_paused
            {
                // Fresh actual positions, not predicted frames, seed the next
                // animation. Keep journals intact while paused or undiscovered.
                state.animations.clear();
                next_poll = 0;
                next_frame = 0;
            }
            if monitors_changed || effective.poll_ms != next.poll_ms {
                next_poll = 0;
            }
            effective = next;
            paused = next_paused;
            state.environment = Some(env);
            next_environment = control.now_ms().saturating_add(ENVIRONMENT_MS);
        }
        if paused || now < retry_after {
            let deadline = if paused {
                next_environment
            } else {
                next_environment.min(retry_after)
            };
            control.wait(deadline.saturating_sub(control.now_ms()).max(1))?;
            continue;
        }
        if control.stopped()? {
            return Ok(Exit::Stop);
        }
        if control.restart_requested() {
            return Ok(Exit::Restart);
        }
        if !effective.center {
            // Skip-resolution restores all owned positions, not just roots that
            // happened to survive the latest accessibility/discovery pass. The
            // backend checks HWND/PID/creation-time ownership before any write.
            // This is deliberately AFTER the fullscreen pause gate above.
            for id in state.moved.clone() {
                backend.restore_position(&id)?;
                state.moved.remove(&id);
            }
            state.animations.clear();
        }
        if !provider_ready {
            backend.set_measurement_backend(measurement(&settings.measurement)?)?;
            provider_ready = true;
        }
        let mut discovery_failed = false;
        let update = (|| {
            if now >= next_poll {
                let discovery = backend.discover().inspect_err(|_| {
                    discovery_failed = true;
                })?;
                if discovery.native_windows11 {
                    return Err(MODERN_TASKBAR_ERROR.to_owned());
                }
                if discovery.warnings != previous_warnings {
                    for warning in &discovery.warnings {
                        warn(warning);
                    }
                    previous_warnings = discovery.warnings.clone();
                }
                if control.stopped()? || control.restart_requested() {
                    return Ok(());
                }
                // A slow out-of-process measurement may span an environment
                // deadline. Discard it and reevaluate fullscreen BEFORE mutation.
                if control.now_ms() >= next_environment {
                    let fresh = backend.environment()?;
                    let changed = state.environment.as_ref().is_none_or(|old| {
                        old.on_battery != fresh.on_battery || !same_monitors(old, &fresh)
                    });
                    if changed {
                        next_environment = 0;
                        return Ok(());
                    }
                    // Slow providers must not starve forever when the environment
                    // is stable. Revalidation permits using this measured snapshot.
                    state.environment = Some(fresh);
                    next_environment = control.now_ms().saturating_add(ENVIRONMENT_MS);
                }
                let empty = state.refresh(backend, discovery, &effective, control.now_ms())?;
                let interval = if state.needs_measurement {
                    FRAME_MS
                } else {
                    u64::from(effective.poll_ms).max(if empty { 250 } else { 1 })
                };
                next_poll = control.now_ms().saturating_add(interval);
            }
            if now >= next_frame
                && !state.needs_measurement
                && control.now_ms() < next_environment
                && !control.stopped()?
                && !control.restart_requested()
            {
                state.frame(backend, &effective, control.now_ms())?;
                next_frame = control.now_ms().saturating_add(FRAME_MS);
            }
            Ok(())
        })();
        if let Err(error) = update {
            // String-only native failures cannot classify stale vs inaccessible
            // handles. Roll back partial work; identity validation is backend-owned.
            if discovery_failed {
                // Discovery is read-only. Do not undo a healthy centered layout
                // just because Explorer was transiently unavailable. Discard
                // stale animation frames but retain identity-checked journals.
                state.animations.clear();
            } else {
                let environment = state.environment.take();
                state = State {
                    environment,
                    ..State::default()
                };
                let cleanup = backend.restore();
                if !cleanup.is_empty() {
                    return combine_cleanup(Err(error), cleanup);
                }
            }
            failures = failures.saturating_add(1);
            if error == MODERN_TASKBAR_ERROR || (!discovery_failed && failures >= 3) {
                return Err(error);
            }
            warn(&format!(
                "{error}; {}, rediscovering",
                if discovery_failed {
                    "holding position"
                } else {
                    "restored partial changes"
                }
            ));
            retry_after = control
                .now_ms()
                .saturating_add(250 * (1u64 << failures.saturating_sub(1).min(4)));
            next_poll = 0;
            next_frame = 0;
            // Backoff is a deadline, not an inner sleep loop: environment and
            // tray/stop processing continue even during extended Explorer outages.
            continue;
        }
        if !state.animating(now, &effective) {
            failures = 0;
        }
        let deadline = if state.animating(now, &effective) && !state.needs_measurement {
            next_poll.min(next_frame)
        } else {
            next_poll
        }
        .min(next_environment);
        control.wait(deadline.saturating_sub(control.now_ms()).max(1))?;
    }
}

#[cfg(test)]
mod tests;
