//! Native adapter. No callback executes application actions or posts WM_QUIT.
use super::protocol::{
    terminal_cleanup, Backend, Callback, Inbox, MenuAnchor, RESTART, SETTINGS, STOP,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{
            GetLastError, SetLastError, ERROR_SUCCESS, HINSTANCE, HWND, LPARAM, LRESULT, POINT,
            WPARAM,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD,
                NIM_DELETE, NIM_MODIFY, NIM_SETFOCUS, NIM_SETVERSION, NINF_KEY, NIN_SELECT,
                NOTIFYICONDATAW, NOTIFYICON_VERSION_4, NOTIFY_ICON_MESSAGE,
            },
            WindowsAndMessaging::{
                AppendMenuW, CopyIcon, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                DestroyIcon, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos,
                GetWindowLongPtrW, LoadIconW, PeekMessageW, PostMessageW, RegisterClassW,
                RegisterWindowMessageW, SendMessageW, SetForegroundWindow, SetMenuDefaultItem,
                SetMenuInfo, SetWindowLongPtrW, TrackPopupMenuEx, TranslateMessage,
                UnregisterClassW, CREATESTRUCTW, GWLP_USERDATA, HICON, HMENU, IDI_APPLICATION,
                MENUINFO, MF_STRING, MIM_STYLE, MNS_MODELESS, MSG, PM_NOREMOVE, PM_REMOVE,
                TPM_RIGHTALIGN, TPM_RIGHTBUTTON, WM_APP, WM_CANCELMODE, WM_CLOSE, WM_COMMAND,
                WM_CONTEXTMENU, WM_EXITMENULOOP, WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_QUIT,
                WM_UNINITMENUPOPUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        },
    },
};

const ICON_ID: u32 = 1; // Fits the 16-bit ID in version-four callback LPARAM.
const CALLBACK_MESSAGE: u32 = WM_APP + 27;
// shellapi.h's expression macro is not emitted by windows 0.61 metadata.
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
static NEXT_CLASS: AtomicU64 = AtomicU64::new(1);

fn error(context: &str, cause: impl std::fmt::Display) -> String {
    format!("{context}: {cause}")
}

struct CallbackState {
    inbox: Rc<RefCell<Inbox>>,
    taskbar_created: u32,
    window_alive: Cell<bool>,
    menu_active: Cell<bool>,
}

pub(super) struct Native {
    module: HINSTANCE,
    class: Vec<u16>,
    class_registered: bool,
    notified: bool,
    window: Option<HWND>,
    icon: Option<HICON>,
    menu: Option<HMENU>,
    // Box keeps the GWLP_USERDATA pointer stable when Native/TrayIcon moves.
    callback: Option<Box<CallbackState>>,
    started: Instant,
}

impl Native {
    pub(super) fn new(inbox: Rc<RefCell<Inbox>>) -> Result<Self, String> {
        if cfg!(test) {
            return Err("native tray creation is disabled in unit tests".into());
        }
        let module = HINSTANCE(
            unsafe { GetModuleHandleW(None) }
                .map_err(|e| error("reading tray module", e))?
                .0,
        );
        let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        if taskbar_created == 0 {
            return Err(error(
                "registering TaskbarCreated",
                unsafe { GetLastError() }.to_hresult(),
            ));
        }
        let class = format!(
            "TaskbarX.Rust.Tray.v1.{}.{}",
            std::process::id(),
            NEXT_CLASS.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        Ok(Self {
            module,
            class,
            class_registered: false,
            notified: false,
            window: None,
            icon: None,
            menu: None,
            started: Instant::now(),
            callback: Some(Box::new(CallbackState {
                inbox,
                taskbar_created,
                window_alive: Cell::new(false),
                menu_active: Cell::new(false),
            })),
        })
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window.unwrap_or_default(),
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK_MESSAGE,
            hIcon: self.icon.unwrap_or_default(),
            ..Default::default()
        };
        let tip: Vec<u16> = "TaskbarX - Settings, Restart, Stop"
            .encode_utf16()
            .collect();
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        data
    }

