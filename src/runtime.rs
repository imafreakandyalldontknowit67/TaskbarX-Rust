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
use taskbarx_core::{animated_position, center_target, legal_position_range, AxisLayout};
use taskbarx_win32::{Discovery, Point, Rect, Session, Style, TaskbarId, TaskbarSnapshot};

const FRAME_MS: u64 = 16;
const STYLE_MS: u64 = 2000;
pub(crate) const MODERN_TASKBAR_ERROR: &str = "The native modern Windows 11 taskbar is not supported. No changes were applied to it. This runtime requires the classic taskbar window hierarchy.";

pub(crate) trait Backend {
    fn discover(&mut self) -> Result<Discovery, String>;
    fn move_list(&mut self, id: &TaskbarId, position: Point) -> Result<(), String>;
    fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String>;
    fn set_style(&mut self, id: &TaskbarId, style: Style, color: [u8; 4]) -> Result<(), String>;
    fn restore(&mut self) -> Vec<String>;
}

pub(crate) struct NativeBackend(Session);
impl NativeBackend {
    pub(crate) fn new() -> Result<Self, String> {
        Session::new().map(Self)
    }
}
impl Backend for NativeBackend {
    fn discover(&mut self) -> Result<Discovery, String> {
        self.0.discover()
    }
    fn move_list(&mut self, id: &TaskbarId, position: Point) -> Result<(), String> {
        self.0.move_task_list(id, position)
    }
    fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String> {
        self.0.restore_position(id)
    }
    fn set_style(&mut self, id: &TaskbarId, style: Style, color: [u8; 4]) -> Result<(), String> {
        self.0.set_style(id, style, color)
    }
    fn restore(&mut self) -> Vec<String> {
        self.0.restore()
    }
}

