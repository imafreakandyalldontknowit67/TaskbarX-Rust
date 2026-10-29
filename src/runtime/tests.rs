use super::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

#[derive(Default)]
struct Data {
    events: Vec<String>,
    moves: Vec<(TaskbarId, i32, i32)>,
    discoveries: VecDeque<Result<Discovery, String>>,
    fail_style: bool,
    fail_move: bool,
    fail_move_after: Option<usize>,
    fail_style_root: Option<isize>,
    panic_move: bool,
    fail_restore: bool,
    features: Vec<(TaskbarId, TaskbarFeatures)>,
    geometry_changes: VecDeque<bool>,
    providers: Vec<MeasurementBackend>,
    environments: VecDeque<(u64, EnvironmentSnapshot)>,
    environment: Option<EnvironmentSnapshot>,
    environment_times: Vec<u64>,
    discovery_times: Vec<u64>,
    mutation_times: Vec<u64>,
    now: u64,
    fail_root_restore: bool,
    fail_provider: bool,
    discovery_delay: u64,
}
#[derive(Clone, Default)]
struct Fake(Rc<RefCell<Data>>);
fn copy_environment(e: &EnvironmentSnapshot) -> EnvironmentSnapshot {
    EnvironmentSnapshot {
        on_battery: e.on_battery,
        warnings: e.warnings.clone(),
        monitors: e
            .monitors
            .iter()
            .map(|m| MonitorState {
                bounds: m.bounds,
                primary: m.primary,
                maximized: m.maximized,
                fullscreen: m.fullscreen,
            })
            .collect(),
    }
}
fn environment() -> EnvironmentSnapshot {
    EnvironmentSnapshot {
        on_battery: None,
        monitors: vec![],
        warnings: vec![],
    }
}
impl Backend for Fake {
    fn environment(&mut self) -> Result<EnvironmentSnapshot, String> {
        let mut d = self.0.borrow_mut();
        let now = d.now;
        d.environment_times.push(now);
        while d.environments.front().is_some_and(|(at, _)| *at <= now) {
            d.environment = d.environments.pop_front().map(|(_, e)| e);
        }
        Ok(d.environment
            .as_ref()
            .map(copy_environment)
            .unwrap_or_else(environment))
    }
    fn set_measurement_backend(&mut self, provider: MeasurementBackend) -> Result<(), String> {
        let mut data = self.0.borrow_mut();
        data.providers.push(provider);
        if data.fail_provider {
            Err("provider unavailable".into())
        } else {
            Ok(())
        }
    }
    fn restore_taskbar(&mut self, id: &TaskbarId) -> Vec<String> {
        let mut d = self.0.borrow_mut();
        d.events.push(format!("restore-root:{}", id.root));
        if d.fail_root_restore {
            vec!["root cleanup failure".into()]
        } else {
            vec![]
        }
    }

    fn discover(&mut self) -> Result<Discovery, String> {
        let mut data = self.0.borrow_mut();
        data.events.push("discover".into());
        let now = data.now;
        data.discovery_times.push(now);
        data.now += data.discovery_delay;
        data.discoveries
            .pop_front()
            .unwrap_or_else(|| Ok(discovery(vec![])))
    }
    fn move_list(&mut self, id: &TaskbarId, point: Point) -> Result<(), String> {
        let mut data = self.0.borrow_mut();
        data.events.push(format!("move:{}", id.root));
        data.moves.push((*id, point.x, point.y));
        let now = data.now;
        data.mutation_times.push(now);
        if data.panic_move {
            panic!("injected move panic");
        }
        if data.fail_move
            || data
                .fail_move_after
                .is_some_and(|count| data.moves.len() >= count)
        {
            return Err("injected movement failure".into());
        }
        Ok(())
    }
    fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String> {
        let mut d = self.0.borrow_mut();
        d.events.push(format!("restore-position:{}", id.root));
        let now = d.now;
        d.mutation_times.push(now);
        Ok(())
    }
    fn apply_features(&mut self, id: &TaskbarId, f: &TaskbarFeatures) -> Result<bool, String> {
        let mut data = self.0.borrow_mut();
        if f.style != Style::Default {
            data.events.push(format!("style:{}", id.root));
        }
        data.features.push((
            *id,
            TaskbarFeatures {
                style: f.style,
                color: f.color,
                opacity: f.opacity,
                corner_radius: f.corner_radius,
                segments: f.segments,
                hide_start: f.hide_start,
                hide_tray: f.hide_tray,
                toolbar_compensation: f.toolbar_compensation,
            },
        ));
        let now = data.now;
        data.mutation_times.push(now);
        if data.fail_style || data.fail_style_root == Some(id.root) {
            Err("injected partial style failure".into())
        } else {
            Ok(data.geometry_changes.pop_front().unwrap_or(false))
        }
    }
    fn restore(&mut self) -> Vec<String> {
        let mut data = self.0.borrow_mut();
        data.events.push("restore-all".into());
        if data.fail_restore {
            vec!["injected cleanup failure".into()]
        } else {
            vec![]
        }
    }
}
struct Clock {
    now: u64,
    stop_at: u64,
    waits: Vec<u64>,
    wake_cap: u64,
    data: Option<Rc<RefCell<Data>>>,
}
impl Clock {
    fn until(stop_at: u64) -> Self {
        Self {
            now: 0,
            stop_at,
            waits: vec![],
            wake_cap: u64::MAX,
            data: None,
        }
    }
}
impl Control for Clock {
    fn now_ms(&self) -> u64 {
        self.data
            .as_ref()
            .map_or(self.now, |data| data.borrow().now)
    }
    fn stopped(&mut self) -> Result<bool, String> {
        Ok(self.now_ms() >= self.stop_at)
    }
    fn wait(&mut self, ms: u64) -> Result<(), String> {
        assert!(ms > 0, "must never busy-loop");
        self.waits.push(ms);
        self.now = self.now_ms() + ms.min(self.wake_cap);
        if let Some(data) = &self.data {
            data.borrow_mut().now = self.now;
        }
        Ok(())
    }
}

