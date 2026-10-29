use super::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use taskbarx_win32::{
    Discovery, EnvironmentSnapshot, MeasurementBackend, Point, TaskbarFeatures, TaskbarId,
};

#[derive(Default)]
struct Data {
    log: Vec<String>,
    duplicate: bool,
    stop: bool,
    stop_on_restore: bool,
    stop_on_close: bool,
    fail_restore: bool,
    fail_close: bool,
    fail_factory: bool,
    panic_discover: bool,
    panic_backend_drop: bool,
    fullscreen: bool,
    stop_message_after_restore: bool,
    now: u64,
    actions: VecDeque<Vec<TrayAction>>,
    contexts: Vec<RunContext>,
}
type Shared = Rc<RefCell<Data>>;
struct Lease(Shared);
impl Control for Lease {
    fn now_ms(&self) -> u64 {
        self.0.borrow().now
    }
    fn complete(&mut self, ok: bool) -> Result<(), String> {
        self.0.borrow_mut().log.push(format!("complete:{ok}"));
        Ok(())
    }
    fn stopped(&mut self) -> Result<bool, String> {
        let mut d = self.0.borrow_mut();
        if d.stop_message_after_restore && d.log.iter().any(|e| e == "backend dropped") {
            // Simulate DispatchMessage running the tray callback on the first
            // owner-thread message pump AFTER a blocking native restoration.
            d.actions.push_front(vec![TrayAction::Stop]);
            d.stop_message_after_restore = false;
        }
        Ok(d.stop || d.now >= 300)
    }
    fn wait(&mut self, ms: u64) -> Result<(), String> {
        assert!(ms > 0 && ms <= 50);
        self.0.borrow_mut().now += ms;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.borrow_mut().log.push("lease released".into());
    }
}
struct FakeCoordinator(Shared);
impl Coordinator for FakeCoordinator {
    type Lease = Lease;
    fn claim(&mut self) -> Result<Option<Lease>, String> {
        self.0.borrow_mut().log.push("claim".into());
        Ok((!self.0.borrow().duplicate).then(|| Lease(self.0.clone())))
    }
    fn stop_existing(&mut self, _: u32) -> Result<(), String> {
        self.0.borrow_mut().log.push("external stop".into());
        Ok(())
    }
}
struct FakeBackend(Shared);
impl Backend for FakeBackend {
    fn environment(&mut self) -> Result<EnvironmentSnapshot, String> {
        Ok(EnvironmentSnapshot {
            on_battery: None,
            monitors: if self.0.borrow().fullscreen {
                vec![taskbarx_win32::MonitorState {
                    bounds: taskbarx_win32::Rect {
                        left: 0,
                        top: 0,
                        right: 1000,
                        bottom: 800,
                    },
                    primary: true,
                    maximized: false,
                    fullscreen: true,
                }]
            } else {
                vec![]
            },
            warnings: vec![],
        })
    }
    fn set_measurement_backend(&mut self, _: MeasurementBackend) -> Result<(), String> {
        Ok(())
    }
    fn discover(&mut self) -> Result<Discovery, String> {
        assert!(!self.0.borrow().panic_discover, "injected panic");
        Ok(Discovery {
            build_number: 19045,
            native_windows11: false,
            taskbars: vec![],
            warnings: vec![],
        })
    }
    fn move_list(&mut self, _: &TaskbarId, _: Point) -> Result<(), String> {
        panic!()
    }
    fn restore_position(&mut self, _: &TaskbarId) -> Result<(), String> {
        panic!()
    }
    fn apply_features(&mut self, _: &TaskbarId, _: &TaskbarFeatures) -> Result<bool, String> {
        panic!()
    }
    fn restore_taskbar(&mut self, _: &TaskbarId) -> Vec<String> {
        panic!()
    }
    fn restore(&mut self) -> Vec<String> {
        let mut d = self.0.borrow_mut();
        d.log.push("restore".into());
        d.stop |= d.stop_on_restore;
        if d.fail_restore {
            vec!["injected restoration failure".into()]
        } else {
            vec![]
        }
    }
}
impl Drop for FakeBackend {
    fn drop(&mut self) {
        self.0.borrow_mut().log.push("backend dropped".into());
        assert!(!self.0.borrow().panic_backend_drop, "backend drop panic");
    }
}
struct FakeTray(Shared);
impl Tray for FakeTray {
    fn poll(&mut self) -> Result<Vec<TrayAction>, String> {
        Ok(self.0.borrow_mut().actions.pop_front().unwrap_or_default())
    }
    fn close(&mut self) -> Result<(), String> {
        let mut d = self.0.borrow_mut();
        d.log.push("tray closed".into());
        d.stop |= d.stop_on_close;
        if d.fail_close {
            Err("injected close failure".into())
        } else {
            Ok(())
        }
    }
}
impl Drop for FakeTray {
    fn drop(&mut self) {
        self.0.borrow_mut().log.push("tray dropped".into());
    }
}
struct FakeShell(Shared);
impl RuntimeShell for FakeShell {
    type Tray = FakeTray;
    fn prepare_console(&mut self, background: bool, show: bool) -> Result<(), String> {
        self.0
            .borrow_mut()
            .log
            .push(format!("console:{background}:{show}"));
        Ok(())
    }
    fn create_tray(&mut self) -> Result<FakeTray, String> {
        self.0.borrow_mut().log.push("tray created".into());
        Ok(FakeTray(self.0.clone()))
    }
    fn open_settings(&mut self, context: &RunContext) -> Result<(), String> {
        self.0.borrow_mut().contexts.push(context.clone());
        Ok(())
    }
}
fn context() -> RunContext {
    RunContext {
        engine: PathBuf::from(r"C:\Program Files\TaskbarX\taskbarx.exe"),
        config: PathBuf::from("C:/config space/\u{96ea}.json"),
        background: true,
    }
}
fn start(data: &Shared, settings: &Settings) -> Result<(), String> {
    start_with(
        &mut FakeCoordinator(data.clone()),
        || {
            let mut d = data.borrow_mut();
            d.log.push("backend created".into());
            if d.fail_factory {
                Err("COM unavailable".into())
            } else {
                Ok(FakeBackend(data.clone()))
            }
        },
        &mut FakeShell(data.clone()),
        settings,
        &context(),
    )
}
fn tray_settings() -> Settings {
    Settings {
        tray_icon: true,
        ..Settings::default()
    }
}

