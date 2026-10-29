// TaskbarX topology/accent adaptations: Copyright (c) 2021 Chris Andriessen.
// Original MIT terms are in upstream-license.txt.
use crate::{
    accessibility,
    geometry::available,
    state::{Accent, Api},
    Discovery, Point, Rect, TaskbarId, TaskbarSnapshot,
};
use std::{
    ffi::c_void,
    marker::PhantomData,
    mem::{size_of, transmute},
    rc::Rc,
};
use windows::{
    core::{s, w, BOOL, PCSTR, PCWSTR},
    Win32::{
        Foundation::{
            CloseHandle, FILETIME, HANDLE, HWND, LPARAM, POINT, RECT, RPC_E_CHANGED_MODE, WPARAM,
        },
        Graphics::Gdi::{
            ClientToScreen, MonitorFromWindow, ScreenToClient, MONITOR_DEFAULTTONEAREST,
        },
        System::{
            Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            HiDpi::{
                GetDpiForMonitor, GetProcessDpiAwareness, SetProcessDpiAwareness,
                DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
                PROCESS_PER_MONITOR_DPI_AWARE,
            },
            WindowsAndMessaging::*,
        },
    },
};

fn hwnd(value: isize) -> HWND {
    HWND(value as *mut c_void)
}
fn error(context: &str, e: impl std::fmt::Display) -> String {
    format!("{context}: {e}")
}
fn last_error(context: &str) -> String {
    error(context, windows::core::Error::from_win32())
}