fn rect(left: i32, right: i32) -> Rect {
    Rect {
        left,
        right,
        top: 0,
        bottom: 40,
    }
}
fn snapshot(root: isize, primary: bool, displayed: i32) -> TaskbarSnapshot {
    TaskbarSnapshot {
        id: TaskbarId {
            root,
            task_list: root + 1,
            parent: root + 2,
            process_id: 42,
            process_started: 100,
        },
        primary,
        vertical: false,
        dpi: 96,
        taskbar_rect: rect(-1000, 0),
        parent_rect: rect(-950, -100),
        list_rect: rect(-950 + displayed, -100 + displayed),
        available_rect: rect(-900, -100),
        content_rect: Some(rect(-940 + displayed, -740 + displayed)),
        position: Point { x: displayed, y: 7 },
    }
}
fn discovery(taskbars: Vec<TaskbarSnapshot>) -> Discovery {
    Discovery {
        build_number: 19045,
        native_windows11: false,
        taskbars,
        warnings: vec![],
    }
}
fn settings() -> Settings {
    Settings {
        animation: "linear".into(),
        animation_ms: 100,
        ..Settings::default()
    }
}

#[test]
fn signed_axis_and_parent_relative_coordinates() {
    let snapshot = snapshot(1, true, 30);
    let a = layout(&snapshot).unwrap();
    assert_eq!(a.bar_start, -1000);
    assert_eq!(a.parent_start, -950);
    assert_eq!(a.free_start, -900);
    assert_eq!(a.content_offset, 10);
    assert_eq!(a.content_len, 200);
    let (_, target, range) = proposal(&snapshot, &settings()).unwrap();
    assert_eq!(target, 340);
    assert!(target >= range.0 && target <= range.1);
}

#[test]
fn retargets_from_actual_displayed_position_not_original_start() {
    let mut state = State::default();
    let mut backend = Fake::default();
    let settings = settings();
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 50)]),
            &settings,
            0,
        )
        .unwrap();
    state.frame(&mut backend, &settings, 50).unwrap();
    let mut changed = snapshot(1, true, 195);
    changed.available_rect = rect(-800, -200);
    changed.content_rect = Some(rect(-745, -645)); // 100 pixels of icons now.
    let id = changed.id;
    state
        .refresh(&mut backend, discovery(vec![changed]), &settings, 60)
        .unwrap();
    let a = &state.animations[&id];
    assert_eq!(a.from, 195);
    assert_eq!(a.started, 60);
    assert_eq!(a.displayed, 195);
    state.frame(&mut backend, &settings, 110).unwrap();
    assert_eq!(backend.0.borrow().moves.last().unwrap().2, 7);
}

#[test]
fn stable_poll_does_not_restart_animation() {
    let mut state = State::default();
    let mut backend = Fake::default();
    let settings = settings();
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 50)]),
            &settings,
            0,
        )
        .unwrap();
    state.frame(&mut backend, &settings, 50).unwrap();
    let displayed = backend.0.borrow().moves.last().unwrap().1;
    let next = snapshot(1, true, displayed);
    let id = next.id;
    state
        .refresh(&mut backend, discovery(vec![next]), &settings, 55)
        .unwrap();
    assert_eq!(state.animations[&id].started, 0);
}

#[test]
fn discovery_replacement_and_explorer_creation_time_discard_animation() {
    let mut state = State::default();
    let mut backend = Fake::default();
    let settings = settings();
    let old = snapshot(1, true, 50).id;
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 50)]),
            &settings,
            0,
        )
        .unwrap();
    state.frame(&mut backend, &settings, 50).unwrap();
    let mut replacement = snapshot(1, true, 80);
    replacement.id.process_started += 1; // Same numeric HWND/PID, new Explorer identity.
    let new = replacement.id;
    state
        .refresh(&mut backend, discovery(vec![replacement]), &settings, 60)
        .unwrap();
    assert!(!state.animations.contains_key(&old));
    assert_eq!(state.animations[&new].from, 80);
    assert!(!backend
        .0
        .borrow()
        .events
        .contains(&"restore-position:1".to_owned()));
}

