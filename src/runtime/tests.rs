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
}
#[derive(Clone, Default)]
struct Fake(Rc<RefCell<Data>>);
impl Backend for Fake {
    fn discover(&mut self) -> Result<Discovery, String> {
        let mut data = self.0.borrow_mut();
        data.events.push("discover".into());
        data.discoveries
            .pop_front()
            .unwrap_or_else(|| Ok(discovery(vec![])))
    }
    fn move_list(&mut self, id: &TaskbarId, point: Point) -> Result<(), String> {
        let mut data = self.0.borrow_mut();
        data.events.push(format!("move:{}", id.root));
        data.moves.push((*id, point.x, point.y));
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
        self.0
            .borrow_mut()
            .events
            .push(format!("restore-position:{}", id.root));
        Ok(())
    }
    fn set_style(&mut self, id: &TaskbarId, _: Style, _: [u8; 4]) -> Result<(), String> {
        let mut data = self.0.borrow_mut();
        data.events.push(format!("style:{}", id.root));
        if data.fail_style || data.fail_style_root == Some(id.root) {
            Err("injected partial style failure".into())
        } else {
            Ok(())
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
}
impl Clock {
    fn until(stop_at: u64) -> Self {
        Self {
            now: 0,
            stop_at,
            waits: vec![],
            wake_cap: u64::MAX,
        }
    }
}
impl Control for Clock {
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn stopped(&mut self) -> Result<bool, String> {
        Ok(self.now >= self.stop_at)
    }
    fn wait(&mut self, ms: u64) -> Result<(), String> {
        assert!(ms > 0, "must never busy-loop");
        self.waits.push(ms);
        self.now += ms.min(self.wake_cap);
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
fn deselecting_styled_taskbar_rolls_back_before_fresh_measurement() {
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
    assert_eq!(backend.0.borrow().events, ["style:1", "restore-all"]);
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
    assert!(clock.waits.contains(&250));
    assert!(clock.waits.contains(&500));
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
    assert_eq!(clock.waits, [400, 400, 400]);
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
    assert_eq!(clock.waits[0], 250);
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
    assert_eq!(&clock.waits[..5], [250, 500, 1000, 2000, 4000]);
    assert!(clock.waits.iter().all(|ms| *ms <= 4000));
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