#[test]
fn duplicate_never_creates_backend_tray_or_console() {
    let d = Shared::default();
    d.borrow_mut().duplicate = true;
    assert!(start(&d, &tray_settings())
        .unwrap_err()
        .contains("already running"));
    assert_eq!(d.borrow().log, ["claim"]);
}
#[test]
fn cleanup_and_backend_drop_precede_tray_completion_and_instance_release() {
    let d = Shared::default();
    d.borrow_mut().actions.push_back(vec![TrayAction::Stop]);
    start(&d, &tray_settings()).unwrap();
    assert_eq!(
        d.borrow().log,
        [
            "claim",
            "console:true:false",
            "tray created",
            "backend created",
            "restore",
            "backend dropped",
            "tray closed",
            "tray dropped",
            "complete:true",
            "lease released"
        ]
    );
}
#[test]
fn no_tray_unless_explicit_and_console_override_reaches_shell() {
    let d = Shared::default();
    start(
        &d,
        &Settings {
            show_console: true,
            ..Settings::default()
        },
    )
    .unwrap();
    assert!(d.borrow().log.contains(&"console:true:true".into()));
    assert!(!d.borrow().log.iter().any(|e| e.starts_with("tray")));
}
#[test]
fn absent_stop_does_not_construct_session_or_claim_runtime() {
    let d = Shared::default();
    FakeCoordinator(d.clone()).stop_existing(100).unwrap();
    assert_eq!(d.borrow().log, ["external stop"]);
}
#[test]
fn failed_session_creation_closes_tray_and_does_not_publish_success() {
    let d = Shared::default();
    d.borrow_mut().fail_factory = true;
    assert_eq!(start(&d, &tray_settings()).unwrap_err(), "COM unavailable");
    assert_eq!(
        &d.borrow().log[4..],
        [
            "tray closed",
            "tray dropped",
            "complete:false",
            "lease released"
        ]
    );
}
#[test]
fn internal_restart_restores_and_drops_old_backend_before_replacement_without_self_stop() {
    let d = Shared::default();
    d.borrow_mut()
        .actions
        .extend([vec![TrayAction::Restart], vec![], vec![TrayAction::Stop]]);
    start(&d, &tray_settings()).unwrap();
    assert_eq!(d.borrow().log.iter().filter(|e| *e == "claim").count(), 1);
    assert_eq!(
        d.borrow()
            .log
            .iter()
            .filter(|e| *e == "backend created")
            .count(),
        2
    );
    assert!(!d.borrow().log.contains(&"external stop".into()));
    assert_eq!(
        &d.borrow().log[4..10],
        [
            "restore",
            "backend dropped",
            "tray closed",
            "tray dropped",
            "tray created",
            "backend created"
        ]
    );
}
#[test]
fn stop_in_batch_or_during_cleanup_preempts_restart() {
    for race in 0..3 {
        let d = Shared::default();
        d.borrow_mut().stop_on_restore = race == 1;
        d.borrow_mut().stop_on_close = race == 2;
        d.borrow_mut().actions.push_back(if race == 0 {
            vec![
                TrayAction::Restart,
                TrayAction::Stop,
                TrayAction::OpenSettings,
            ]
        } else {
            vec![TrayAction::Restart]
        });
        start(&d, &tray_settings()).unwrap();
        assert_eq!(
            d.borrow()
                .log
                .iter()
                .filter(|e| *e == "backend created")
                .count(),
            1
        );
        assert!(d.borrow().contexts.is_empty());
    }
}
#[test]
fn failed_backend_or_tray_cleanup_prevents_restart_and_named_success() {
    for tray_failure in [false, true] {
        let d = Shared::default();
        d.borrow_mut().fail_close = tray_failure;
        d.borrow_mut().fail_restore = !tray_failure;
        d.borrow_mut().actions.push_back(vec![TrayAction::Restart]);
        assert!(start(&d, &tray_settings()).is_err());
        assert_eq!(
            d.borrow()
                .log
                .iter()
                .filter(|e| *e == "backend created")
                .count(),
            1
        );
        assert!(d.borrow().log.contains(&"complete:false".into()));
        assert!(!d.borrow().log.contains(&"complete:true".into()));
    }
}
#[test]
fn panic_restores_backend_and_closes_tray_without_success_acknowledgement() {
    let d = Shared::default();
    d.borrow_mut().panic_discover = true;
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| start(&d, &tray_settings())))
            .is_err()
    );
    assert!(d.borrow().log.contains(&"restore".into()));
    assert!(d.borrow().log.contains(&"tray closed".into()));
    assert!(!d.borrow().log.contains(&"complete:true".into()));
}
#[test]
fn tray_open_settings_preserves_absolute_engine_and_config_context() {
    let d = Shared::default();
    d.borrow_mut()
        .actions
        .extend([vec![TrayAction::OpenSettings], vec![TrayAction::Stop]]);
    start(&d, &tray_settings()).unwrap();
    assert_eq!(d.borrow().contexts, [context()]);
}