/// Monotonic clock plus interruptible waiting. The Windows implementation also
/// pumps owner-thread COM messages and observes shutdown notifications.
pub(crate) trait Control {
    fn now_ms(&self) -> u64;
    fn stopped(&mut self) -> Result<bool, String>;
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
    fn finish(mut self, result: Result<(), String>) -> Result<(), String> {
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

fn combine_cleanup(result: Result<(), String>, failures: Vec<String>) -> Result<(), String> {
    if failures.is_empty() {
        return result;
    }
    let cleanup = format!("restoration failed: {}", failures.join("; "));
    Err(match result {
        Ok(()) => cleanup,
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
    let target = center_target(&layout, offset, settings.center_between)?.clamp(range.0, range.1);
    Some((layout, target, range))
}

fn style(name: &str) -> Result<Style, String> {
    match name {
        "default" => Ok(Style::Default),
        "transparent" => Ok(Style::Transparent),
        "blur" => Ok(Style::Blur),
        "acrylic" => Ok(Style::Acrylic),
        "opaque" => Ok(Style::Opaque),
        _ => Err(format!("unsupported taskbar style: {name}")),
    }
}

#[derive(PartialEq, Eq)]
struct Geometry {
    axis: [i32; 7],
    parent: [i32; 4],
    vertical: bool,
    dpi: u32,
}
impl Geometry {
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
    moved: bool,
}

#[derive(Default)]
struct State {
    animations: HashMap<TaskbarId, Animation>,
    styles: HashMap<TaskbarId, u64>,
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
        if self.styles.keys().any(|id| {
            settings.style == "default"
                || discovery
                    .taskbars
                    .iter()
                    .any(|s| s.id == *id && !selected(s, settings))
        }) {
            // There is no per-taskbar style restore in the fixed backend
            // contract. Roll back the session if selection ceases to include a
            // styled taskbar, then wait for fresh post-restoration measurements.
            let cleanup = backend.restore();
            *self = Self::default();
            combine_cleanup(Ok(()), cleanup)?;
            return Ok(true);
        }
        let ids: HashSet<_> = discovery.taskbars.iter().map(|s| s.id).collect();
        // Never reuse an animation across an HWND/PID/creation-time replacement.
        // An absent identity is stale, not a target for a restoration write.
        // Session retains its own identity-checked restoration records. Empty
        // measurements of still-present taskbars are restored below instead.
        self.animations.retain(|id, _| ids.contains(id));
        self.styles.retain(|id, _| ids.contains(id));
        let empty = discovery.taskbars.is_empty();
        for s in discovery.taskbars {
            if selected(&s, settings)
                && settings.style != "default"
                && self
                    .styles
                    .get(&s.id)
                    .is_none_or(|last| now.saturating_sub(*last) >= STYLE_MS)
            {
                backend.set_style(&s.id, style(&settings.style)?, settings.color)?;
                self.styles.insert(s.id, now);
            }
            let Some((axis, target, range)) = proposal(&s, settings) else {
                if let Some(old) = self.animations.remove(&s.id) {
                    if old.moved {
                        backend.restore_position(&s.id)?;
                    }
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
                            moved: false,
                        },
                    );
                }
            }
        }
        Ok(empty)
    }

    fn frame(
        &mut self,
        backend: &mut impl Backend,
        settings: &Settings,
        now: u64,
    ) -> Result<(), String> {
        for (id, a) in &mut self.animations {
            let elapsed = now.saturating_sub(a.started);
            let next =
                if settings.animation == "none" || elapsed >= u64::from(settings.animation_ms) {
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
                a.moved = true; // An error may still mean a partial native mutation.
                backend.move_list(id, point)?;
                a.displayed = next;
            }
        }
        Ok(())
    }

    fn animating(&self) -> bool {
        self.animations.values().any(|a| a.displayed != a.target)
    }
}

pub(crate) fn drive<B: Backend>(
    backend: B,
    control: &mut impl Control,
    settings: &Settings,
) -> Result<(), String> {
    let mut guard = RestoreGuard {
        backend,
        armed: true,
    };
    let result = run_loop(&mut guard.backend, control, settings);
    guard.finish(result) // Backend (and its COM apartment) drops BEFORE the instance lease.
}

fn run_loop(
    backend: &mut impl Backend,
    control: &mut impl Control,
    settings: &Settings,
) -> Result<(), String> {
    settings.validate()?;
    style(&settings.style)?;
    let mut state = State::default();
    let mut next_poll = 0;
    let mut next_frame = 0;
    let mut failures = 0u32;
    let mut previous_warnings = Vec::new();
    loop {
        if control.stopped()? {
            return Ok(());
        }
        let now = control.now_ms();
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
                if control.stopped()? {
                    return Ok(());
                }
                let empty = state.refresh(backend, discovery, settings, now)?;
                // An absent Explorer is an idle, stoppable state, not a restart request.
                let interval = u64::from(settings.poll_ms).max(if empty { 250 } else { 1 });
                next_poll = now.saturating_add(interval);
            }
            if now >= next_frame && !control.stopped()? {
                state.frame(backend, settings, now)?;
                next_frame = now.saturating_add(FRAME_MS);
            }
            Ok(())
        })();
        if let Err(error) = update {
            // String-only backend errors cannot safely distinguish stale handles
            // from access failures. Roll back ALL partial work, then rediscover
            // with bounded exponential backoff; repeated failures exit cleanly.
            state = State::default();
            let cleanup = backend.restore();
            if !cleanup.is_empty() {
                return combine_cleanup(Err(error), cleanup);
            }
            failures = failures.saturating_add(1);
            if error == MODERN_TASKBAR_ERROR || (!discovery_failed && failures >= 3) {
                return Err(error);
            }
            warn(&format!("{error}; restored partial changes, rediscovering"));
            // Discovery may fail throughout an Explorer restart. Keep waiting
            // (up to four seconds between attempts) rather than assuming that
            // a string-only error identifies permanent incompatibility.
            let deadline = control
                .now_ms()
                .saturating_add(250 * (1u64 << failures.saturating_sub(1).min(4)));
            // Control may wake early for COM/window messages. Those wakeups do
            // not shorten the backoff; shutdown always interrupts it promptly.
            while !control.stopped()? {
                let remaining = deadline.saturating_sub(control.now_ms());
                if remaining == 0 {
                    break;
                }
                control.wait(remaining)?;
            }
            next_poll = 0;
            next_frame = 0;
            continue;
        }
        // Reset only after a full discovery + animation cycle has succeeded;
        // idle frames must not mask a persistently failing discovery.
        if !state.animating() {
            failures = 0;
        }
        let deadline = if state.animating() {
            next_poll.min(next_frame)
        } else {
            next_poll
        };
        control.wait(deadline.saturating_sub(control.now_ms()).max(1))?;
    }
}

#[cfg(test)]
mod tests;