struct ComApartment {
    uninitialize: bool,
    _thread_bound: PhantomData<Rc<()>>,
}
impl ComApartment {
    fn new() -> Result<Self, String> {
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        // An already initialized MTA is valid for these synchronous calls. It is
        // borrowed, not owned; only S_OK/S_FALSE require a matching uninitialize.
        if hr.is_err() && hr != RPC_E_CHANGED_MODE {
            return Err(error("CoInitializeEx", hr));
        }
        Ok(Self {
            uninitialize: hr.is_ok(),
            _thread_bound: PhantomData,
        })
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.uninitialize {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

type SetThreadDpi = unsafe extern "system" fn(DPI_AWARENESS_CONTEXT) -> DPI_AWARENESS_CONTEXT;
type WindowDpi = unsafe extern "system" fn(HWND) -> u32;
#[derive(Clone, Copy)]
struct Dpi {
    set_thread: Option<SetThreadDpi>,
    window_dpi: Option<WindowDpi>,
}
struct DpiScope {
    restore: Option<(SetThreadDpi, DPI_AWARENESS_CONTEXT)>,
}
impl Drop for DpiScope {
    fn drop(&mut self) {
        if let Some((set, old)) = self.restore {
            unsafe {
                set(old);
            }
        }
    }
}
impl Dpi {
    fn new() -> Result<Self, String> {
        // Establish process awareness before any measurement. The process may
        // already have created UI; scoped thread overrides then avoid changing
        // the host's DPI behavior. Older Windows 10 uses per-monitor V1 process
        // awareness and rejects an incompatible host instead of virtualizing.
        unsafe {
            let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
        }
        let d = unsafe {
            Self {
                set_thread: proc(w!("user32.dll"), s!("SetThreadDpiAwarenessContext"))
                    .map(|p| transmute::<unsafe extern "system" fn() -> isize, SetThreadDpi>(p)),
                window_dpi: proc(w!("user32.dll"), s!("GetDpiForWindow"))
                    .map(|p| transmute::<unsafe extern "system" fn() -> isize, WindowDpi>(p)),
            }
        };
        let _scope = d.enter()?;
        Ok(d)
    }
    fn enter(self) -> Result<DpiScope, String> {
        if let Some(set) = self.set_thread {
            let mut old = unsafe { set(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            if old.0.is_null() {
                old = unsafe { set(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) };
            }
            if old.0.is_null() {
                return Err(last_error("Cannot enter per-monitor DPI context"));
            }
            Ok(DpiScope {
                restore: Some((set, old)),
            })
        } else {
            let awareness = unsafe { GetProcessDpiAwareness(None) }
                .map_err(|e| error("GetProcessDpiAwareness", e))?;
            if awareness != PROCESS_PER_MONITOR_DPI_AWARE {
                return Err("Per-monitor DPI awareness is required; host has an incompatible process context".into());
            }
            Ok(DpiScope { restore: None })
        }
    }
    fn measure(self, root: HWND) -> Result<u32, String> {
        if let Some(get) = self.window_dpi {
            let dpi = unsafe { get(root) };
            if dpi == 0 {
                return Err("GetDpiForWindow returned zero".into());
            }
            Ok(dpi)
        } else {
            let (mut x, mut y) = (0, 0);
            unsafe {
                GetDpiForMonitor(
                    MonitorFromWindow(root, MONITOR_DEFAULTTONEAREST),
                    MDT_EFFECTIVE_DPI,
                    &mut x,
                    &mut y,
                )
            }
            .map_err(|e| error("GetDpiForMonitor", e))?;
            if x == 0 || x != y {
                return Err("Unsupported/unknown monitor DPI".into());
            }
            Ok(x)
        }
    }
}

// These DLLs are already loaded/imported for the process lifetime. No module
// references are leaked and no library path can be supplied by untrusted input.
unsafe fn proc(module: PCWSTR, name: PCSTR) -> Option<unsafe extern "system" fn() -> isize> {
    GetProcAddress(GetModuleHandleW(module).ok()?, name)
}

#[repr(C)]
struct Version {
    size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform: u32,
    service_pack: [u16; 128],
}
fn build_number() -> Result<u32, String> {
    type RtlGetVersion = unsafe extern "system" fn(*mut Version) -> i32;
    let f: RtlGetVersion = unsafe {
        transmute(proc(w!("ntdll.dll"), s!("RtlGetVersion")).ok_or("RtlGetVersion unavailable")?)
    };
    let mut v = Version {
        size: size_of::<Version>() as u32,
        major: 0,
        minor: 0,
        build: 0,
        platform: 0,
        service_pack: [0; 128],
    };
    if unsafe { f(&mut v) } < 0 {
        return Err("RtlGetVersion failed".into());
    }
    if v.major < 10 {
        return Err("Only Windows 10 classic taskbars are supported".into());
    }
    Ok(v.build)
}

#[repr(C)]
struct CompositionData {
    attribute: i32,
    data: *mut c_void,
    size: usize,
}
type CompositionFn = unsafe extern "system" fn(HWND, *mut CompositionData) -> BOOL;
#[derive(Clone, Copy)]
struct Composition {
    get: Option<CompositionFn>,
    set: Option<CompositionFn>,
}
impl Composition {
    fn new() -> Self {
        unsafe {
            Self {
                get: proc(w!("user32.dll"), s!("GetWindowCompositionAttribute"))
                    .map(|p| transmute::<unsafe extern "system" fn() -> isize, CompositionFn>(p)),
                set: proc(w!("user32.dll"), s!("SetWindowCompositionAttribute"))
                    .map(|p| transmute::<unsafe extern "system" fn() -> isize, CompositionFn>(p)),
            }
        }
    }
    fn call(f: CompositionFn, root: HWND, accent: &mut Accent) -> bool {
        let mut data = CompositionData {
            attribute: 19,
            data: (accent as *mut Accent).cast(),
            size: size_of::<Accent>(),
        };
        unsafe { f(root, &mut data).as_bool() }
    }
}

pub(crate) struct Windows {
    dpi: Dpi,
    build: u32,
    composition: Composition,
    // Kept alive until Controller::drop has restored everything.
    _com: ComApartment,
}
impl Windows {
    pub fn new() -> Result<Self, String> {
        let build = build_number()?;
        let dpi = Dpi::new()?;
        Ok(Self {
            dpi,
            build,
            composition: Composition::new(),
            _com: ComApartment::new()?,
        })
    }
}

fn class(window: HWND) -> Result<String, String> {
    let mut name = [0u16; 256];
    let n = unsafe { GetClassNameW(window, &mut name) };
    if n <= 0 {
        return Err(last_error("GetClassNameW"));
    }
    Ok(String::from_utf16_lossy(&name[..n as usize]))
}
fn parent(window: HWND) -> Result<HWND, String> {
    unsafe { GetParent(window) }.map_err(|e| error("GetParent", e))
}
fn child(window: HWND, name: &str) -> Result<HWND, String> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    unsafe { FindWindowExW(Some(window), None, PCWSTR(wide.as_ptr()), None) }
        .map_err(|_| format!("Missing {name} child (Explorer may be rebuilding its taskbar)"))
}
fn pid(window: HWND) -> Result<u32, String> {
    let mut pid = 0;
    if unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) } == 0 || pid == 0 {
        Err(last_error("GetWindowThreadProcessId"))
    } else {
        Ok(pid)
    }
}
struct Process(HANDLE);
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn process_started(pid: u32) -> Result<u64, String> {
    let p = Process(
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .map_err(|e| error("OpenProcess(Explorer identity)", e))?,
    );
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe { GetProcessTimes(p.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .map_err(|e| error("GetProcessTimes", e))?;
    let value = ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64;
    if value == 0 {
        return Err("Process creation identity is unavailable".into());
    }
    Ok(value)
}

struct Enumeration {
    windows: Vec<HWND>,
    exceeded: bool,
}
unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> BOOL {
    let out = &mut *(data.0 as *mut Enumeration);
    if out.windows.len() >= 16384 {
        out.exceeded = true;
        return BOOL(0);
    }
    out.windows.push(window);
    BOOL(1)
}
fn enumerate(root: Option<HWND>) -> Result<Vec<HWND>, String> {
    let mut out = Enumeration {
        windows: Vec::new(),
        exceeded: false,
    };
    let data = LPARAM((&mut out as *mut Enumeration) as isize);
    if let Some(root) = root {
        // EnumChildWindows' return value is explicitly unused by Win32. A false
        // value is also possible for an empty child list; don't invent an error.
        unsafe {
            let _ = EnumChildWindows(Some(root), Some(collect), data);
        }
    } else {
        unsafe { EnumWindows(Some(collect), data) }.map_err(|e| error("EnumWindows", e))?;
    }
    if out.exceeded {
        return Err("Window enumeration limit exceeded".into());
    }
    Ok(out.windows)
}

/// Exact classic hierarchy; no arbitrary first secondary FindWindow shortcut.
fn topology(root: HWND) -> Result<(HWND, HWND, Vec<HWND>, bool), String> {
    match class(root)?.as_str() {
        "Shell_TrayWnd" => {
            let rebar = child(root, "ReBarWindow32")?;
            let p = child(rebar, "MSTaskSwWClass")?;
            let list = child(p, "MSTaskListWClass")?;
            Ok((list, p, vec![p, rebar, root], true))
        }
        "Shell_SecondaryTrayWnd" => {
            let p = child(root, "WorkerW")?;
            let list = child(p, "MSTaskListWClass")?;
            Ok((list, p, vec![p, root], false))
        }
        _ => Err("Root handle is no longer a taskbar".into()),
    }
}
fn modern(root: HWND, build: u32) -> Result<bool, String> {
    if build < 22000 {
        return Ok(false);
    }
    for w in enumerate(Some(root))? {
        if matches!(
            class(w)?.as_str(),
            "Windows.UI.Composition.DesktopWindowContentBridge"
                | "Windows.UI.Input.InputSite.WindowClass"
        ) {
            return Ok(true);
        }
    }
    // Modern taskbars no longer expose the classic hierarchy on newer builds.
    Ok(topology(root).is_err())
}
fn identity(root: HWND, build: u32) -> Result<TaskbarId, String> {
    if !unsafe { IsWindow(Some(root)) }.as_bool() {
        return Err("Stale taskbar root handle".into());
    }
    if modern(root, build)? {
        return Err(
            "Native modern Windows 11 taskbar is unsupported; no mutation performed".into(),
        );
    }
    let (list, p, chain, _) = topology(root)?;
    let process_id = pid(root)?;
    for w in chain.iter().copied().chain(Some(list)) {
        if pid(w)? != process_id {
            return Err("Taskbar ancestry crosses process identities".into());
        }
    }
    let mut previous = list;
    for ancestor in chain {
        if parent(previous)? != ancestor {
            return Err("Taskbar ancestry changed".into());
        }
        previous = ancestor;
    }
    if unsafe { GetAncestor(list, GA_ROOT) } != root {
        return Err("Task-list root ancestry changed".into());
    }
    Ok(TaskbarId {
        root: root.0 as isize,
        task_list: list.0 as isize,
        parent: p.0 as isize,
        process_id,
        process_started: process_started(process_id)?,
    })
}
fn validate(id: &TaskbarId, build: u32) -> Result<(), String> {
    if identity(hwnd(id.root), build)? != *id {
        return Err(
            "Stale taskbar identity: handle, ancestry, PID or process creation time changed".into(),
        );
    }
    Ok(())
}

fn window_rect(window: HWND) -> Result<Rect, String> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(window, &mut r) }.map_err(|e| error("GetWindowRect", e))?;
    let r = Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
    .normalized();
    Ok(r)
}
fn rect(window: HWND) -> Result<Rect, String> {
    let r = window_rect(window)?;
    if !r.valid() {
        return Err("Window has an empty rectangle".into());
    }
    Ok(r)
}
fn client_rect(window: HWND) -> Result<Rect, String> {
    let mut r = RECT::default();
    unsafe { GetClientRect(window, &mut r) }.map_err(|e| error("GetClientRect", e))?;
    let (mut a, mut b) = (
        POINT {
            x: r.left,
            y: r.top,
        },
        POINT {
            x: r.right,
            y: r.bottom,
        },
    );
    if !unsafe { ClientToScreen(window, &mut a) }.as_bool()
        || !unsafe { ClientToScreen(window, &mut b) }.as_bool()
    {
        return Err(last_error("ClientToScreen"));
    }
    let r = Rect {
        left: a.x,
        top: a.y,
        right: b.x,
        bottom: b.y,
    }
    .normalized();
    if !r.valid() {
        return Err("Parent client rectangle is empty".into());
    }
    Ok(r)
}
fn position(id: &TaskbarId, list_rect: Rect) -> Result<Point, String> {
    // ScreenToClient handles negative monitor coordinates. Never use Abs, or
    // subtract the *window* rectangle (which can include non-client borders).
    let mut p = POINT {
        x: list_rect.left,
        y: list_rect.top,
    };
    if !unsafe { ScreenToClient(hwnd(id.parent), &mut p) }.as_bool() {
        return Err(last_error("ScreenToClient"));
    }
    Ok(Point { x: p.x, y: p.y })
}
fn vertical(r: Rect) -> bool {
    i64::from(r.bottom) - i64::from(r.top) > i64::from(r.right) - i64::from(r.left)
}

fn measure(
    id: &TaskbarId,
    dpi: Dpi,
    build: u32,
    warnings: &mut Vec<String>,
) -> Result<TaskbarSnapshot, String> {
    validate(id, build)?;
    let root = hwnd(id.root);
    if !unsafe { IsWindowVisible(root) }.as_bool()
        || !unsafe { IsWindowVisible(hwnd(id.task_list)) }.as_bool()
    {
        return Err("Taskbar is not currently visible".into());
    }
    let (list, _, chain, primary) = topology(root)?;
    let taskbar_rect = rect(root)?;
    if i64::from(taskbar_rect.right) - i64::from(taskbar_rect.left)
        == i64::from(taskbar_rect.bottom) - i64::from(taskbar_rect.top)
    {
        return Err("Taskbar orientation is ambiguous (square root rectangle)".into());
    }
    let parent_rect = client_rect(hwnd(id.parent))?;
    let list_rect = rect(list)?;
    let position = position(id, list_rect)?;
    let vertical = vertical(taskbar_rect);
    let mut clip = parent_rect;
    for ancestor in &chain {
        clip = clip
            .intersect(client_rect(*ancestor)?)
            .ok_or("Taskbar ancestors have no shared clipping area")?;
    }
    clip = clip
        .intersect(taskbar_rect)
        .ok_or("Taskbar clipping area is empty")?;
    let content_rect = match clip.intersect(list_rect) {
        Some(c) => match accessibility::content(list, c) {
            Ok(r) => Some(r),
            Err(e) => {
                warnings.push(format!("Taskbar {:#x}: {e}", id.root));
                None
            }
        },
        None => {
            warnings.push(format!(
                "Taskbar {:#x}: task-list is outside parent clip",
                id.root
            ));
            None
        }
    };
    // Reserve all visible non-task-list branches, not just a fixed list of
    // localized captions. This covers Start, TrayButton (search/task view),
    // TrayNotifyWnd/ClockButton, DynamicContent/news/widgets and custom rebar
    // toolbars. Ancestors are clipping containers, not reservations; descendants
    // of the task-list are the buttons being positioned, not other controls.
    let reservations = (|| -> Result<Vec<Rect>, String> {
        let mut regions = Vec::new();
        for w in enumerate(Some(root))? {
            if w == list
                || chain.contains(&w)
                || unsafe { IsChild(list, w) }.as_bool()
                || !unsafe { IsWindowVisible(w) }.as_bool()
            {
                continue;
            }
            // A successfully measured zero-area control reserves no space. An
            // API failure is different: abort the reservation measurement.
            if let Some(r) = window_rect(w)?.intersect(clip) {
                regions.push(r);
            }
        }
        Ok(regions)
    })();
    let available = match reservations {
        Ok(regions) => available(clip, &regions, content_rect, vertical),
        Err(e) => {
            warnings.push(format!(
                "Taskbar {:#x}: reservation measurement failed: {e}",
                id.root
            ));
            None
        }
    };
    let (available_rect, content_rect) = match available {
        Some(r) => (r, content_rect),
        None => {
            warnings.push(format!(
                "Taskbar {:#x}: no unambiguous free span; movement disabled",
                id.root
            ));
            (
                Rect {
                    right: clip.left,
                    bottom: clip.top,
                    ..clip
                },
                None,
            )
        }
    };
    let dpi = dpi.measure(root)?;
    // Fail rather than return a mixed snapshot if Explorer laid out/recreated a
    // window during COM traversal. Content changes may still race any OS call.
    validate(id, build)?;
    if rect(list)? != list_rect
        || client_rect(hwnd(id.parent))? != parent_rect
        || rect(root)? != taskbar_rect
    {
        return Err("Taskbar geometry changed during measurement; retry discovery".into());
    }
    Ok(TaskbarSnapshot {
        id: *id,
        primary,
        vertical,
        dpi,
        taskbar_rect,
        parent_rect,
        list_rect,
        available_rect,
        content_rect,
        position,
    })
}

impl Api for Windows {
    fn validate_root(&mut self, id: &TaskbarId) -> Result<(), String> {
        let root = hwnd(id.root);
        if !unsafe { IsWindow(Some(root)) }.as_bool()
            || !matches!(
                class(root)?.as_str(),
                "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            )
            || pid(root)? != id.process_id
            || process_started(id.process_id)? != id.process_started
        {
            return Err("Taskbar root identity changed".into());
        }
        Ok(())
    }
    fn root_obsolete(&mut self, id: &TaskbarId) -> bool {
        !unsafe { IsWindow(Some(hwnd(id.root))) }.as_bool()
            || pid(hwnd(id.root)).is_ok_and(|p| p != id.process_id)
            || process_started(id.process_id).is_ok_and(|t| t != id.process_started)
    }

    fn obsolete(&mut self, id: &TaskbarId) -> bool {
        if [id.root, id.task_list, id.parent]
            .iter()
            .any(|h| !unsafe { IsWindow(Some(hwnd(*h))) }.as_bool())
        {
            return true;
        }
        // Successful identity reads establish replacement. Failed reads alone do not.
        if let Ok(actual) = identity(hwnd(id.root), self.build) {
            return actual != *id;
        }
        if let Ok(actual_pid) = pid(hwnd(id.root)) {
            if actual_pid != id.process_id {
                return true;
            }
            if let Ok(created) = process_started(actual_pid) {
                return created != id.process_started;
            }
        }
        false
    }

    fn discover(&mut self) -> Result<Discovery, String> {
        let _dpi = self.dpi.enter()?;
        let mut d = Discovery {
            build_number: self.build,
            native_windows11: false,
            taskbars: Vec::new(),
            warnings: Vec::new(),
        };
        let mut roots = 0;
        for w in enumerate(None)? {
            // Unrelated top-level windows can disappear during EnumWindows.
            let Ok(c) = class(w) else {
                continue;
            };
            if c != "Shell_TrayWnd" && c != "Shell_SecondaryTrayWnd" {
                continue;
            }
            roots += 1;
            let result = (|| {
                if modern(w, self.build)? {
                    d.native_windows11 = true;
                    return Err(
                        "Native modern Windows 11 taskbar is unsupported; left untouched".into(),
                    );
                }
                let id = identity(w, self.build)?;
                measure(&id, self.dpi, self.build, &mut d.warnings)
            })();
            match result {
                Ok(s) => d.taskbars.push(s),
                Err(e) => d.warnings.push(format!("Taskbar {:#x}: {e}", w.0 as isize)),
            }
        }
        if roots == 0 {
            d.warnings.push(
                "No Explorer taskbars are present; retry discovery after shell startup/recreation"
                    .into(),
            );
        }
        d.taskbars.sort_by_key(|s| (!s.primary, s.id.root));
        Ok(d)
    }
    fn validate(&mut self, id: &TaskbarId) -> Result<(), String> {
        validate(id, self.build)
    }
    fn snapshot(&mut self, id: &TaskbarId) -> Result<TaskbarSnapshot, String> {
        let _dpi = self.dpi.enter()?;
        let mut warnings = Vec::new();
        let s = measure(id, self.dpi, self.build, &mut warnings)?;
        if s.content_rect.is_none() {
            return Err(warnings.join("; "));
        }
        Ok(s)
    }
    fn position(&mut self, id: &TaskbarId) -> Result<(Point, bool), String> {
        let _dpi = self.dpi.enter()?;
        validate(id, self.build)?;
        Ok((
            position(id, rect(hwnd(id.task_list))?)?,
            vertical(rect(hwnd(id.root))?),
        ))
    }
    fn move_to(&mut self, id: &TaskbarId, p: Point, expected: (Point, bool)) -> Result<(), String> {
        let _dpi = self.dpi.enter()?;
        validate(id, self.build)?;
        let current = position(id, rect(hwnd(id.task_list))?)?;
        let vertical = vertical(rect(hwnd(id.root))?);
        if (current, vertical) != expected {
            return Err(
                "Taskbar position/orientation changed after measurement; retry discovery".into(),
            );
        }
        // Re-read the cross-axis immediately before SetWindowPos, including on
        // restoration. No style words, visibility, size or unrelated HWNDs change.
        let p = crate::geometry::axis_position(current, p, vertical);
        validate(id, self.build)?;
        unsafe {
            SetWindowPos(
                hwnd(id.task_list),
                None,
                p.x,
                p.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSENDCHANGING,
            )
        }
        .map_err(|e| error("SetWindowPos", e))
    }
    fn query_accent(&mut self, id: &TaskbarId) -> Result<Option<Accent>, String> {
        validate(id, self.build)?;
        let Some(get) = self.composition.get else {
            return Ok(None);
        };
        // Sentinel prevents a purported successful no-write query being treated
        // as captured state. Preserve flags, tint and animation exactly if known.
        let mut a = Accent {
            state: -1,
            ..Accent::default()
        };
        if Composition::call(get, hwnd(id.root), &mut a) && (0..=6).contains(&a.state) {
            Ok(Some(a))
        } else {
            Ok(None)
        }
    }
    fn apply_accent(&mut self, id: &TaskbarId, mut a: Accent) -> Result<(), String> {
        if a.state == 4 && self.build < 17134 {
            return Err("Acrylic accent requires Windows 10 build 17134 or newer".into());
        }
        let set = self
            .composition
            .set
            .ok_or("SetWindowCompositionAttribute is unavailable")?;
        self.validate_root(id)?;
        if Composition::call(set, hwnd(id.root), &mut a) {
            Ok(())
        } else {
            Err("SetWindowCompositionAttribute rejected the accent policy".into())
        }
    }
    fn restore_accent(
        &mut self,
        id: &TaskbarId,
        a: Option<Accent>,
    ) -> Result<Option<String>, String> {
        if let Some(a) = a {
            self.apply_accent(id, a)?;
            return Ok(None);
        }
        self.apply_accent(id, Accent::default())?;
        self.validate_root(id)?;
        // No global broadcasts, registry changes, fabricated colorization values
        // or unbounded SendMessage. One bounded theme notification to this root.
        let result = unsafe {
            SendMessageTimeoutW(
                hwnd(id.root),
                WM_THEMECHANGED,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK | SMTO_ERRORONEXIT,
                200,
                None,
            )
        };
        if result.0 == 0 {
            return Err("Accent disabled, but bounded WM_THEMECHANGED restoration failed/timed out; exact third-party appearance was not queryable".into());
        }
        Ok(Some(format!("Taskbar {:#x}: restored Windows-managed appearance with bounded theme refresh; original third-party accent was unqueryable, so exact restoration is not guaranteed",id.root)))
    }
}