#[test]
fn selection_and_center_false_never_move_unselected_taskbars() {
    let mut state = State::default();
    let mut backend = Fake::default();
    let mut settings = settings();
    settings.secondary = false;
    settings.style = "transparent".into();
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 50), snapshot(10, false, 50)]),
            &settings,
            0,
        )
        .unwrap();
    state.frame(&mut backend, &settings, 100).unwrap();
    assert!(backend
        .0
        .borrow()
        .events
        .iter()
        .all(|e| !e.ends_with(":10")));
    settings.center = false;
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 340)]),
            &settings,
            200,
        )
        .unwrap();
    assert!(state.animations.is_empty());
    assert!(backend
        .0
        .borrow()
        .events
        .contains(&"restore-position:1".to_owned()));
}

#[test]
fn empty_or_unsafe_measurements_restore_previous_position() {
    for unsafe_rect in [false, true] {
        let mut state = State::default();
        let mut backend = Fake::default();
        let settings = settings();
        state
            .refresh(
                &mut backend,
                discovery(vec![snapshot(1, true, 50)]),
                &settings,
                0,
            )
            .unwrap();
        state.frame(&mut backend, &settings, 100).unwrap();
        let mut empty = snapshot(1, true, 340);
        if unsafe_rect {
            empty.available_rect.right = empty.available_rect.left;
        } else {
            empty.content_rect = None;
        }
        state
            .refresh(&mut backend, discovery(vec![empty]), &settings, 200)
            .unwrap();
        assert!(state.animations.is_empty());
        assert!(backend
            .0
            .borrow()
            .events
            .contains(&"restore-position:1".into()));
    }
}

#[test]
fn vertical_orientation_dpi_change_and_cross_axis_are_preserved() {
    let mut s = snapshot(1, true, 50);
    let rotate = |r: &mut Rect| {
        std::mem::swap(&mut r.left, &mut r.top);
        std::mem::swap(&mut r.right, &mut r.bottom);
    };
    for r in [
        &mut s.taskbar_rect,
        &mut s.parent_rect,
        &mut s.list_rect,
        &mut s.available_rect,
        s.content_rect.as_mut().unwrap(),
    ] {
        rotate(r);
    }
    s.vertical = true;
    s.dpi = 144;
    s.position = Point { x: 9, y: 50 };
    let mut state = State::default();
    let mut backend = Fake::default();
    state
        .refresh(&mut backend, discovery(vec![s]), &settings(), 0)
        .unwrap();
    state.frame(&mut backend, &settings(), 100).unwrap();
    let data = backend.0.borrow();
    assert_eq!(data.moves[0].1, 9);
    assert_eq!(data.moves[0].2, 340);
}

#[test]
fn every_frame_including_overshoot_is_clamped() {
    let mut s = settings();
    for name in taskbarx_core::EASING_NAMES {
        s.animation = name.to_string();
        s.primary_offset = 10000;
        let mut state = State::default();
        let mut backend = Fake::default();
        let snapshot = snapshot(1, true, -500);
        let (_, _, range) = proposal(&snapshot, &s).unwrap();
        state
            .refresh(&mut backend, discovery(vec![snapshot]), &s, 0)
            .unwrap();
        for time in 0..=100 {
            state.frame(&mut backend, &s, time).unwrap();
        }
        for (_, x, y) in &backend.0.borrow().moves {
            assert!(*x >= range.0 && *x <= range.1, "{name} escaped range");
            assert_eq!(*y, 7);
        }
    }
}

#[test]
fn styles_are_reasserted_at_modest_cadence() {
    let mut settings = settings();
    settings.style = "blur".into();
    let mut state = State::default();
    let mut backend = Fake::default();
    for time in [0, 100, 1999, 2000] {
        state
            .refresh(
                &mut backend,
                discovery(vec![snapshot(1, true, 50)]),
                &settings,
                time,
            )
            .unwrap();
    }
    assert_eq!(
        backend
            .0
            .borrow()
            .events
            .iter()
            .filter(|s| *s == "style:1")
            .count(),
        2
    );
}

#[test]
fn deselecting_styled_taskbar_restores_only_its_root() {
    let mut settings = settings();
    settings.style = "blur".into();
    let mut state = State::default();
    let mut backend = Fake::default();
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 50)]),
            &settings,
            0,
        )
        .unwrap();
    settings.primary = false;
    assert!(state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 50)]),
            &settings,
            100
        )
        .unwrap());
    assert!(state.animations.is_empty() && state.styles.is_empty());
    assert_eq!(backend.0.borrow().events, ["style:1", "restore-root:1"]);
}

#[test]
fn recurring_movement_errors_back_off_even_when_first_frame_is_noop() {
    let backend = Fake::default();
    backend.0.borrow_mut().fail_move = true;
    for _ in 0..3 {
        backend
            .0
            .borrow_mut()
            .discoveries
            .push_back(Ok(discovery(vec![snapshot(1, true, 50)])));
    }
    let mut clock = Clock::until(5000);
    let error = drive(backend.clone(), &mut clock, &settings()).unwrap_err();
    assert!(error.contains("movement failure"));
    assert!(clock.waits.iter().all(|ms| *ms <= ENVIRONMENT_MS));
    assert!(clock.now >= 750);
    assert_eq!(backend.0.borrow().moves.len(), 3);
}

