//! Kernel objects coordinate this implementation only; no process enumeration,
//! process termination, Explorer restart, or startup installation is used.
use super::{
    protocol::{self, Kernel},
    Coordinator,
};
use crate::runtime::{warn, Control};
use std::{
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, HANDLE, HINSTANCE, HWND, LPARAM,
            LRESULT, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM,
        },
        Security::{GetLengthSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::{
            Console::{
                SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT,
                CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
            },
            LibraryLoader::GetModuleHandleW,
            RemoteDesktop::ProcessIdToSessionId,
            Threading::{
                CreateEventW, CreateMutexW, GetCurrentProcess, GetCurrentProcessId, OpenEventW,
                OpenMutexW, OpenProcessToken, ReleaseMutex, ResetEvent, SetEvent,
                WaitForSingleObject, EVENT_MODIFY_STATE, MUTEX_MODIFY_STATE,
                SYNCHRONIZATION_SYNCHRONIZE,
            },
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            MsgWaitForMultipleObjectsEx, PeekMessageW, RegisterClassW, TranslateMessage,
            UnregisterClassW, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, WM_CLOSE,
            WM_ENDSESSION, WM_QUERYENDSESSION, WM_QUIT, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
};

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
static CLEANUP_DONE: AtomicBool = AtomicBool::new(true);

fn error(context: &str, e: impl std::fmt::Display) -> String {
    format!("{context}: {e}")
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub(super) struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub(super) struct KernelMutex {
    handle: Handle,
    owned: bool,
}
impl Drop for KernelMutex {
    fn drop(&mut self) {
        if self.owned {
            if let Err(e) = unsafe { ReleaseMutex(self.handle.0) } {
                warn(&error("releasing runtime mutex", e));
            }
        }
    }
}

pub(super) struct NamedCoordinator {
    prefix: String,
    started: Instant,
}
impl NamedCoordinator {
    pub(super) fn new() -> Result<Self, String> {
        // TokenUser is stable across elevation and does not trust environment
        // variables. Local\ namespaces are session-local; the numeric session
        // ID is included as well to make the scope explicit.
        unsafe {
            let mut raw = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw)
                .map_err(|e| error("opening user token", e))?;
            let token = Handle(raw);
            let mut size = 0;
            let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut size);
            if size == 0 || size > 65536 {
                return Err("invalid user token information size".into());
            }
            // usize-aligned storage is required before interpreting TOKEN_USER.
            let mut data = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            GetTokenInformation(
                token.0,
                TokenUser,
                Some(data.as_mut_ptr().cast()),
                size,
                &mut size,
            )
            .map_err(|e| error("reading user token", e))?;
            let user = &*data.as_ptr().cast::<TOKEN_USER>();
            let length = GetLengthSid(user.User.Sid) as usize;
            if length == 0 || length > 68 {
                return Err("invalid user SID length".into());
            }
            let sid = std::slice::from_raw_parts(user.User.Sid.0.cast::<u8>(), length);
            let hex: String = sid.iter().map(|byte| format!("{byte:02x}")).collect();
            let mut session = 0;
            ProcessIdToSessionId(GetCurrentProcessId(), &mut session)
                .map_err(|e| error("reading session ID", e))?;
            Ok(Self {
                prefix: format!("Local\\TaskbarX.Rust.v1.{hex}.{session}"),
                started: Instant::now(),
            })
        }
    }

    fn name(&self, suffix: &str) -> Vec<u16> {
        wide(&format!("{}.{}", self.prefix, suffix))
    }
}