#[test]
fn tray_stop_queued_during_restoration_is_not_lost_when_old_tray_is_closed() {
    let d = Shared::default();
    d.borrow_mut()
        .actions
        .extend([vec![TrayAction::Restart], vec![TrayAction::Stop]]);
    start(&d, &tray_settings()).unwrap();
    assert_eq!(
        d.borrow()
            .log
            .iter()
            .filter(|e| *e == "backend created")
            .count(),
        1
    );
    assert!(d.borrow().log.contains(&"complete:true".into()));
}

#[test]
fn restart_pumps_pending_stop_callback_before_polling_old_tray_queue() {
    let d = Shared::default();
    d.borrow_mut().stop_message_after_restore = true;
    d.borrow_mut().actions.push_back(vec![TrayAction::Restart]);
    start(&d, &tray_settings()).unwrap();
    assert_eq!(
        d.borrow()
            .log
            .iter()
            .filter(|e| *e == "backend created")
            .count(),
        1
    );
    assert!(d.borrow().log.contains(&"complete:true".into()));
}

#[test]
fn backend_drop_panic_cannot_publish_success_or_construct_replacement() {
    let d = Shared::default();
    d.borrow_mut().panic_backend_drop = true;
    d.borrow_mut().actions.push_back(vec![TrayAction::Restart]);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| start(&d, &tray_settings())))
            .is_err()
    );
    assert!(!d.borrow().log.contains(&"complete:true".into()));
    assert_eq!(
        d.borrow()
            .log
            .iter()
            .filter(|e| *e == "backend created")
            .count(),
        1
    );
    assert!(d.borrow().log.contains(&"tray closed".into()));
}

#[test]
fn paused_engine_keeps_tray_stop_responsive_and_cleanup_failure_is_not_acknowledged() {
    let d = Shared::default();
    d.borrow_mut().fullscreen = true;
    d.borrow_mut().panic_discover = true; // Any accessibility polling fails this test.
    d.borrow_mut().fail_close = true;
    d.borrow_mut()
        .actions
        .extend([vec![], vec![TrayAction::Stop]]);
    let settings = Settings {
        pause_on_fullscreen: true,
        poll_ms: 10000,
        ..tray_settings()
    };
    assert!(start(&d, &settings).unwrap_err().contains("close failure"));
    assert!(d.borrow().now <= 50);
    assert!(!d.borrow().log.contains(&"complete:true".into()));
    assert!(d.borrow().log.contains(&"restore".into()));
}