#[test]
fn modern_windows11_rejected_before_any_mutation() {
    let backend = Fake::default();
    let mut discovery = discovery(vec![snapshot(1, true, 50)]);
    discovery.native_windows11 = true;
    backend.0.borrow_mut().discoveries.push_back(Ok(discovery));
    let mut settings = settings();
    settings.style = "opaque".into();
    let error = drive(backend.clone(), &mut Clock::until(1000), &settings).unwrap_err();
    assert!(error.contains("Windows 11"));
    assert!(backend.0.borrow().moves.is_empty());
    assert!(!backend
        .0
        .borrow()
        .events
        .iter()
        .any(|s| s.starts_with("style")));
}

#[test]
fn diagnostics_only_discovers_even_with_style_and_center_enabled() {
    let mut backend = Fake::default();
    backend
        .0
        .borrow_mut()
        .discoveries
        .push_back(Ok(discovery(vec![snapshot(1, true, 50)])));
    let mut settings = settings();
    settings.style = "acrylic".into();
    let report = crate::diagnostics::report(&mut backend, &settings).unwrap();
    assert_eq!(report["read_only"], true);
    assert_eq!(
        report["taskbars"][0]["proposed_target"]["position"]["x"],
        340
    );
    assert_eq!(backend.0.borrow().events, ["discover"]);
}

#[test]
fn absent_explorer_waits_and_remains_stoppable() {
    let backend = Fake::default();
    let mut clock = Clock::until(1200);
    drive(backend.clone(), &mut clock, &settings()).unwrap();
    assert_eq!(clock.waits, vec![100; 12]);
    assert_eq!(backend.0.borrow().events.last().unwrap(), "restore-all");
}

#[test]
fn partial_errors_restore_before_backoff_and_preserve_original_error() {
    for style_error in [true, false] {
        let backend = Fake::default();
        {
            let mut data = backend.0.borrow_mut();
            data.fail_style = style_error;
            data.fail_move = !style_error;
            data.fail_restore = true;
            data.discoveries
                .push_back(Ok(discovery(vec![snapshot(1, true, 50)])));
        }
        let mut settings = settings();
        settings.style = "blur".into();
        settings.animation = "none".into();
        let error = drive(backend.clone(), &mut Clock::until(5000), &settings).unwrap_err();
        assert!(error.contains(if style_error {
            "partial style"
        } else {
            "movement failure"
        }));
        assert!(error.contains("cleanup failure"));
        let data = backend.0.borrow();
        let mutation = data
            .events
            .iter()
            .position(|s| s.starts_with(if style_error { "style" } else { "move" }))
            .unwrap();
        assert_eq!(data.events[mutation + 1], "restore-all");
    }
}

#[test]
fn invalid_handle_failure_discards_state_and_rediscovers_with_backoff() {
    let backend = Fake::default();
    backend.0.borrow_mut().discoveries.extend([
        Err("invalid taskbar handle".into()),
        Ok(discovery(vec![snapshot(100, true, 50)])),
    ]);
    let mut clock = Clock::until(500);
    let mut settings = settings();
    settings.animation = "none".into();
    drive(backend.clone(), &mut clock, &settings).unwrap();
    assert_eq!(clock.waits[0], 100);
    assert_eq!(
        &backend.0.borrow().events[..3],
        ["discover", "restore-all", "discover"]
    );
    assert_eq!(backend.0.borrow().moves[0].0.root, 100);
}

#[test]
fn backoff_survives_early_wakeups_but_stop_interrupts_it() {
    let backend = Fake::default();
    backend
        .0
        .borrow_mut()
        .discoveries
        .push_back(Err("Explorer absent".into()));
    let mut clock = Clock::until(200);
    clock.wake_cap = 10;
    drive(backend.clone(), &mut clock, &settings()).unwrap();
    assert_eq!(clock.now, 200);
    assert_eq!(
        backend
            .0
            .borrow()
            .events
            .iter()
            .filter(|e| *e == "discover")
            .count(),
        1
    );
}

#[test]
fn extended_explorer_outage_is_stoppable_with_capped_backoff() {
    let backend = Fake::default();
    for _ in 0..10 {
        backend
            .0
            .borrow_mut()
            .discoveries
            .push_back(Err("Explorer absent".into()));
    }
    let mut clock = Clock::until(12000);
    drive(backend.clone(), &mut clock, &settings()).unwrap();
    assert!(clock.waits.iter().all(|ms| *ms <= ENVIRONMENT_MS));
    assert_eq!(
        backend
            .0
            .borrow()
            .events
            .iter()
            .filter(|e| *e == "discover")
            .count(),
        7
    );
}