    fn notify(&self, message: NOTIFY_ICON_MESSAGE, context: &str) -> Result<(), String> {
        // Shell_NotifyIcon does not promise meaningful GetLastError details.
        if unsafe { Shell_NotifyIconW(message, &self.data()) }.as_bool() {
            Ok(())
        } else {
            Err(format!("{context}: Shell_NotifyIconW returned false"))
        }
    }
}

impl Backend for Native {
    fn register_class(&mut self) -> Result<(), String> {
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: self.module,
            lpszClassName: PCWSTR(self.class.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            Err(error(
                "registering tray window class",
                unsafe { GetLastError() }.to_hresult(),
            ))
        } else {
            self.class_registered = true;
            Ok(())
        }
    }

    fn create_window(&mut self) -> Result<(), String> {
        let state = self
            .callback
            .as_deref()
            .ok_or("missing tray callback state")?;
        // A hidden TOP-LEVEL window is required for TaskbarCreated broadcasts.
        // HWND_MESSAGE would not receive Explorer's recovery notification.
        self.window = Some(
            unsafe {
                CreateWindowExW(
                    WS_EX_TOOLWINDOW,
                    PCWSTR(self.class.as_ptr()),
                    w!("TaskbarX tray"),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(self.module),
                    Some((state as *const CallbackState).cast()),
                )
            }
            .map_err(|e| error("creating tray window", e))?,
        );
        Ok(())
    }

    fn copy_icon(&mut self) -> Result<(), String> {
        // IDI_APPLICATION is a Windows-provided shared icon, requiring no
        // external asset/license. Never destroy the shared handle; own a copy.
        let shared = unsafe { LoadIconW(None, IDI_APPLICATION) }
            .map_err(|e| error("loading system tray icon", e))?;
        self.icon = Some(unsafe { CopyIcon(shared) }.map_err(|e| error("copying tray icon", e))?);
        Ok(())
    }

    fn create_menu(&mut self) -> Result<(), String> {
        self.menu = Some(unsafe { CreatePopupMenu() }.map_err(|e| error("creating tray menu", e))?);
        Ok(())
    }

    fn configure_menu(&mut self) -> Result<(), String> {
        let menu = self.menu.ok_or("missing owned tray menu")?;
        unsafe {
            AppendMenuW(menu, MF_STRING, usize::from(SETTINGS), w!("&Settings"))
                .map_err(|e| error("adding Settings menu item", e))?;
            AppendMenuW(menu, MF_STRING, usize::from(RESTART), w!("&Restart"))
                .map_err(|e| error("adding Restart menu item", e))?;
            AppendMenuW(menu, MF_STRING, usize::from(STOP), w!("S&top"))
                .map_err(|e| error("adding Stop menu item", e))?;
            SetMenuDefaultItem(menu, u32::from(SETTINGS), 0)
                .map_err(|e| error("setting default tray menu item", e))?;
            // No nested modal menu loop: polling and graceful shutdown remain
            // bounded even if the user leaves the context menu open indefinitely.
            SetMenuInfo(
                menu,
                &MENUINFO {
                    cbSize: std::mem::size_of::<MENUINFO>() as u32,
                    fMask: MIM_STYLE,
                    dwStyle: MNS_MODELESS,
                    ..Default::default()
                },
            )
            .map_err(|e| error("making tray menu modeless", e))?;
        }
        Ok(())
    }

    fn add_icon(&mut self) -> Result<(), String> {
        self.notify(NIM_ADD, "adding tray icon")?;
        self.notified = true;
        Ok(())
    }
    fn modify_icon(&mut self) -> Result<(), String> {
        self.notify(NIM_MODIFY, "refreshing tray icon")
    }
    fn set_version(&mut self) -> Result<(), String> {
        self.notify(NIM_SETVERSION, "setting tray callback version")
    }

