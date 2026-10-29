use taskbarx_win32::{
    Discovery, EnvironmentSnapshot, MeasurementBackend, MonitorState, Point, Rect, Session, Style,
    TaskbarFeatures, TaskbarId, TaskbarSnapshot,
};

// Compile the fixed contract without constructing a Windows session or invoking
// any real mutation API. Integration consumers need no other TaskbarX crate.
#[test]
fn fixed_interface_compiles() {
    let _: fn() -> Result<Session, String> = Session::new;
    let _: fn(&mut Session) -> Result<Discovery, String> = Session::discover;
    let _: fn(&mut Session, &TaskbarId, Point) -> Result<(), String> = Session::move_task_list;
    let _: fn(&mut Session, &TaskbarId) -> Result<(), String> = Session::restore_position;
    let _: fn(&mut Session, &TaskbarId, Style, [u8; 4]) -> Result<(), String> = Session::set_style;
    let _: fn(&mut Session) -> Vec<String> = Session::restore;
    let _: fn(&mut Session) -> Result<EnvironmentSnapshot, String> = Session::environment;
    let _: fn(&mut Session, MeasurementBackend) -> Result<(), String> =
        Session::set_measurement_backend;
    let _: fn(&mut Session, &TaskbarId, &TaskbarFeatures) -> Result<bool, String> =
        Session::apply_features;
    let _: fn(&mut Session, &TaskbarId) -> Vec<String> = Session::restore_taskbar;
    fn copy_eq<T: Copy + Eq + std::fmt::Debug>() {}
    fn debug_clone<T: Clone + std::fmt::Debug>() {}
    fn hash<T: std::hash::Hash>() {}
    copy_eq::<Rect>();
    copy_eq::<Point>();
    copy_eq::<TaskbarId>();
    copy_eq::<Style>();
    debug_clone::<TaskbarSnapshot>();
    debug_clone::<Discovery>();
    debug_clone::<EnvironmentSnapshot>();
    debug_clone::<MonitorState>();
    debug_clone::<TaskbarFeatures>();
    copy_eq::<MeasurementBackend>();
    let f = TaskbarFeatures {
        style: Style::TransparentGradient,
        color: [1, 2, 3, 4],
        opacity: Some(100),
        corner_radius: 8,
        segments: true,
        hide_start: true,
        hide_tray: false,
        toolbar_compensation: false,
    };
    assert_eq!(f, f.clone());
    let d = TaskbarFeatures::default();
    assert_eq!(
        (d.style, d.color, d.opacity, d.corner_radius),
        (Style::Default, [0, 0, 0, 128], None, 0)
    );
    assert!(!d.segments && !d.hide_start && !d.hide_tray && d.toolbar_compensation);
    let m = MonitorState {
        bounds: Rect {
            left: -1,
            top: -2,
            right: 3,
            bottom: 4,
        },
        primary: false,
        maximized: false,
        fullscreen: false,
    };
    let e = EnvironmentSnapshot {
        on_battery: None,
        monitors: vec![m],
        warnings: vec![],
    };
    assert_eq!(e.monitors.len(), 1);
    assert_ne!(MeasurementBackend::Msaa, MeasurementBackend::UiAutomation);
    hash::<TaskbarId>();
}

#[cfg(not(windows))]
#[test]
fn unsupported_platform_is_an_error_not_a_mock_backend() {
    assert!(Session::new().err().unwrap().contains("unsupported"));
}