#[test]
fn later_taskbar_failure_rolls_back_earlier_successful_changes() {
    for style_error in [true, false] {
        let backend = Fake::default();
        {
            let mut data = backend.0.borrow_mut();
            if style_error {
                data.fail_style_root = Some(10);
            } else {
                data.fail_move_after = Some(2);
            }
            data.fail_restore = true; // Make this error terminal so the complete trace is inspectable.
            data.discoveries.push_back(Ok(discovery(vec![
                snapshot(1, true, 50),
                snapshot(10, false, 50),
            ])));
        }
        let mut settings = settings();
        settings.style = "blur".into();
        settings.animation = "none".into();
        let error = drive(backend.clone(), &mut Clock::until(1000), &settings).unwrap_err();
        assert!(error.contains("failure"));
        let data = backend.0.borrow();
        assert_eq!(&data.events[..3], ["discover", "style:1", "style:10"]);
        if style_error {
            assert_eq!(data.events[3], "restore-all");
        } else {
            assert_eq!(data.moves.len(), 2);
            assert_eq!(data.events[5], "restore-all");
        }
    }
}

#[test]
fn restoration_runs_during_unwind() {
    let backend = Fake::default();
    backend.0.borrow_mut().panic_move = true;
    backend
        .0
        .borrow_mut()
        .discoveries
        .push_back(Ok(discovery(vec![snapshot(1, true, 50)])));
    let mut settings = settings();
    settings.animation = "none".into();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drive(backend.clone(), &mut Clock::until(1000), &settings)
    }));
    assert!(result.is_err());
    assert_eq!(backend.0.borrow().events.last().unwrap(), "restore-all");
}

#[test]
fn unsafe_measurements_restore_even_after_a_discovery_gap() {
    let mut state = State::default();
    let mut backend = Fake::default();
    let settings = settings();
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 0)]),
            &settings,
            0,
        )
        .unwrap();
    state.frame(&mut backend, &settings, 50).unwrap();
    state
        .refresh(&mut backend, discovery(vec![]), &settings, 60)
        .unwrap();
    let mut returned = snapshot(1, true, 100);
    returned.content_rect = None;
    state
        .refresh(&mut backend, discovery(vec![returned]), &settings, 70)
        .unwrap();
    assert!(backend
        .0
        .borrow()
        .events
        .contains(&"restore-position:1".into()));
}
#[test]
fn bounce_peak_does_not_end_animation_before_duration() {
    let mut state = State::default();
    let mut backend = Fake::default();
    let mut settings = settings();
    settings.animation = "bounceeaseout".into();
    settings.animation_ms = 352;
    settings.poll_ms = 10000;
    state
        .refresh(
            &mut backend,
            discovery(vec![snapshot(1, true, 0)]),
            &settings,
            0,
        )
        .unwrap();
    state.frame(&mut backend, &settings, 128).unwrap();
    assert!(state.animating(128, &settings));
    let peak = backend.0.borrow().moves.last().unwrap().1;
    state.frame(&mut backend, &settings, 144).unwrap();
    assert!(backend.0.borrow().moves.last().unwrap().1 < peak);
    state.frame(&mut backend, &settings, 352).unwrap();
    assert!(!state.animating(352, &settings));
}

fn monitor(
    left: i32,
    right: i32,
    primary: bool,
    maximized: bool,
    fullscreen: bool,
) -> MonitorState {
    MonitorState {
        bounds: rect(left, right),
        primary,
        maximized,
        fullscreen,
    }
}
fn env(monitors: Vec<MonitorState>, battery: Option<bool>) -> EnvironmentSnapshot {
    EnvironmentSnapshot {
        on_battery: battery,
        monitors,
        warnings: vec![],
    }
}
fn bound_clock(fake: &Fake, end: u64) -> Clock {
    let mut clock = Clock::until(end);
    clock.data = Some(fake.0.clone());
    clock
}
fn shift(mut s: TaskbarSnapshot, dx: i32) -> TaskbarSnapshot {
    for r in [
        &mut s.taskbar_rect,
        &mut s.parent_rect,
        &mut s.list_rect,
        &mut s.available_rect,
        s.content_rect.as_mut().unwrap(),
    ] {
        r.left += dx;
        r.right += dx;
    }
    s
}

#[test]
fn battery_unknown_and_none_overrides_inherit_independently() {
    let mut s = settings();
    s.battery_animation = Some("none".into());
    s.battery_poll_ms = Some(700);
    for battery in [None, Some(false)] {
        let effective = effective_settings(&s, &env(vec![], battery));
        assert_eq!(effective.animation, s.animation);
        assert_eq!(effective.poll_ms, s.poll_ms);
    }
    let effective = effective_settings(&s, &env(vec![], Some(true)));
    assert_eq!(effective.animation, "none");
    assert_eq!(effective.poll_ms, 700);
    s.battery_animation = None;
    assert_eq!(
        effective_settings(&s, &env(vec![], Some(true))).animation,
        s.animation
    );
    s.battery_poll_ms = None;
    assert_eq!(effective_settings(&s, &env(vec![], Some(true))), s);
}