impl Kernel for NamedCoordinator {
    type Mutex = KernelMutex;
    type Event = Handle;
    type Completion = Handle;
    fn create_completion(&self) -> Result<Handle, String> {
        let name = self.name("restored");
        let handle = unsafe { CreateEventW(None, true, false, PCWSTR(name.as_ptr())) }
            .map(Handle)
            .map_err(|e| error("creating restoration result", e))?;
        self.reset_event(&handle)?;
        Ok(handle)
    }
    fn open_completion(&self) -> Result<Handle, String> {
        let name = self.name("restored");
        unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())) }
            .map(Handle)
            .map_err(|e| error("opening restoration result", e))
    }
    fn completion_ok(&self, event: &Handle) -> Result<bool, String> {
        match unsafe { WaitForSingleObject(event.0, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err("cannot read restoration result".into()),
        }
    }
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
    fn create_mutex(&self, key: &str) -> Result<KernelMutex, String> {
        let name = self.name(key);
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map(Handle)
            .map_err(|e| error("creating instance mutex", e))?;
        Ok(KernelMutex {
            handle,
            owned: false,
        })
    }
    fn open_mutex(&self, key: &str) -> Result<Option<KernelMutex>, String> {
        let name = self.name(key);
        match unsafe {
            OpenMutexW(
                SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE,
                false,
                PCWSTR(name.as_ptr()),
            )
        } {
            Ok(handle) => Ok(Some(KernelMutex {
                handle: Handle(handle),
                owned: false,
            })),
            Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() => Ok(None),
            Err(e) => Err(error("opening existing runtime mutex", e)),
        }
    }
    fn acquire(&self, mutex: &mut KernelMutex, timeout: u32) -> Result<bool, String> {
        match unsafe { WaitForSingleObject(mutex.handle.0, timeout) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => {
                mutex.owned = true;
                Ok(true)
            }
            WAIT_TIMEOUT => Ok(false),
            _ => Err(error(
                "waiting for instance mutex",
                unsafe { GetLastError() }.to_hresult(),
            )),
        }
    }
    fn create_event(&self) -> Result<Handle, String> {
        let name = self.name("stop");
        unsafe { CreateEventW(None, true, false, PCWSTR(name.as_ptr())) }
            .map(Handle)
            .map_err(|e| error("creating stop event", e))
    }
    fn open_event(&self) -> Result<Handle, String> {
        let name = self.name("stop");
        unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) }
            .map(Handle)
            .map_err(|e| error("opening existing stop event", e))
    }
    fn reset_event(&self, event: &Handle) -> Result<(), String> {
        unsafe { ResetEvent(event.0) }.map_err(|e| error("resetting stale stop event", e))
    }
    fn signal_event(&self, event: &Handle) -> Result<(), String> {
        unsafe { SetEvent(event.0) }.map_err(|e| error("signaling runtime stop", e))
    }
}

impl Coordinator for NamedCoordinator {
    type Lease = Instance;
    fn claim(&mut self) -> Result<Option<Instance>, String> {
        let Some(claim) = protocol::claim(self)? else {
            return Ok(None);
        };
        let notifications = Notifications::new()?;
        Ok(Some(Instance {
            stop: claim.stop,
            completion: claim.completion,
            _notifications: notifications,
            _owned: claim.running,
            started: Instant::now(),
            _owner_thread: PhantomData,
        }))
    }

    fn stop_existing(&mut self, timeout_ms: u32) -> Result<(), String> {
        protocol::stop(self, timeout_ms)
    }
}

pub(super) struct Instance {
    // Rust drops fields in declaration order. Keep the stop event alive until
    // after mutex release, including while joining the notification helper.
    _notifications: Notifications,
    _owned: KernelMutex,
    stop: Handle,
    completion: Handle,
    started: Instant,
    // Prevent accidentally transferring mutex ownership/notification window.
    _owner_thread: PhantomData<Rc<()>>,
}
impl Control for Instance {
    fn complete(&mut self, success: bool) -> Result<(), String> {
        if success {
            unsafe { SetEvent(self.completion.0) }
                .map_err(|e| error("publishing restoration result", e))?;
        }
        Ok(())
    }
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
    fn stopped(&mut self) -> Result<bool, String> {
        pump_messages(true);
        if STOP_REQUESTED.load(Ordering::Acquire) {
            return Ok(true);
        }
        match unsafe { WaitForSingleObject(self.stop.0, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(error(
                "checking stop event",
                unsafe { GetLastError() }.to_hresult(),
            )),
        }
    }
    fn wait(&mut self, milliseconds: u64) -> Result<(), String> {
        if self.stopped()? {
            return Ok(());
        }
        // A console callback only sets an atomic flag. The 50 ms upper bound
        // makes that path responsive without sharing closable handles with it.
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(
                Some(&[self.stop.0]),
                milliseconds.min(50) as u32,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        if result == WAIT_FAILED {
            return Err(error(
                "waiting for stop/messages",
                unsafe { GetLastError() }.to_hresult(),
            ));
        }
        pump_messages(true);
        Ok(())
    }
}

struct HiddenWindow {
    window: HWND,
    module: HINSTANCE,
}
impl HiddenWindow {
    fn new() -> Result<Self, String> {
        unsafe {
            let module = HINSTANCE(
                GetModuleHandleW(None)
                    .map_err(|e| error("reading module handle", e))?
                    .0,
            );
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: w!("TaskbarX.Rust.SessionNotifications.v1"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(error(
                    "registering shutdown window",
                    GetLastError().to_hresult(),
                ));
            }
            // A hidden TOP-LEVEL window, not HWND_MESSAGE: broadcasts must arrive.
            let window = match CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.lpszClassName,
                w!("TaskbarX runtime"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(module),
                None,
            ) {
                Ok(window) => window,
                Err(e) => {
                    let _ = UnregisterClassW(class.lpszClassName, Some(module));
                    return Err(error("creating shutdown window", e));
                }
            };
            Ok(Self { window, module })
        }
    }
}
impl Drop for HiddenWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.window);
            let _ = UnregisterClassW(
                w!("TaskbarX.Rust.SessionNotifications.v1"),
                Some(self.module),
            );
        }
    }
}

