//! Accessibility-free environment inspection. Every call builds a fresh window
//! set (not a count cache); unknown queries are warnings, never policy decisions.
use crate::platform::{class, enumerate, error, rect};
use crate::{EnvironmentSnapshot, MonitorState, Rect};
use std::mem::size_of;
use windows::{
    core::BOOL,
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        Graphics::{
            Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS},
            Gdi::*,
        },
        System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS},
        UI::WindowsAndMessaging::*,
    },
};

fn battery(ac: u8) -> Option<bool> {
    match ac {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}
fn shell(c: &str) -> bool {
    matches!(
        c,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "Shell_DesktopWnd"
            | "DV2ControlHost"
    )
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Flags {
    visible: bool,
    minimized: bool,
    cloaked: bool,
    desktop: bool,
}
impl Flags {
    fn qualifies(self) -> bool {
        self.visible && !self.minimized && !self.cloaked && !self.desktop
    }
}
struct WindowState {
    maximized: bool,
    monitor: isize,
    bounds: Rect,
}
type MonitorList = (Vec<(isize, MonitorState)>, Vec<String>);
// The entire inspection algorithm is testable without COM or live windows.
trait Reader {
    fn power(&mut self) -> Result<Option<bool>, String>;
    fn monitors(&mut self) -> Result<MonitorList, String>;
    fn foreground(&mut self) -> isize;
    fn windows(&mut self) -> Result<Vec<isize>, String>;
    fn flags(&mut self, h: isize) -> Result<Flags, String>;
    fn window(&mut self, h: isize, foreground: bool) -> Result<WindowState, String>;
}
fn inspect_with(api: &mut impl Reader) -> Result<EnvironmentSnapshot, String> {
    let mut out = EnvironmentSnapshot {
        on_battery: None,
        monitors: vec![],
        warnings: vec![],
    };
    match api.power() {
        Ok(p) => {
            out.on_battery = p;
            if p.is_none() {
                out.warnings.push("Power source is unknown".into());
            }
        }
        Err(e) => out.warnings.push(e),
    }
    let (mut monitors, errors) = api.monitors()?;
    out.warnings.extend(errors);
    if monitors.is_empty() {
        return Err("No monitor bounds could be queried; environment unknown".into());
    }
    let foreground = api.foreground();
    let windows = api.windows()?;
    if foreground == 0 || !windows.contains(&foreground) {
        out.warnings
            .push("Foreground unavailable or not enumerated; fullscreen state uncertain".into());
    }
    for w in windows {
        let result: Result<(), String> = (|| {
            let flags = api.flags(w)?;
            if !flags.qualifies() {
                return Ok(());
            }
            let state = api.window(w, w == foreground)?;
            if api.flags(w)? != flags {
                return Err(
                    "Window visibility/cloaking changed during environment inspection".into(),
                );
            }
            if !monitors.iter().any(|(h, _)| *h == state.monitor) {
                return Err("Window monitor unavailable; policy state uncertain".into());
            }
            for (h, m) in &mut monitors {
                m.maximized |= state.maximized && *h == state.monitor;
                // Physical monitor bounds, never rcWork. Spanning games qualify
                // on every monitor they fully cover, only when foreground.
                m.fullscreen |= w == foreground && state.bounds.contains(m.bounds);
            }
            Ok(())
        })();
        if let Err(e) = result {
            out.warnings.push(format!("Window {w:#x}: {e}"));
        }
    }
    if api.foreground() != foreground {
        out.warnings.push(
            "Foreground changed during environment inspection; fullscreen state uncertain".into(),
        );
    }
    out.monitors = monitors.into_iter().map(|(_, m)| m).collect();
    Ok(out)
}
struct Native;
struct Monitors {
    entries: Vec<(isize, MonitorState)>,
    errors: Vec<String>,
}
unsafe extern "system" fn monitor(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let out = &mut *(data.0 as *mut Monitors);
    if out.entries.len() >= 128 {
        out.errors.push("Monitor enumeration limit exceeded".into());
        return BOOL(0);
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(h, &mut info).as_bool() {
        out.errors
            .push("GetMonitorInfoW failed; monitor policy uncertain".into());
        return BOOL(1);
    }
    let b = info.rcMonitor;
    let bounds = Rect {
        left: b.left,
        top: b.top,
        right: b.right,
        bottom: b.bottom,
    };
    if !bounds.valid() {
        out.errors
            .push("Invalid monitor bounds; monitor policy uncertain".into());
        return BOOL(1);
    }
    out.entries.push((
        h.0 as isize,
        MonitorState {
            bounds,
            primary: info.dwFlags & 1 != 0,
            maximized: false,
            fullscreen: false,
        },
    ));
    BOOL(1)
}
impl Reader for Native {
    fn power(&mut self) -> Result<Option<bool>, String> {
        let mut p = SYSTEM_POWER_STATUS::default();
        unsafe { GetSystemPowerStatus(&mut p) }.map_err(|e| error("GetSystemPowerStatus", e))?;
        Ok(battery(p.ACLineStatus))
    }
    fn monitors(&mut self) -> Result<MonitorList, String> {
        let mut out = Monitors {
            entries: vec![],
            errors: vec![],
        };
        if !unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(monitor),
                LPARAM((&mut out as *mut Monitors) as isize),
            )
        }
        .as_bool()
        {
            return Err("EnumDisplayMonitors failed; monitor policy unknown".into());
        }
        Ok((out.entries, out.errors))
    }
    fn foreground(&mut self) -> isize {
        unsafe { GetForegroundWindow() }.0 as isize
    }
    fn windows(&mut self) -> Result<Vec<isize>, String> {
        Ok(enumerate(None)?.into_iter().map(|h| h.0 as isize).collect())
    }
    fn flags(&mut self, h: isize) -> Result<Flags, String> {
        let w = HWND(h as *mut _);
        if !unsafe { IsWindow(Some(w)) }.as_bool() {
            return Err("Window disappeared during environment inspection".into());
        }
        let mut f = Flags {
            visible: unsafe { IsWindowVisible(w) }.as_bool(),
            minimized: unsafe { IsIconic(w) }.as_bool(),
            desktop: w == unsafe { GetShellWindow() }
                || w == unsafe { GetDesktopWindow() }
                || shell(&class(w)?),
            cloaked: false,
        };
        if f.visible && !f.minimized && !f.desktop {
            let mut cloak = 0u32;
            unsafe {
                DwmGetWindowAttribute(
                    w,
                    DWMWA_CLOAKED,
                    (&mut cloak as *mut u32).cast(),
                    size_of::<u32>() as u32,
                )
            }
            .map_err(|e| error("Cloaking query uncertain", e))?;
            f.cloaked = cloak != 0;
        }
        Ok(f)
    }
    fn window(&mut self, h: isize, foreground: bool) -> Result<WindowState, String> {
        let w = HWND(h as *mut _);
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        unsafe { GetWindowPlacement(w, &mut placement) }
            .map_err(|e| error("Window placement uncertain", e))?;
        let bounds = if foreground {
            let mut r = RECT::default();
            unsafe {
                DwmGetWindowAttribute(
                    w,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    (&mut r as *mut RECT).cast(),
                    size_of::<RECT>() as u32,
                )
            }
            .map_err(|e| error("Foreground frame bounds uncertain", e))?;
            let r = Rect {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            };
            if !r.valid() {
                return Err("Foreground returned invalid physical bounds".into());
            }
            r
        } else {
            rect(w)?
        };
        Ok(WindowState {
            maximized: placement.showCmd == SW_SHOWMAXIMIZED.0 as u32,
            monitor: unsafe { MonitorFromWindow(w, MONITOR_DEFAULTTONULL) }.0 as isize,
            bounds,
        })
    }
}
pub(crate) fn inspect() -> Result<EnvironmentSnapshot, String> {
    inspect_with(&mut Native)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        flags: Flags,
        foreground: isize,
        monitor: isize,
        bounds: Rect,
        power: Result<Option<bool>, String>,
        query_error: bool,
        maxed: bool,
    }
    fn rects() -> [Rect; 2] {
        [
            Rect {
                left: -1920,
                top: -1080,
                right: 0,
                bottom: 0,
            },
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
        ]
    }
    fn fake() -> Fake {
        Fake {
            flags: Flags {
                visible: true,
                minimized: false,
                cloaked: false,
                desktop: false,
            },
            foreground: 9,
            monitor: 1,
            bounds: rects()[0],
            power: Ok(Some(false)),
            query_error: false,
            maxed: true,
        }
    }
    impl Reader for Fake {
        fn power(&mut self) -> Result<Option<bool>, String> {
            self.power.clone()
        }
        fn monitors(&mut self) -> Result<MonitorList, String> {
            Ok((
                rects()
                    .into_iter()
                    .enumerate()
                    .map(|(i, bounds)| {
                        (
                            (i + 1) as isize,
                            MonitorState {
                                bounds,
                                primary: i == 1,
                                maximized: false,
                                fullscreen: false,
                            },
                        )
                    })
                    .collect(),
                vec![],
            ))
        }
        fn foreground(&mut self) -> isize {
            self.foreground
        }
        fn windows(&mut self) -> Result<Vec<isize>, String> {
            Ok(vec![9])
        }
        fn flags(&mut self, _: isize) -> Result<Flags, String> {
            if self.query_error {
                Err("cloaking query failed".into())
            } else {
                Ok(self.flags)
            }
        }
        fn window(&mut self, _: isize, _: bool) -> Result<WindowState, String> {
            Ok(WindowState {
                maximized: self.maxed,
                monitor: self.monitor,
                bounds: self.bounds,
            })
        }
    }
    #[test]
    fn power_and_shell_filtering() {
        assert_eq!(battery(0), Some(true));
        assert_eq!(battery(1), Some(false));
        assert_eq!(battery(255), None);
        assert_eq!(battery(2), None);
        for c in [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
        ] {
            assert!(shell(c));
        }
        assert!(!shell("ApplicationFrameWindow"));
        assert!(!shell("Windows.UI.Core.CoreWindow"));
    }
    #[test]
    fn hidden_minimized_cloaked_and_desktop_never_qualify() {
        for n in 0..4 {
            let mut f = fake();
            match n {
                0 => f.flags.visible = false,
                1 => f.flags.minimized = true,
                2 => f.flags.cloaked = true,
                _ => f.flags.desktop = true,
            };
            let s = inspect_with(&mut f).unwrap();
            assert!(s.monitors.iter().all(|m| !m.maximized && !m.fullscreen));
        }
    }
    #[test]
    fn same_count_window_moves_to_different_negative_coordinate_monitor() {
        let mut f = fake();
        let a = inspect_with(&mut f).unwrap();
        assert!(a.monitors[0].maximized && a.monitors[0].fullscreen);
        assert!(!a.monitors[1].maximized);
        f.monitor = 2;
        f.bounds = rects()[1];
        let b = inspect_with(&mut f).unwrap();
        assert!(!b.monitors[0].maximized && !b.monitors[0].fullscreen);
        assert!(b.monitors[1].maximized && b.monitors[1].fullscreen);
    }
    #[test]
    fn fullscreen_requires_foreground_and_monitor_not_work_area() {
        let mut f = fake();
        f.bounds.bottom -= 40;
        let s = inspect_with(&mut f).unwrap();
        assert!(s.monitors[0].maximized);
        assert!(!s.monitors[0].fullscreen);
        f.bounds = rects()[0];
        f.foreground = 0;
        assert!(!inspect_with(&mut f).unwrap().monitors[0].fullscreen);
    }
    #[test]
    fn uncertainty_reports_warnings_and_never_assumes_battery_state() {
        let mut f = fake();
        f.query_error = true;
        f.power = Err("power query failed".into());
        let s = inspect_with(&mut f).unwrap();
        assert!(s.on_battery.is_none());
        assert_eq!(s.warnings.len(), 2);
        assert!(!s.monitors[0].maximized);
        f.power = Ok(None);
        assert!(inspect_with(&mut f)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.contains("unknown")));
    }
}