    fn show_menu(&mut self, mut anchor: MenuAnchor) -> Result<(), String> {
        let hwnd = self.window.ok_or("missing owned tray window")?;
        let menu = self.menu.ok_or("missing owned tray menu")?;
        let callback = self
            .callback
            .as_deref()
            .ok_or("missing tray callback state")?;
        if callback.menu_active.get() {
            return Ok(());
        }
        if anchor.x == -1 && anchor.y == -1 {
            let mut position = POINT::default();
            unsafe { GetCursorPos(&mut position) }
                .map_err(|e| error("locating tray context menu", e))?;
            anchor = MenuAnchor {
                x: position.x,
                y: position.y,
            };
        }
        unsafe {
            // Required for normal dismissal when clicking outside the menu.
            let _ = SetForegroundWindow(hwnd);
            callback.menu_active.set(true);
            if !TrackPopupMenuEx(
                menu,
                (TPM_RIGHTBUTTON | TPM_RIGHTALIGN).0,
                anchor.x,
                anchor.y,
                hwnd,
                None,
            )
            .as_bool()
            {
                callback.menu_active.set(false);
                return Err(error("showing tray menu", GetLastError().to_hresult()));
            }
            // Standard notification-menu focus workaround; this is sent only
            // to OUR window, never to Explorer or the thread/global queue.
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }

    fn pump_messages(&mut self, budget: usize) {
        let Some(hwnd) = self.window else {
            return;
        };
        unsafe {
            let mut message = MSG::default();
            for _ in 0..budget {
                // Inspect before removing: leave a pending thread-level quit
                // for the embedding lifecycle loop rather than swallowing it.
                if !PeekMessageW(&mut message, Some(hwnd), 0, 0, PM_NOREMOVE).as_bool()
                    || message.message == WM_QUIT
                    || message.hwnd != hwnd
                {
                    break;
                }
                if !PeekMessageW(&mut message, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
    fn return_focus(&mut self) -> Result<(), String> {
        self.notify(
            NIM_SETFOCUS,
            "returning keyboard focus to notification area",
        )
    }
    fn delete_icon(&mut self) -> Result<(), String> {
        if self.notified {
            // Never address a handle that the HWND callback says was already
            // destroyed (and therefore could have been reused by Windows).
            if self
                .callback
                .as_deref()
                .is_some_and(|state| state.window_alive.get())
            {
                self.notify(NIM_DELETE, "removing tray icon")?;
            }
            self.notified = false;
        }
        Ok(())
    }

    fn destroy_menu(&mut self) -> Result<(), String> {
        if let Some(menu) = self.menu {
            if let (Some(hwnd), Some(callback)) = (self.window, self.callback.as_deref()) {
                if callback.menu_active.get() {
                    // Cancel only the owned window's menu; do not use global
                    // EndMenu against an embedding application's active menu.
                    unsafe {
                        SendMessageW(hwnd, WM_CANCELMODE, None, None);
                    }
                }
            }
            unsafe { DestroyMenu(menu) }.map_err(|e| error("destroying tray menu", e))?;
            self.menu = None;
        }
        Ok(())
    }

    fn destroy_window(&mut self) -> Result<(), String> {
        if let Some(hwnd) = self.window {
            if self
                .callback
                .as_deref()
                .is_some_and(|state| state.window_alive.get())
            {
                unsafe { DestroyWindow(hwnd) }.map_err(|e| error("destroying tray window", e))?;
            }
            self.window = None;
        }
        Ok(())
    }

    fn destroy_icon(&mut self) -> Result<(), String> {
        if let Some(icon) = self.icon {
            unsafe { DestroyIcon(icon) }.map_err(|e| error("destroying owned tray icon", e))?;
            self.icon = None;
        }
        Ok(())
    }

    fn unregister_class(&mut self) -> Result<(), String> {
        if self.class_registered {
            unsafe { UnregisterClassW(PCWSTR(self.class.as_ptr()), Some(self.module)) }
                .map_err(|e| error("unregistering tray window class", e))?;
            self.class_registered = false;
        }
        Ok(())
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        // Retryable close may intentionally retain the HWND until deletion of
        // its notification succeeds. At final Drop, release all LOCAL resources
        // independently: Explorer may have exited and NIM_DELETE may stay false.
        terminal_cleanup(self);
        // If the OS persistently refuses DestroyWindow, detach the callback
        // pointer before freeing its allocation. This only modifies our owned
        // live HWND, never a handle that has already been destroyed/reused.
        if self
            .callback
            .as_deref()
            .is_some_and(|state| state.window_alive.get())
        {
            let mut detached = false;
            if let Some(hwnd) = self.window {
                for _ in 0..3 {
                    unsafe {
                        SetLastError(ERROR_SUCCESS);
                        let old = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                        if old != 0 || GetLastError() == ERROR_SUCCESS {
                            detached = true;
                            break;
                        }
                    }
                }
            }
            if !detached {
                // Unrecoverable Win32 failure: both destruction and detachment
                // failed. Keep only the callback allocation alive rather than
                // permit use-after-free. The OS reclaims it at process exit.
                if let Some(callback) = self.callback.take() {
                    let _ = Box::leak(callback);
                }
            }
        }
    }
}

fn decode_notification(wparam: usize, lparam: isize) -> Option<Callback> {
    let bits = lparam as u32;
    if bits >> 16 != ICON_ID {
        return None;
    }
    match bits & 0xffff {
        NIN_SELECT | NIN_KEYSELECT => Some(Callback::Activate),
        WM_CONTEXTMENU => Some(Callback::Menu(MenuAnchor {
            x: wparam as u16 as i16 as i32,
            y: (wparam >> 16) as u16 as i16 as i32,
        })),
        // Version four supplies semantic activation events. Do not additionally
        // handle raw mouse-up/double-click messages and produce duplicate actions.
        _ => None,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let creation = &*(lparam.0 as *const CREATESTRUCTW);
        let state = creation.lpCreateParams as *const CallbackState;
        if state.is_null() {
            return LRESULT(0);
        }
        SetLastError(ERROR_SUCCESS);
        let previous = SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
        if previous == 0 && GetLastError() != ERROR_SUCCESS {
            return LRESULT(0);
        }
        (*state).window_alive.set(true);
        return LRESULT(1);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const CallbackState;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let state = &*pointer;
    if message == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        state.window_alive.set(false);
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    if message == WM_EXITMENULOOP || message == WM_UNINITMENUPOPUP {
        state.menu_active.set(false);
    }
    let callback = if message == WM_EXITMENULOOP || message == WM_UNINITMENUPOPUP {
        Some(Callback::MenuClosed)
    } else if message == state.taskbar_created {
        Some(Callback::TaskbarCreated)
    } else if message == CALLBACK_MESSAGE {
        decode_notification(wparam.0, lparam.0)
    } else if message == WM_COMMAND && lparam.0 == 0 && wparam.0 >> 16 == 0 {
        Some(Callback::Command(wparam.0 as u16))
    } else if message == WM_CLOSE {
        Some(Callback::Command(STOP))
    } else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    if let Some(callback) = callback {
        // All native calls occur outside Inbox borrows; try_borrow_mut also
        // prevents any unexpected reentrant borrow from unwinding across FFI.
        if let Ok(mut inbox) = state.inbox.try_borrow_mut() {
            inbox.receive(callback);
        }
    }
    LRESULT(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONUP, WM_RBUTTONUP};

    #[test]
    fn semantic_mouse_and_keyboard_activation_only() {
        for event in [NIN_SELECT, NIN_KEYSELECT] {
            assert!(matches!(
                decode_notification(0, ((ICON_ID << 16) | event) as isize),
                Some(Callback::Activate)
            ));
        }
        for event in [WM_LBUTTONUP, WM_RBUTTONUP] {
            assert!(decode_notification(0, ((ICON_ID << 16) | event) as isize).is_none());
        }
        assert!(decode_notification(0, ((2 << 16) | NIN_SELECT) as isize).is_none());
    }

    #[test]
    fn context_coordinates_preserve_signed_multi_monitor_positions() {
        let packed = u32::from((-300i16) as u16) | (u32::from((-20i16) as u16) << 16);
        match decode_notification(packed as usize, ((ICON_ID << 16) | WM_CONTEXTMENU) as isize) {
            Some(Callback::Menu(anchor)) => assert_eq!(anchor, MenuAnchor { x: -300, y: -20 }),
            _ => panic!("context menu not decoded"),
        }
    }
}