fn pump_messages(owner: bool) {
    unsafe {
        let mut msg = MSG::default();
        for _ in 0..64 {
            if (owner && STOP_REQUESTED.load(Ordering::Acquire))
                || !PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool()
            {
                break;
            }
            if msg.message == WM_QUIT {
                STOP_REQUESTED.store(true, Ordering::Release);
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

struct Notifications {
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Notifications {
    fn new() -> Result<Self, String> {
        STOP_REQUESTED.store(false, Ordering::Release);
        CLEANUP_DONE.store(false, Ordering::Release);
        unsafe { SetConsoleCtrlHandler(Some(console_handler), true) }
            .map_err(|e| error("registering console shutdown handler", e))?;
        let mut guard = Self { thread: None };
        let (ready, receive) = std::sync::mpsc::sync_channel(1);
        // One fixed notification helper, never an animation thread. It does NOT
        // own Session, COM, or any taskbar handle. WM_ENDSESSION may terminate a
        // process as soon as its callback returns, so the helper acknowledges
        // only after owner-thread restoration (or the OS's bounded grace period).
        // Keeping this callback off the owner thread avoids a cleanup deadlock.
        guard.thread = Some(
            std::thread::Builder::new()
                .name("taskbarx-shutdown".into())
                .spawn(move || {
                    let window = match HiddenWindow::new() {
                        Ok(window) => window,
                        Err(error) => {
                            let _ = ready.send(Err(error));
                            return;
                        }
                    };
                    if ready.send(Ok(())).is_err() {
                        return;
                    }
                    while !CLEANUP_DONE.load(Ordering::Acquire) {
                        let result = unsafe {
                            MsgWaitForMultipleObjectsEx(None, 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
                        };
                        if result == WAIT_FAILED {
                            warn(&error(
                                "waiting for session notifications",
                                unsafe { GetLastError() }.to_hresult(),
                            ));
                            STOP_REQUESTED.store(true, Ordering::Release);
                            break;
                        }
                        pump_messages(false);
                    }
                    drop(window);
                })
                .map_err(|e| error("starting shutdown notification thread", e))?,
        );
        receive
            .recv()
            .map_err(|e| error("initializing shutdown notifications", e))??;
        Ok(guard)
    }
}
impl Drop for Notifications {
    fn drop(&mut self) {
        // Instance outlives the restoration guard AND Session's Drop.
        CLEANUP_DONE.store(true, Ordering::Release);
        unsafe {
            let _ = SetConsoleCtrlHandler(Some(console_handler), false);
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                warn("shutdown notification thread panicked");
            }
        }
    }
}

fn await_cleanup() {
    let started = Instant::now();
    while !CLEANUP_DONE.load(Ordering::Acquire) && started.elapsed().as_millis() < 4000 {
        unsafe {
            windows::Win32::System::Threading::Sleep(10);
        }
    }
}

unsafe extern "system" fn console_handler(kind: u32) -> windows::core::BOOL {
    match kind {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {
            STOP_REQUESTED.store(true, Ordering::Release);
            true.into()
        }
        CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT | CTRL_SHUTDOWN_EVENT => {
            STOP_REQUESTED.store(true, Ordering::Release);
            // Returning immediately from CLOSE allows Windows to terminate the
            // process before owner-thread cleanup. Only wait for its acknowledgement;
            // no logging, COM, allocation, locks, or taskbar APIs in this callback.
            await_cleanup();
            true.into()
        }
        _ => false.into(),
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_QUERYENDSESSION => {
            STOP_REQUESTED.store(true, Ordering::Release);
            LRESULT(1)
        }
        WM_ENDSESSION if wparam.0 != 0 => {
            STOP_REQUESTED.store(true, Ordering::Release);
            await_cleanup();
            LRESULT(0)
        }
        WM_CLOSE => {
            STOP_REQUESTED.store(true, Ordering::Release);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