#[test]
fn power_transitions_interrupt_long_poll_and_use_selected_interval_without_busy_wait() {
    let fake = Fake::default();
    let mut s = settings();
    s.poll_ms = 10000;
    s.animation_ms = 1000;
    s.battery_animation = Some("none".into());
    s.battery_poll_ms = Some(50);
    fake.0.borrow_mut().environments.extend([
        (0, env(vec![], Some(false))),
        (100, env(vec![], Some(true))),
        (200, env(vec![], None)),
    ]);
    for displayed in [50, 90, 340, 340] {
        fake.0
            .borrow_mut()
            .discoveries
            .push_back(Ok(discovery(vec![snapshot(1, true, displayed)])));
    }
    let mut clock = bound_clock(&fake, 400);
    drive(fake.clone(), &mut clock, &s).unwrap();
    let data = fake.0.borrow();
    assert_eq!(data.discovery_times, [0, 100, 150, 200]);
    assert!(data.mutation_times.contains(&100));
    assert!(data.moves.iter().any(|(_, x, _)| *x == 340));
    assert!(clock.waits.iter().all(|v| *v > 0 && *v <= ENVIRONMENT_MS));
}

#[test]
fn fullscreen_precedes_provider_and_accessibility_and_shutdown_still_restores() {
    for provider in ["msaa", "uia"] {
        let fake = Fake::default();
        fake.0.borrow_mut().environment =
            Some(env(vec![monitor(-1000, 0, true, false, true)], None));
        let s = Settings {
            pause_on_fullscreen: true,
            poll_ms: 10000,
            measurement: provider.into(),
            style: "blur".into(),
            hide_primary_tray: true,
            ..settings()
        };
        drive(fake.clone(), &mut bound_clock(&fake, 350), &s).unwrap();
        let d = fake.0.borrow();
        assert!(
            d.providers.is_empty() && d.discovery_times.is_empty() && d.mutation_times.is_empty()
        );
        assert_eq!(d.events, ["restore-all"]);
        assert!(d.environment_times.len() >= 4);
    }
}

#[test]
fn fullscreen_freezes_all_mutation_preserves_journal_and_rediscovers_on_resume() {
    let fake = Fake::default();
    let s = Settings {
        pause_on_fullscreen: true,
        poll_ms: 10000,
        animation: "none".into(),
        ..settings()
    };
    fake.0.borrow_mut().environments.extend([
        (0, env(vec![monitor(-1000, 0, true, false, false)], None)),
        (100, env(vec![monitor(-1000, 0, true, false, true)], None)),
        (300, env(vec![monitor(-1000, 0, true, false, false)], None)),
    ]);
    let mut missing = snapshot(1, true, 340);
    missing.content_rect = None;
    fake.0.borrow_mut().discoveries.extend([
        Ok(discovery(vec![snapshot(1, true, 50)])),
        Ok(discovery(vec![missing])),
    ]);
    drive(fake.clone(), &mut bound_clock(&fake, 400), &s).unwrap();
    let d = fake.0.borrow();
    assert_eq!(d.discovery_times, [0, 300]);
    assert!(d.mutation_times.iter().all(|at| *at < 100 || *at >= 300));
    assert!(d.events.contains(&"restore-position:1".into()));
    assert_eq!(d.events.iter().filter(|e| *e == "restore-all").count(), 1);
}

#[test]
fn skip_uses_primary_monitor_width_for_all_taskbars_not_secondary_or_vertical_length() {
    let s = Settings {
        skip_resolutions: vec![1000],
        ..settings()
    };
    assert!(
        !effective_settings(
            &s,
            &env(
                vec![
                    monitor(-1000, 0, true, false, false),
                    monitor(0, 800, false, false, false)
                ],
                None
            )
        )
        .center
    );
    assert!(
        effective_settings(
            &s,
            &env(
                vec![
                    monitor(-800, 0, true, false, false),
                    monitor(0, 1000, false, false, false)
                ],
                None
            )
        )
        .center
    );
    let mut tall = monitor(0, 40, true, false, false);
    tall.bounds.bottom = 1000;
    assert!(effective_settings(&s, &env(vec![tall], None)).center);
}

#[test]
fn skip_restores_all_owned_positions_but_not_appearance_and_resumes() {
    let fake = Fake::default();
    let s = Settings {
        skip_resolutions: vec![1000],
        style: "transparent-gradient".into(),
        primary_opacity: Some(0),
        secondary_opacity: Some(100),
        animation: "none".into(),
        poll_ms: 10000,
        ..settings()
    };
    fake.0.borrow_mut().environments.extend([
        (0, env(vec![monitor(-800, 0, true, false, false)], None)),
        (100, env(vec![monitor(-1000, 0, true, false, false)], None)),
        (200, env(vec![monitor(-800, 0, true, false, false)], None)),
    ]);
    for _ in 0..3 {
        fake.0.borrow_mut().discoveries.push_back(Ok(discovery(vec![
            snapshot(1, true, 50),
            snapshot(10, false, 50),
        ])));
    }
    drive(fake.clone(), &mut bound_clock(&fake, 300), &s).unwrap();
    let d = fake.0.borrow();
    assert_eq!(d.discovery_times, [0, 100, 200]);
    for root in [1, 10] {
        assert!(d.events.contains(&format!("restore-position:{root}")));
    }
    assert_eq!(d.moves.len(), 4); // Both initially and both after resolution resumes.
    assert_eq!(d.features.len(), 2); // No unnecessary appearance reset/reapplication.
    assert_eq!(d.features[0].1.opacity, Some(0));
    assert_eq!(d.features[1].1.opacity, Some(100));
    assert!(!d.events.iter().any(|e| e.starts_with("restore-root")));
}

