use taskbarx_win32::{Discovery, Point, Rect, Session, Style, TaskbarId, TaskbarSnapshot};

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
    fn copy_eq<T: Copy + Eq + std::fmt::Debug>() {}
    fn debug_clone<T: Clone + std::fmt::Debug>() {}
    fn hash<T: std::hash::Hash>() {}
    copy_eq::<Rect>();
    copy_eq::<Point>();
    copy_eq::<TaskbarId>();
    copy_eq::<Style>();
    debug_clone::<TaskbarSnapshot>();
    debug_clone::<Discovery>();
    hash::<TaskbarId>();
}

#[cfg(not(windows))]
#[test]
fn unsupported_platform_is_an_error_not_a_mock_backend() {
    assert!(Session::new().err().unwrap().contains("unsupported"));
}