#[test]
fn maximization_follows_monitor_not_window_count_and_preserves_other_features_without_centering() {
    let mut state = State::default();
    let mut fake = Fake::default();
    let s = Settings {
        center: false,
        default_style_on_maximized: true,
        style: "transparent".into(),
        primary_opacity: Some(25),
        secondary_opacity: Some(75),
        hide_primary_start: true,
        hide_secondary_tray: true,
        corner_radius: 8,
        taskbar_segments: true,
        toolbar_compensation: false,
        ..settings()
    };
    for first_maximized in [true, false] {
        state.environment = Some(env(
            vec![
                monitor(-1000, 0, true, first_maximized, false),
                monitor(0, 1000, false, !first_maximized, false),
            ],
            None,
        ));
        state
            .refresh(
                &mut fake,
                discovery(vec![
                    snapshot(1, true, 50),
                    shift(snapshot(10, false, 50), 1000),
                ]),
                &s,
                100,
            )
            .unwrap();
    }
    let d = fake.0.borrow();
    assert_eq!(
        d.features
            .iter()
            .map(|(id, f)| (id.root, f.style))
            .collect::<Vec<_>>(),
        [
            (1, Style::Default),
            (10, Style::Transparent),
            (1, Style::Transparent),
            (10, Style::Default)
        ]
    );
    for (id, f) in &d.features {
        assert_eq!(f.opacity, Some(if id.root == 1 { 25 } else { 75 }));
        assert_eq!(f.hide_start, id.root == 1);
        assert_eq!(f.hide_tray, id.root == 10);
        assert!(f.segments && !f.toolbar_compensation);
        assert_eq!(f.corner_radius, 8);
    }
    assert!(d.moves.is_empty());
    assert!(!d.events.iter().any(|e| e.starts_with("restore")));
}

#[test]
fn unselection_restores_one_root_without_resetting_other_effects_or_animation() {
    let mut state = State::default();
    let mut fake = Fake::default();
    let mut s = Settings {
        style: "blur".into(),
        ..settings()
    };
    state
        .refresh(
            &mut fake,
            discovery(vec![snapshot(1, true, 50), snapshot(10, false, 50)]),
            &s,
            0,
        )
        .unwrap();
    s.secondary = false;
    state
        .refresh(
            &mut fake,
            discovery(vec![snapshot(1, true, 50), snapshot(10, false, 50)]),
            &s,
            50,
        )
        .unwrap();
    assert_eq!(state.animations.len(), 1);
    assert_eq!(state.animations[&snapshot(1, true, 50).id].started, 0);
    assert_eq!(
        fake.0.borrow().events,
        ["style:1", "style:10", "restore-root:10"]
    );
    fake.0.borrow_mut().fail_root_restore = true;
    s.primary = false;
    assert!(state
        .refresh(&mut fake, discovery(vec![snapshot(1, true, 50)]), &s, 60)
        .unwrap_err()
        .contains("root cleanup"));
}

#[test]
fn visibility_style_or_toolbar_geometry_invalidation_discards_frames_and_stabilizes_fresh_measurement(
) {
    for variant in 0..3 {
        let fake = Fake::default();
        fake.0.borrow_mut().geometry_changes.push_back(true);
        let mut s = settings();
        s.poll_ms = 10000;
        match variant {
            0 => s.hide_primary_tray = true,
            1 => s.style = "blur".into(),
            _ => s.toolbar_compensation = false,
        }
        let mut changed = snapshot(1, true, 80);
        changed.available_rect = rect(-800, -200);
        fake.0.borrow_mut().discoveries.extend([
            Ok(discovery(vec![snapshot(1, true, 50)])),
            Ok(discovery(vec![changed])),
        ]);
        drive(fake.clone(), &mut bound_clock(&fake, 70), &s).unwrap();
        let d = fake.0.borrow();
        assert_eq!(d.discovery_times, [0, 16]);
        assert_eq!(d.moves.len(), 1);
        assert_eq!(d.moves[0].1, 340);
        assert_eq!(*d.mutation_times.last().unwrap(), 16);
    }
}

#[test]
fn provider_selection_is_explicit_and_failure_cannot_fall_back_silently() {
    for (name, expected) in [
        ("msaa", MeasurementBackend::Msaa),
        ("uia", MeasurementBackend::UiAutomation),
    ] {
        let fake = Fake::default();
        let s = Settings {
            measurement: name.into(),
            ..settings()
        };
        drive(fake.clone(), &mut bound_clock(&fake, 100), &s).unwrap();
        assert_eq!(fake.0.borrow().providers, [expected]);
        fake.0.borrow_mut().fail_provider = true;
        fake.0.borrow_mut().discovery_times.clear();
        assert!(drive(fake.clone(), &mut Clock::until(100), &s)
            .unwrap_err()
            .contains("provider unavailable"));
        assert!(fake.0.borrow().discovery_times.is_empty());
    }
}

#[test]
fn overlap_fallback_uses_unclamped_offset_and_keeps_cross_axis() {
    let mut s = settings();
    s.primary_offset = 1000;
    s.revert_on_tray_overlap = true;
    let snap = snapshot(1, true, 50);
    assert_eq!(proposal(&snap, &s).unwrap().1, 40); // zero would clip reserved controls.
    s.revert_on_tray_overlap = false;
    assert_eq!(proposal(&snap, &s).unwrap().1, 640);
    let mut missing = snap;
    missing.content_rect = None;
    assert!(proposal(&missing, &s).is_none());
}

#[test]
fn battery_curve_change_retargets_from_measured_position_not_old_start_or_predicted_frame() {
    let fake = Fake::default();
    let s = Settings {
        poll_ms: 10000,
        animation_ms: 1000,
        battery_animation: Some("quadeasein".into()),
        ..settings()
    };
    fake.0.borrow_mut().environments.extend([
        (0, env(vec![], Some(false))),
        (100, env(vec![], Some(true))),
    ]);
    fake.0.borrow_mut().discoveries.extend([
        Ok(discovery(vec![snapshot(1, true, 50)])),
        Ok(discovery(vec![snapshot(1, true, 200)])), // Actual external/displayed position.
    ]);
    drive(fake.clone(), &mut bound_clock(&fake, 197), &s).unwrap();
    let d = fake.0.borrow();
    assert_eq!(d.discovery_times, [0, 100]);
    assert_eq!(d.moves.last().unwrap().1, 201); // quad(96/1000), from 200, not 50 or 78.
}

#[test]
fn slow_accessibility_is_revalidated_for_fullscreen_and_stable_measurements_do_not_starve() {
    for becomes_fullscreen in [false, true] {
        let fake = Fake::default();
        fake.0.borrow_mut().discovery_delay = 150;
        fake.0.borrow_mut().environments.extend([
            (0, env(vec![monitor(-1000, 0, true, false, false)], None)),
            (
                100,
                env(
                    vec![monitor(-1000, 0, true, false, becomes_fullscreen)],
                    None,
                ),
            ),
        ]);
        fake.0
            .borrow_mut()
            .discoveries
            .push_back(Ok(discovery(vec![snapshot(1, true, 50)])));
        let s = Settings {
            pause_on_fullscreen: true,
            animation: "none".into(),
            ..settings()
        };
        drive(fake.clone(), &mut bound_clock(&fake, 300), &s).unwrap();
        let d = fake.0.borrow();
        assert_eq!(d.discovery_times, [0]);
        if becomes_fullscreen {
            assert!(d.features.is_empty() && d.moves.is_empty());
        } else {
            assert_eq!(d.moves.len(), 1);
            assert!(d.mutation_times.contains(&150));
        }
    }
}

#[test]
fn fullscreen_and_skip_combination_defers_position_restore_until_resume() {
    let fake = Fake::default();
    let s = Settings {
        pause_on_fullscreen: true,
        skip_resolutions: vec![1000],
        poll_ms: 10000,
        animation: "none".into(),
        ..settings()
    };
    fake.0.borrow_mut().environments.extend([
        (0, env(vec![monitor(-800, 0, true, false, false)], None)),
        (100, env(vec![monitor(-1000, 0, true, false, true)], None)),
        (300, env(vec![monitor(-1000, 0, true, false, false)], None)),
    ]);
    fake.0
        .borrow_mut()
        .discoveries
        .push_back(Ok(discovery(vec![snapshot(1, true, 50)])));
    drive(fake.clone(), &mut bound_clock(&fake, 400), &s).unwrap();
    let d = fake.0.borrow();
    assert!(d.mutation_times.iter().all(|at| *at < 100 || *at >= 300));
    assert_eq!(
        d.events
            .iter()
            .filter(|e| *e == "restore-position:1")
            .count(),
        1
    );
    assert!(d.mutation_times.contains(&300));
}

#[test]
fn content_changes_still_animate_with_toolbar_compensation_enabled() {
    let mut state = State::default();
    let mut fake = Fake::default();
    let s = settings();
    state
        .refresh(&mut fake, discovery(vec![snapshot(1, true, 50)]), &s, 0)
        .unwrap();
    let mut changed = snapshot(1, true, 100);
    changed.content_rect.as_mut().unwrap().right -= 100;
    state
        .refresh(&mut fake, discovery(vec![changed]), &s, 50)
        .unwrap();
    state.frame(&mut fake, &s, 50).unwrap();
    assert!(fake.0.borrow().moves.is_empty());
    state.frame(&mut fake, &s, 100).unwrap();
    assert_eq!(fake.0.borrow().moves[0].1, 245); // Halfway from 100 to 390.
}

#[test]
fn auto_hidden_and_vertical_negative_taskbars_map_to_the_nearest_monitor() {
    let mut s = snapshot(1, true, 0);
    s.taskbar_rect = Rect {
        left: -1000,
        right: -960,
        top: -45,
        bottom: -5,
    };
    let e = env(
        vec![
            monitor(-1000, 0, true, true, false),
            monitor(0, 1000, false, false, false),
        ],
        None,
    );
    assert!(monitor_for(&s, &e).unwrap().maximized);
    s.vertical = true;
    s.taskbar_rect = Rect {
        left: 1000,
        right: 1040,
        top: 0,
        bottom: 40,
    };
    assert!(!monitor_for(&s, &e).unwrap().maximized);
}
