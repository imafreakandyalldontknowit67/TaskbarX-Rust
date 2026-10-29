//! A conventional Win32 control tree. The window borrows a stable Box<RefCell<Ui>>
//! owned by run(); WM_NCDESTROY clears that pointer, never frees it. Reentrant
//! control notifications use try_borrow_mut, not aliasing &mut references.
//! Worker threads never hold an HWND or a pointer into window state.
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};
use taskbarx_configurator::{
    controller::{startup_report, Action, Controller, Outcome},
    process::{Process, SystemProcess},
    protocol::Options,
    settings::{
        hex_rgba, rgba_hex, AnimationPreview, Form, Settings, ANIMATIONS, MEASUREMENT_BACKENDS,
        STYLES,
    },
    UPSTREAM_LICENSE,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{Dialogs::*, *},
            HiDpi::*,
            Input::KeyboardAndMouse::{
                EnableWindow, GetCapture, GetFocus, ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE,
            },
            WindowsAndMessaging::*,
        },
    },
};

const CENTER: u16 = 101;
const PRIMARY: u16 = 102;
const SECONDARY: u16 = 103;
const BETWEEN: u16 = 104;
const PRIMARY_OFFSET: u16 = 105;
const SECONDARY_OFFSET: u16 = 106;
const ANIMATION: u16 = 107;
const DURATION: u16 = 108;
const POLL: u16 = 109;
const STYLE: u16 = 110;
const RED: u16 = 111;
const GREEN: u16 = 112;
const BLUE: u16 = 113;
const ALPHA: u16 = 114;
const SWATCH: u16 = 115;
const PRIMARY_OPACITY: u16 = 116;
const SECONDARY_OPACITY: u16 = 117;
const RADIUS: u16 = 118;
const SEGMENTS: u16 = 119;
const MAXIMIZED: u16 = 120;
const BATTERY_ANIMATION: u16 = 121;
const BATTERY_POLL: u16 = 122;
const SKIP1: u16 = 123;
const SKIP2: u16 = 124;
const SKIP3: u16 = 125;
const FULLSCREEN: u16 = 126;
const HIDE_PRIMARY_START: u16 = 127;
const HIDE_SECONDARY_START: u16 = 128;
const HIDE_PRIMARY_TRAY: u16 = 129;
const HIDE_SECONDARY_TRAY: u16 = 130;
const OVERLAP: u16 = 131;
const TOOLBARS: u16 = 132;
const TRAY_ICON: u16 = 133;
const DELAY: u16 = 134;
const MEASUREMENT: u16 = 135;
const CONSOLE: u16 = 136;
const COLOR_HEX: u16 = 137;
const SAVE: u16 = 201;
const APPLY: u16 = 202;
const STOP: u16 = 203;
const RELOAD: u16 = 204;
const RESET: u16 = 205;
const DIAGNOSE: u16 = 206;
const ABOUT: u16 = 207;
const CLOSE: u16 = 208;
const RESTART: u16 = 209;
const STARTUP_REFRESH: u16 = 210;
const STARTUP_ENABLE: u16 = 211;
const STARTUP_DISABLE: u16 = 212;
const SHORTCUT_GENERATE: u16 = 213;
const SHORTCUT_CREATE: u16 = 214;
const SHORTCUT_REMOVE: u16 = 215;
const OPEN_START: u16 = 216;
const REMOVE_STARTUP: u16 = 217;
const REMOVE_STOP: u16 = 218;
const COLOR_DIALOG: u16 = 219;
const EYEDROPPER: u16 = 220;
const ANIMATION_PREV: u16 = 221;
const ANIMATION_NEXT: u16 = 222;
const BATTERY_PREV: u16 = 223;
const BATTERY_NEXT: u16 = 224;
const PREVIEW_BATTERY: u16 = 225;
const PREVIEW_CANCEL: u16 = 226;
const OUTPUT: u16 = 301;
const DIRTY: u16 = 303;
const STARTUP_STATUS: u16 = 304;
const SHORTCUT_PATH: u16 = 305;
const SHORTCUT_ICON: u16 = 306;
const SHORTCUT_COMMAND: u16 = 307;
const CONTENT_W: i32 = 850;
const CONTENT_H: i32 = 600;
const NAV: u16 = 500;
const TITLE: u16 = 490;
const STATUS: u16 = 302;
const RADIO: u16 = 600;
const SLIDER: u16 = 700;
const TBM_GETPOS: u32 = WM_USER; // commctrl.h: trackbar position query
const PREVIEW: u16 = 800;
const PREVIEW_TRACK: u16 = 801;
const PAGES: [&str; 9] = [
    "Style",
    "Animation",
    "Position",
    "Startup",
    "Extra",
    "Mechanics",
    "Start Button",
    "About",
    "Uninstall",
];
// Segoe MDL2 Assets glyphs, escaped to survive shell/code-page conversions.
const ICONS: [&str; 9] = [
    "\u{E790}", "\u{E768}", "\u{E8AB}", "\u{E7E8}", "\u{E713}", "\u{E9D9}", "\u{E80F}", "\u{E946}",
    "\u{E74D}",
];
// WinUser.h static-control styles (windows metadata otherwise places these in
// the unrelated SystemServices feature).
const SS_LEFT: u32 = 0;
const SS_OWNERDRAW: u32 = 13;
#[cfg(test)]
thread_local! {
    static LAYOUT_REPAINT_FLAGS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn wide(s: &str) -> Vec<u16> {
    s.replace('\0', " ").encode_utf16().chain(Some(0)).collect()
}
pub fn message(text: &str, error: bool) {
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(wide(text).as_ptr()),
            w!("TaskbarX Configurator"),
            MB_OK
                | if error {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                },
        );
    }
}
fn winerr(e: windows::core::Error) -> String {
    e.to_string()
}
fn px(value: i32, dpi: u32) -> i32 {
    (value * dpi as i32 + 48) / 96
}

struct Item {
    page: i32,
    id: u16,
    hwnd: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}
enum Event {
    Done(Box<Completion>),
    Runtime(String),
}
struct Completion {
    result: Result<Outcome, String>,
    saved: Option<Settings>,
    startup: Option<taskbarx_shell::StartupStatus>,
}
struct PreviewPlayback {
    started: Instant,
    animation: AnimationPreview,
}
struct PickSession {
    original: [u8; 4],
    hex: String,
}
struct Ui {
    hwnd: HWND,
    page: i32,
    building_page: i32,
    preview: Option<PreviewPlayback>,
    picker: Option<PickSession>,
    color_dialog_requested: bool,
    custom_colors: [COLORREF; 16],
    saved: Option<Settings>,
    schema_version: u32,
    items: Vec<Item>,
    dpi: u32,
    font: HFONT,
    scroll_x: i32,
    scroll_y: i32,
    busy: bool,
    has_form: bool,
    populating: bool,
    alert: Option<String>,
    tx: Sender<Action>,
    rx: Receiver<Event>,
}

impl Drop for Ui {
    fn drop(&mut self) {
        // All child windows have been destroyed before the owning Box is dropped.
        unsafe {
            if !self.font.is_invalid() {
                let _ = DeleteObject(self.font.into());
            }
        }
        // Dropping the sender lets the worker exit. Child::drop closes process
        // handles, it does not kill the independently running engine.
    }
}

impl Ui {
    fn control(&self, id: u16) -> HWND {
        self.items
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.hwnd)
            .unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn add(
        &mut self,
        id: u16,
        class: PCWSTR,
        text: &str,
        style: WINDOW_STYLE,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<HWND, String> {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(wide(text).as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            px(x, self.dpi),
            px(y, self.dpi),
            px(width, self.dpi),
            px(height, self.dpi),
            Some(self.hwnd),
            Some(HMENU(id as usize as *mut _)),
            None,
            None,
        )
        .map_err(winerr)?;
        self.items.push(Item {
            page: self.building_page,
            id,
            hwnd,
            x,
            y,
            w: width,
            h: height,
        });
        SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(self.font.0 as usize)),
            Some(LPARAM(0)),
        );
        Ok(hwnd)
    }
    unsafe fn label(
        &mut self,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<(), String> {
        self.add(
            0,
            w!("STATIC"),
            text,
            WINDOW_STYLE(SS_LEFT),
            x,
            y,
            width,
            height,
        )?;
        Ok(())
    }
    unsafe fn edit(
        &mut self,
        id: u16,
        label: &str,
        x: i32,
        y: i32,
        width: i32,
    ) -> Result<(), String> {
        self.label(label, x, y, width, 22)?;
        let hwnd = self.add(
            id,
            w!("EDIT"),
            "",
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            x,
            y + 23,
            width,
            26,
        )?;
        // Deliberately not ES_NUMBER: signed offsets and paste must work.
        SendMessageW(hwnd, EM_SETLIMITTEXT, Some(WPARAM(32)), None);
        Ok(())
    }
    unsafe fn combo(
        &mut self,
        id: u16,
        label: &str,
        choices: &[&str],
        x: i32,
        y: i32,
    ) -> Result<(), String> {
        self.label(label, x, y, 360, 22)?;
        let hwnd = self.add(
            id,
            w!("COMBOBOX"),
            "",
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
            x,
            y + 23,
            360,
            250,
        )?;
        for value in choices {
            SendMessageW(
                hwnd,
                CB_ADDSTRING,
                None,
                Some(LPARAM(wide(value).as_ptr() as isize)),
            );
        }
        SendMessageW(hwnd, CB_SETMINVISIBLE, Some(WPARAM(12)), None);
        Ok(())
    }
    unsafe fn button(
        &mut self,
        id: u16,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
    ) -> Result<(), String> {
        self.add(
            id,
            w!("BUTTON"),
            text,
            WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32),
            x,
            y,
            width,
            32,
        )?;
        Ok(())
    }
    unsafe fn check(
        &mut self,
        id: u16,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
    ) -> Result<(), String> {
        self.add(
            id,
            w!("BUTTON"),
            text,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            x,
            y,
            width,
            30,
        )?;
        Ok(())
    }
    unsafe fn show_page(&mut self, page: i32) {
        if page != 0 {
            self.finish_pick(false);
        }
        if page != 1 {
            self.preview = None;
        }
        self.page = page.clamp(0, 8);
        self.set_text(TITLE, PAGES[self.page as usize]);
        self.scroll_x = 0;
        self.scroll_y = 0;
        self.layout();
        let _ = InvalidateRect(Some(self.hwnd), None, true);
    }
    unsafe fn build(&mut self, options: &Options) -> Result<(), String> {
        self.replace_font()?;
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        });
        self.building_page = -1;
        self.label("TaskbarX", 16, 20, 150, 28)?;
        self.label("Configurator - Rust", 16, 49, 156, 22)?;
        for (i, name) in PAGES.iter().enumerate() {
            self.add(
                NAV + i as u16,
                w!("BUTTON"),
                name,
                WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                0,
                92 + i as i32 * 43,
                178,
                42,
            )?;
        }
        self.add(
            TITLE,
            w!("STATIC"),
            "Style",
            WINDOW_STYLE(SS_OWNERDRAW),
            200,
            43,
            610,
            36,
        )?;
        for (id, text, x, width) in [
            (STOP, "Stop TaskbarX", 16, 150),
            (RESET, "Reset", 200, 90),
            (SAVE, "Save", 300, 90),
            (RELOAD, "Reload", 400, 90),
            (RESTART, "Restart saved", 500, 135),
            (APPLY, "Apply", 714, 110),
        ] {
            self.button(id, text, x, 548, width)?;
        }
        self.add(
            STATUS,
            w!("STATIC"),
            "Loading settings...",
            WINDOW_STYLE(SS_LEFT),
            200,
            516,
            624,
            24,
        )?;

        self.add(
            DIRTY,
            w!("STATIC"),
            "Settings not loaded",
            WINDOW_STYLE(SS_LEFT),
            201,
            491,
            620,
            22,
        )?;
        self.building_page = 0;
        // Keep the protocol's canonical style values in a hidden combo; visible
        // radios use the same validated selection without duplicating settings.
        self.building_page = -2;
        self.combo(STYLE, "", STYLES, 200, 90)?;
        self.building_page = 0;
        for (index, label, y) in [
            (0, "Default", 94),
            (1, "Transparent", 131),
            (5, "Transparent Gradient", 168),
            (4, "Opaque", 205),
            (2, "Blur", 242),
            (3, "Acrylic (can flicker)", 279),
        ] {
            self.add(
                RADIO + index,
                w!("BUTTON"),
                label,
                WS_TABSTOP | WINDOW_STYLE(BS_RADIOBUTTON as u32),
                201,
                y,
                245,
                30,
            )?;
        }
        self.edit(
            PRIMARY_OPACITY,
            "Primary opacity % (blank = restore)",
            480,
            96,
            322,
        )?;
        self.edit(
            SECONDARY_OPACITY,
            "Secondary opacity % (blank = restore)",
            480,
            158,
            322,
        )?;
        self.edit(RADIUS, "Corner radius (px)", 480, 224, 150)?;
        self.check(SEGMENTS, "Segments", 648, 244, 160)?;
        self.check(
            MAXIMIZED,
            "Default background on maximized windows",
            201,
            312,
            610,
        )?;
        self.edit(COLOR_HEX, "#RRGGBB or #RRGGBBAA", 201, 348, 240)?;
        self.label("RGBA 0..255 (alpha is a byte)", 465, 348, 335, 22)?;
        self.button(COLOR_DIALOG, "Color dialog...", 465, 370, 160)?;
        self.button(EYEDROPPER, "Screen picker", 640, 370, 160)?;
        self.add(
            SWATCH,
            w!("STATIC"),
            "Color preview",
            WINDOW_STYLE(SS_OWNERDRAW),
            201,
            412,
            132,
            70,
        )?;
        for (i, (id, label)) in [
            (RED, "Red"),
            (GREEN, "Green"),
            (BLUE, "Blue"),
            (ALPHA, "Alpha"),
        ]
        .iter()
        .enumerate()
        {
            let y = 408 + i as i32 * 19;
            self.label(label, 350, y, 55, 19)?;
            let slider = self.add(
                SLIDER + i as u16,
                w!("msctls_trackbar32"),
                label,
                WS_TABSTOP | WINDOW_STYLE(TBS_NOTICKS),
                405,
                y,
                310,
                19,
            )?;
            SendMessageW(
                slider,
                TBM_SETRANGE,
                Some(WPARAM(1)),
                Some(LPARAM(255 << 16)),
            );
            self.add(
                *id,
                w!("EDIT"),
                "",
                WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                730,
                y,
                70,
                19,
            )?;
        }
        self.building_page = 1;
        self.combo(ANIMATION, "Animation Style", ANIMATIONS, 201, 96)?;
        self.button(ANIMATION_PREV, "Previous", 580, 115, 100)?;
        self.button(ANIMATION_NEXT, "Next", 695, 115, 100)?;
        let battery_choices: Vec<_> = std::iter::once("Inherit ordinary animation")
            .chain(ANIMATIONS.iter().copied())
            .collect();
        self.combo(BATTERY_ANIMATION, "On battery", &battery_choices, 201, 170)?;
        self.button(BATTERY_PREV, "Previous", 580, 189, 100)?;
        self.button(BATTERY_NEXT, "Next", 695, 189, 100)?;
        self.edit(DURATION, "Duration (ms; 0 = instant)", 201, 245, 260)?;
        self.button(PREVIEW, "Preview ordinary", 201, 318, 175)?;
        self.button(PREVIEW_BATTERY, "Preview battery", 390, 318, 175)?;
        self.button(PREVIEW_CANCEL, "Cancel preview", 580, 318, 175)?;
        let preview = self.add(
            PREVIEW_TRACK,
            w!("msctls_trackbar32"),
            "Local animation preview",
            WINDOW_STYLE(TBS_NOTICKS),
            201,
            375,
            600,
            32,
        )?;
        SendMessageW(
            preview,
            TBM_SETRANGE,
            Some(WPARAM(1)),
            Some(LPARAM(1000 << 16)),
        );
        let _ = EnableWindow(preview, false);
        self.label("Previews affect only this slider, never taskbars or the engine.\nBattery inheritance follows the ordinary selection, not a fixed default.", 201, 431, 610, 48)?;

        self.building_page = 2;
        self.edit(PRIMARY_OFFSET, "Primary Taskbar Offset (px)", 201, 96, 280)?;
        self.edit(
            SECONDARY_OFFSET,
            "Secondary Taskbar Offset (px)",
            201,
            170,
            280,
        )?;
        self.label(
            "Skip primary monitor widths (px; blank slots are unused)",
            201,
            245,
            610,
            24,
        )?;
        for (id, x) in [(SKIP1, 201), (SKIP2, 405), (SKIP3, 609)] {
            self.edit(id, "1..65535", x, 277, 185)?;
        }
        self.check(
            BETWEEN,
            "Center between Start and tray / clock",
            201,
            353,
            610,
        )?;
        self.check(CENTER, "Center Taskbar", 201, 392, 610)?;
        self.check(OVERLAP, "Revert centering on tray overlap", 201, 431, 610)?;
        self.label(
            "Signed offsets are physical pixels relative to the center.",
            201,
            466,
            610,
            22,
        )?;

        self.building_page = 3;
        self.edit(DELAY, "Startup delay preference (seconds)", 201, 96, 325)?;
        self.button(STARTUP_REFRESH, "Refresh status", 201, 166, 175)?;
        self.button(STARTUP_ENABLE, "Enable / update", 390, 166, 185)?;
        self.button(STARTUP_DISABLE, "Disable / remove", 590, 166, 210)?;
        self.label("Enable uses the selected SAVED configuration and the delay above.\nIt does not save this form. Save/Apply never change startup registration.\nChanging delay alone does not update Task Scheduler.", 201, 210, 620, 65)?;
        self.readonly(
            STARTUP_STATUS,
            "Status not queried. Use Refresh status. No registration occurs on opening.",
            201,
            290,
            600,
            192,
        )?;

        self.building_page = 4;
        for (id, label, y) in [
            (PRIMARY, "Primary taskbar (centering and styling)", 96),
            (SECONDARY, "Secondary taskbars (centering and styling)", 133),
            (TOOLBARS, "Compensate for toolbar / tray size changes", 170),
            (FULLSCREEN, "Pause on fullscreen applications", 207),
            (HIDE_PRIMARY_START, "Hide primary Start button", 244),
            (HIDE_SECONDARY_START, "Hide secondary Start buttons", 281),
            (HIDE_PRIMARY_TRAY, "Hide primary tray area", 318),
            (HIDE_SECONDARY_TRAY, "Hide secondary tray areas", 355),
            (TRAY_ICON, "Show engine tray icon", 392),
        ] {
            self.check(id, label, 201, y, 610)?;
        }
        self.label(
            "Changes take effect only with Apply or a later engine start.",
            201,
            452,
            610,
            30,
        )?;

        self.building_page = 5;
        self.combo(
            MEASUREMENT,
            "Measurement backend (MSAA / UI Automation)",
            MEASUREMENT_BACKENDS,
            201,
            96,
        )?;
        self.check(CONSOLE, "Show engine console", 580, 115, 230)?;
        self.edit(POLL, "Poll interval (ms)", 201, 162, 275)?;
        self.edit(
            BATTERY_POLL,
            "Battery poll (blank = inherit)",
            505,
            162,
            296,
        )?;
        self.label("Selected engine / saved configuration", 201, 220, 600, 22)?;
        self.readonly(
            401,
            &options.engine.display().to_string(),
            201,
            243,
            600,
            25,
        )?;
        self.readonly(
            402,
            &options
                .config
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "%APPDATA%\\TaskbarX-Rust\\settings.json".into()),
            201,
            274,
            600,
            25,
        )?;
        self.button(DIAGNOSE, "Read-only diagnostics", 201, 309, 270)?;
        let output = self.readonly(OUTPUT, "Loading settings", 201, 354, 600, 128)?;
        SendMessageW(output, EM_SETLIMITTEXT, Some(WPARAM(512 * 1024)), None);

        self.building_page = 6;
        self.edit(SHORTCUT_PATH, "Chosen shortcut path (.lnk)", 201, 96, 600)?;
        self.edit(
            SHORTCUT_ICON,
            "Optional icon path (blank = default)",
            201,
            161,
            600,
        )?;
        for id in [SHORTCUT_PATH, SHORTCUT_ICON] {
            SendMessageW(self.control(id), EM_SETLIMITTEXT, Some(WPARAM(32767)), None);
        }
        self.button(SHORTCUT_GENERATE, "Show command", 201, 229, 180)?;
        self.button(SHORTCUT_CREATE, "Create shortcut", 399, 229, 180)?;
        self.button(SHORTCUT_REMOVE, "Remove owned .lnk", 597, 229, 204)?;
        self.readonly(
            SHORTCUT_COMMAND,
            &taskbarx_shell::format_command_line(&options.start_menu_launch())?,
            201,
            278,
            600,
            78,
        )?;
        self.button(OPEN_START, "Open Start now", 201, 375, 200)?;
        self.label("Creation/removal requires an explicit click and affects only the chosen path.\nRemoval checks ownership; unrelated shortcuts are not removed.\nPinning to the taskbar is a manual Windows operation.\nNo Start buttons are hidden by previewing or creating this shortcut.", 201, 417, 620, 68)?;

        self.building_page = 7;
        self.label("TaskbarX", 201, 105, 600, 35)?;
        self.label(
            concat!(
                "Rust ",
                env!("CARGO_PKG_VERSION"),
                " - original interface by Chris Andriessen"
            ),
            201,
            153,
            600,
            30,
        )?;
        self.label("An independent rewrite of ChrisAnd1998/TaskbarX.\nNative Rust / Win32 - no .NET or browser runtime.\n\nDesigned for the classic Windows 10 taskbar.\nThe modern Windows 11 taskbar is unsupported.",201,208,600,145)?;
        self.button(ABOUT, "MIT license / attribution", 201, 389, 280)?;
        self.label("Upstream release checks do not identify updates to this Rust rewrite.\nClosing this window leaves the engine running; Stop restores it.",201,435,610,48)?;
        self.building_page = 8;
        self.label("Remove TaskbarX Rust", 201, 104, 600, 35)?;
        self.label("These are separate, explicit actions. Settings are preserved.\nNo application directory or unrelated scheduled task is deleted.", 201, 155, 610, 62)?;
        self.button(REMOVE_STARTUP, "Remove owned startup task", 201, 239, 340)?;
        self.button(REMOVE_STOP, "Graceful stop and restore", 201, 292, 340)?;
        self.label("After stopping successfully, close this window and manually remove\nthe portable application folder if desired. Keep your configuration.\nRemove any custom Start shortcut explicitly on the Start Button page.", 201, 369, 610, 95)?;
        self.building_page = -1;
        self.show_page(0);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn readonly(
        &mut self,
        id: u16,
        text: &str,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> Result<HWND, String> {
        let style = if height <= 30 {
            WINDOW_STYLE(ES_READONLY as u32 | ES_AUTOHSCROLL as u32)
        } else {
            WS_VSCROLL
                | WINDOW_STYLE(ES_READONLY as u32 | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32)
        };
        self.add(
            id,
            w!("EDIT"),
            text,
            WS_BORDER | WS_TABSTOP | style,
            x,
            y,
            width,
            height,
        )
    }

    unsafe fn replace_font(&mut self) -> Result<(), String> {
        let font = CreateFontW(
            -px(14, self.dpi),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Segoe UI"),
        );
        if font.is_invalid() {
            return Err("Cannot create the native UI font".into());
        }
        let old = std::mem::replace(&mut self.font, font);
        for item in &self.items {
            SendMessageW(
                item.hwnd,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(0)),
            );
        }
        if !old.is_invalid() {
            let _ = DeleteObject(old.into());
        }
        Ok(())
    }

    unsafe fn layout(&mut self) {
        // Showing/hiding either scrollbar changes the client rectangle. Settle
        // both axes here instead of depending on reentrant WM_SIZE callbacks.
        for _ in 0..3 {
            let mut rect = RECT::default();
            if GetClientRect(self.hwnd, &mut rect).is_err() {
                return;
            }
            self.scroll_x = self
                .scroll_x
                .clamp(0, (px(CONTENT_W, self.dpi) - rect.right).max(0));
            self.scroll_y = self
                .scroll_y
                .clamp(0, (px(CONTENT_H, self.dpi) - rect.bottom).max(0));
            for (bar, total, page, pos) in [
                (SB_HORZ, px(CONTENT_W, self.dpi), rect.right, self.scroll_x),
                (SB_VERT, px(CONTENT_H, self.dpi), rect.bottom, self.scroll_y),
            ] {
                let info = SCROLLINFO {
                    cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
                    fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                    nMin: 0,
                    nMax: total - 1,
                    nPage: page.max(0) as u32,
                    nPos: pos,
                    ..Default::default()
                };
                SetScrollInfo(self.hwnd, bar, &info, true);
            }
            let mut updated = RECT::default();
            if GetClientRect(self.hwnd, &mut updated).is_err()
                || (rect.right == updated.right && rect.bottom == updated.bottom)
            {
                break;
            }
        }
        for item in &self.items {
            let _ = ShowWindow(
                item.hwnd,
                if item.page == -1 || item.page == self.page {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            let _ = MoveWindow(
                item.hwnd,
                px(item.x, self.dpi) - self.scroll_x,
                px(item.y, self.dpi) - self.scroll_y,
                px(item.w, self.dpi),
                px(item.h, self.dpi),
                false,
            );
        }
        // Defer repaint until the window procedure has released RefCell<Ui>.
        // MoveWindow(..., true) synchronously sent WM_DRAWITEM while borrowed,
        // causing owner-drawn controls to skip painting and retain old pixels.
        let flags = RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN;
        #[cfg(test)]
        LAYOUT_REPAINT_FLAGS.with(|record| record.set(flags.0));
        let _ = RedrawWindow(Some(self.hwnd), None, None, flags);
    }

    unsafe fn ensure_focus_visible(&mut self) {
        let focus = GetFocus();
        if let Some(page) = self.items.iter().find(|i| i.hwnd == focus).map(|i| i.page) {
            if page >= 0 && page != self.page {
                self.show_page(page);
            }
        }
        if let Some(item) = self.items.iter().find(|i| i.hwnd == focus) {
            let mut rect = RECT::default();
            if GetClientRect(self.hwnd, &mut rect).is_err() {
                return;
            }
            let height = if matches!(item.id, ANIMATION | STYLE | BATTERY_ANIMATION | MEASUREMENT) {
                26
            } else {
                item.h
            };
            let left = px(item.x, self.dpi);
            let top = px(item.y, self.dpi);
            let right = left + px(item.w, self.dpi);
            let bottom = top + px(height, self.dpi);
            let old = (self.scroll_x, self.scroll_y);
            if top < self.scroll_y {
                self.scroll_y = top.saturating_sub(px(24, self.dpi));
            } else if bottom > self.scroll_y + rect.bottom {
                self.scroll_y = bottom - rect.bottom + px(8, self.dpi);
            }
            if left < self.scroll_x {
                self.scroll_x = left;
            } else if right > self.scroll_x + rect.right {
                self.scroll_x = right - rect.right;
            }
            if old != (self.scroll_x, self.scroll_y) {
                self.layout();
            }
        }
    }

    unsafe fn scroll(&mut self, horizontal: bool, command: u16) {
        let bar = if horizontal { SB_HORZ } else { SB_VERT };
        let mut info = SCROLLINFO {
            cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            ..Default::default()
        };
        if GetScrollInfo(self.hwnd, bar, &mut info).is_err() {
            return;
        }
        let pos = if horizontal {
            &mut self.scroll_x
        } else {
            &mut self.scroll_y
        };
        match SCROLLBAR_COMMAND(command as i32) {
            SB_LINEUP => *pos -= px(28, self.dpi),
            SB_LINEDOWN => *pos += px(28, self.dpi),
            SB_PAGEUP => *pos -= info.nPage as i32,
            SB_PAGEDOWN => *pos += info.nPage as i32,
            SB_THUMBTRACK | SB_THUMBPOSITION => *pos = info.nTrackPos,
            SB_TOP => *pos = 0,
            SB_BOTTOM => *pos = info.nMax,
            _ => {}
        }
        self.layout();
    }

    unsafe fn text(&self, id: u16) -> String {
        let hwnd = self.control(id);
        let mut buffer = vec![0u16; GetWindowTextLengthW(hwnd).max(0) as usize + 1];
        let len = GetWindowTextW(hwnd, &mut buffer);
        String::from_utf16_lossy(&buffer[..len.max(0) as usize])
    }
    unsafe fn set_text(&self, id: u16, text: &str) {
        if id == OUTPUT {
            let summary = text.lines().next().unwrap_or("");
            let _ = SetWindowTextW(self.control(STATUS), PCWSTR(wide(summary).as_ptr()));
        }
        let _ = SetWindowTextW(
            self.control(id),
            PCWSTR(wide(&text.replace('\n', "\r\n").replace("\r\r\n", "\r\n")).as_ptr()),
        );
    }
    unsafe fn selected(&self, id: u16, choices: &[&str]) -> String {
        let index = SendMessageW(self.control(id), CB_GETCURSEL, None, None).0;
        choices.get(index as usize).copied().unwrap_or("").into()
    }
    unsafe fn read_form(&self) -> Form {
        let checked = |id| {
            SendMessageW(self.control(id), BM_GETCHECK, None, None).0 == BST_CHECKED.0 as isize
        };
        let battery_index =
            SendMessageW(self.control(BATTERY_ANIMATION), CB_GETCURSEL, None, None).0;
        Form {
            schema_version: self.schema_version,
            center: checked(CENTER),
            primary: checked(PRIMARY),
            secondary: checked(SECONDARY),
            center_between: checked(BETWEEN),
            taskbar_segments: checked(SEGMENTS),
            default_style_on_maximized: checked(MAXIMIZED),
            pause_on_fullscreen: checked(FULLSCREEN),
            hide_primary_start: checked(HIDE_PRIMARY_START),
            hide_secondary_start: checked(HIDE_SECONDARY_START),
            hide_primary_tray: checked(HIDE_PRIMARY_TRAY),
            hide_secondary_tray: checked(HIDE_SECONDARY_TRAY),
            revert_on_tray_overlap: checked(OVERLAP),
            toolbar_compensation: checked(TOOLBARS),
            tray_icon: checked(TRAY_ICON),
            show_console: checked(CONSOLE),
            primary_offset: self.text(PRIMARY_OFFSET),
            secondary_offset: self.text(SECONDARY_OFFSET),
            animation_ms: self.text(DURATION),
            poll_ms: self.text(POLL),
            primary_opacity: self.text(PRIMARY_OPACITY),
            secondary_opacity: self.text(SECONDARY_OPACITY),
            corner_radius: self.text(RADIUS),
            battery_poll_ms: self.text(BATTERY_POLL),
            startup_delay_secs: self.text(DELAY),
            animation: self.selected(ANIMATION, ANIMATIONS),
            battery_animation: if battery_index == 0 {
                String::new()
            } else {
                ANIMATIONS
                    .get((battery_index - 1) as usize)
                    .copied()
                    .unwrap_or("invalid selection")
                    .into()
            },
            style: self.selected(STYLE, STYLES),
            measurement: self.selected(MEASUREMENT, MEASUREMENT_BACKENDS),
            color: [
                self.text(RED),
                self.text(GREEN),
                self.text(BLUE),
                self.text(ALPHA),
            ],
            skip_resolutions: [self.text(SKIP1), self.text(SKIP2), self.text(SKIP3)],
        }
    }
    unsafe fn fill(&mut self, form: &Form) {
        self.finish_pick(false);
        self.preview = None;
        self.populating = true;
        self.schema_version = form.schema_version;
        for (id, value) in [
            (CENTER, form.center),
            (PRIMARY, form.primary),
            (SECONDARY, form.secondary),
            (BETWEEN, form.center_between),
            (SEGMENTS, form.taskbar_segments),
            (MAXIMIZED, form.default_style_on_maximized),
            (FULLSCREEN, form.pause_on_fullscreen),
            (HIDE_PRIMARY_START, form.hide_primary_start),
            (HIDE_SECONDARY_START, form.hide_secondary_start),
            (HIDE_PRIMARY_TRAY, form.hide_primary_tray),
            (HIDE_SECONDARY_TRAY, form.hide_secondary_tray),
            (OVERLAP, form.revert_on_tray_overlap),
            (TOOLBARS, form.toolbar_compensation),
            (TRAY_ICON, form.tray_icon),
            (CONSOLE, form.show_console),
        ] {
            SendMessageW(
                self.control(id),
                BM_SETCHECK,
                Some(WPARAM(if value { BST_CHECKED.0 as usize } else { 0 })),
                None,
            );
        }
        for (id, text) in [
            (PRIMARY_OFFSET, &form.primary_offset),
            (SECONDARY_OFFSET, &form.secondary_offset),
            (DURATION, &form.animation_ms),
            (POLL, &form.poll_ms),
            (PRIMARY_OPACITY, &form.primary_opacity),
            (SECONDARY_OPACITY, &form.secondary_opacity),
            (RADIUS, &form.corner_radius),
            (BATTERY_POLL, &form.battery_poll_ms),
            (DELAY, &form.startup_delay_secs),
            (RED, &form.color[0]),
            (GREEN, &form.color[1]),
            (BLUE, &form.color[2]),
            (ALPHA, &form.color[3]),
            (SKIP1, &form.skip_resolutions[0]),
            (SKIP2, &form.skip_resolutions[1]),
            (SKIP3, &form.skip_resolutions[2]),
        ] {
            self.set_text(id, text);
        }
        for (id, value, choices) in [
            (ANIMATION, form.animation.as_str(), ANIMATIONS),
            (STYLE, form.style.as_str(), STYLES),
            (MEASUREMENT, form.measurement.as_str(), MEASUREMENT_BACKENDS),
        ] {
            let index = choices
                .iter()
                .position(|c| *c == value)
                .map(|i| i as isize)
                .unwrap_or(-1);
            SendMessageW(
                self.control(id),
                CB_SETCURSEL,
                Some(WPARAM(index as usize)),
                None,
            );
        }
        let battery_index = if form.battery_animation.is_empty() {
            0
        } else {
            ANIMATIONS
                .iter()
                .position(|v| *v == form.battery_animation)
                .map(|i| i as isize + 1)
                .unwrap_or(-1)
        };
        SendMessageW(
            self.control(BATTERY_ANIMATION),
            CB_SETCURSEL,
            Some(WPARAM(battery_index as usize)),
            None,
        );
        self.sync_style();
        self.sync_color_from_rgba();
        self.has_form = true;
        self.populating = false;
        self.update_dirty();
    }
    unsafe fn validated_form(
        &self,
    ) -> Result<Form, taskbarx_configurator::settings::ValidationError> {
        let form = self.read_form();
        let rgba = form.rgba()?;
        hex_rgba(&self.text(COLOR_HEX), rgba[3])?;
        form.settings()?;
        Ok(form)
    }
    unsafe fn update_dirty(&self) {
        let saved = self.validated_form().ok().and_then(|f| f.settings().ok());
        let text = if !self.has_form {
            "Settings not loaded"
        } else if self.saved.is_some() && saved == self.saved {
            "Form matches last loaded/saved settings. Startup registration is separate."
        } else {
            "UNSAVED settings. Save/Apply persists; Restart and startup use saved settings."
        };
        self.set_text(DIRTY, text);
    }
    unsafe fn edited(&mut self) {
        self.preview = None;
        self.update_dirty();
        self.set_text(
            OUTPUT,
            "Form edited locally. Save persists; Apply saves/restarts. Startup is unchanged.",
        );
    }
    unsafe fn availability(&self) {
        // Explicit ID groups only: preview indicator, hidden style combo, labels
        // and read-only reports must never be enabled by a blanket loop.
        let local = !self.busy && self.picker.is_none() && !self.color_dialog_requested;
        for item in &self.items {
            let form_control = is_form_control(item.id)
                || (RADIO..RADIO + 6).contains(&item.id)
                || (SLIDER..SLIDER + 4).contains(&item.id)
                || matches!(
                    item.id,
                    COLOR_DIALOG
                        | EYEDROPPER
                        | ANIMATION_PREV
                        | ANIMATION_NEXT
                        | BATTERY_PREV
                        | BATTERY_NEXT
                        | PREVIEW
                        | PREVIEW_BATTERY
                        | PREVIEW_CANCEL
                );
            let action = matches!(
                item.id,
                SAVE | APPLY
                    | STOP
                    | RELOAD
                    | RESET
                    | DIAGNOSE
                    | RESTART
                    | STARTUP_REFRESH
                    | STARTUP_ENABLE
                    | STARTUP_DISABLE
                    | SHORTCUT_GENERATE
                    | SHORTCUT_CREATE
                    | SHORTCUT_REMOVE
                    | OPEN_START
                    | REMOVE_STARTUP
                    | REMOVE_STOP
                    | SHORTCUT_PATH
                    | SHORTCUT_ICON
            );
            if form_control || action {
                let needs_form = form_control || matches!(item.id, SAVE | APPLY | STARTUP_ENABLE);
                let _ = EnableWindow(item.hwnd, local && (!needs_form || self.has_form));
            }
        }
    }
    unsafe fn start(&mut self, action: Action) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.availability();
        self.set_text(OUTPUT, "Working... CLI requests run off the UI thread with deadlines. No other action will run concurrently.");
        if self.tx.send(action).is_err() {
            self.busy = false;
            self.availability();
            self.set_text(OUTPUT, "The engine-command worker is unavailable. Close and reopen the configurator. Runtime state is unknown.");
        }
    }
    unsafe fn drain(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(Event::Done(completion)) => {
                    self.busy = false;
                    self.saved = completion.saved;
                    self.set_text(STARTUP_STATUS, &completion.startup.as_ref().map(startup_report).unwrap_or_else(|| "Status unknown / not refreshed. Use Refresh status. No registration is assumed.".into()));
                    match completion.result {
                        Ok(outcome) => {
                            if let Some(form) = outcome.form {
                                self.fill(&form);
                            }
                            self.set_text(OUTPUT, &outcome.message);
                        }
                        Err(error) => {
                            self.set_text(OUTPUT, &format!("ERROR - {error}\n\nIf loading failed, fix the file/engine and Reload, or explicitly Reset form to defaults. Nothing was silently replaced."));
                            self.show_page(5);
                            let _ = SetFocus(Some(self.control(OUTPUT)));
                            self.ensure_focus_visible();
                        }
                    }
                    self.update_dirty();
                    self.availability();
                }
                Ok(Event::Runtime(message)) => {
                    // Preserve the action's status instead of hiding a failed
                    // stop behind an asynchronously delivered runner exit.
                    let current = self.text(OUTPUT);
                    let retained: String = current
                        .chars()
                        .rev()
                        .take(180_000)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    self.set_text(OUTPUT, &format!("{retained}\n\n{message}"));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.busy {
                        self.busy = false;
                        self.availability();
                        self.set_text(
                            OUTPUT,
                            "ERROR - command worker disconnected; operation outcome is unknown.",
                        );
                    }
                    break;
                }
            }
        }
    }

    unsafe fn action(&mut self, id: u16) {
        if self.busy && id != CLOSE {
            return;
        }
        match id {
            SAVE | APPLY => {
                let form = match self.validated_form() {
                    Ok(form) => form,
                    Err(error) => {
                        self.set_text(
                            OUTPUT,
                            &format!("Validation error - {error}. Nothing was saved or applied."),
                        );
                        self.alert = Some(format!("{error}\n\nNothing was saved or applied."));
                        let id = match error.field {
                            "primary_offset" => PRIMARY_OFFSET,
                            "secondary_offset" => SECONDARY_OFFSET,
                            "animation" => ANIMATION,
                            "animation_ms" => DURATION,
                            "poll_ms" => POLL,
                            "style" => STYLE,
                            "red" => RED,
                            "green" => GREEN,
                            "blue" => BLUE,
                            "alpha" => ALPHA,
                            "color_hex" => COLOR_HEX,
                            "primary_opacity" => PRIMARY_OPACITY,
                            "secondary_opacity" => SECONDARY_OPACITY,
                            "corner_radius" => RADIUS,
                            "battery_animation" => BATTERY_ANIMATION,
                            "battery_poll_ms" => BATTERY_POLL,
                            "skip_resolutions" => SKIP1,
                            "startup_delay_secs" => DELAY,
                            "measurement" => MEASUREMENT,
                            _ => OUTPUT,
                        };
                        if let Some(page) = self.items.iter().find(|i| i.id == id).map(|i| i.page) {
                            if page >= 0 {
                                self.show_page(page);
                            }
                        }
                        let _ = SetFocus(Some(self.control(id)));
                        self.ensure_focus_visible();
                        // Parentless modal dialogs are avoided; keeping state
                        // borrowed across MessageBox would suppress our callback.
                        return;
                    }
                };
                self.start(if id == SAVE {
                    Action::Save(form)
                } else {
                    Action::Apply(form)
                });
            }
            STOP | REMOVE_STOP => self.start(Action::Stop),
            RESTART => self.start(Action::Restart),
            STARTUP_REFRESH => self.start(Action::StartupRefresh),
            STARTUP_DISABLE | REMOVE_STARTUP => self.start(Action::StartupDisable),
            STARTUP_ENABLE => match self.read_form().startup_delay() {
                Ok(delay_secs) => self.start(Action::StartupEnable { delay_secs }),
                Err(e) => self.set_text(OUTPUT, &e.to_string()),
            },
            SHORTCUT_GENERATE => self.start(Action::GenerateShortcut),
            SHORTCUT_CREATE | SHORTCUT_REMOVE => {
                // Path text is not shell syntax: preserve spaces and Unicode as
                // one argv entry. The engine is the ownership/security authority.
                let path = PathBuf::from(self.text(SHORTCUT_PATH));
                if !path.is_absolute()
                    || !path
                        .extension()
                        .is_some_and(|v| v.eq_ignore_ascii_case("lnk"))
                {
                    self.set_text(
                        OUTPUT,
                        "Choose an absolute .lnk path first. Nothing was changed.",
                    );
                    return;
                }
                let icon_text = self.text(SHORTCUT_ICON);
                let icon = if icon_text.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(icon_text))
                };
                self.start(if id == SHORTCUT_CREATE {
                    Action::ShortcutCreate { path, icon }
                } else {
                    Action::ShortcutRemove { path }
                });
            }
            OPEN_START => self.start(Action::OpenStart),
            COLOR_DIALOG => {
                if self.validated_color().is_ok() {
                    self.color_dialog_requested = true;
                    self.availability();
                } else {
                    self.set_text(
                        OUTPUT,
                        "Enter valid RGBA and hex before opening the color dialog.",
                    );
                }
            }
            EYEDROPPER => self.begin_pick(),
            ANIMATION_PREV | ANIMATION_NEXT | BATTERY_PREV | BATTERY_NEXT => {
                let battery = matches!(id, BATTERY_PREV | BATTERY_NEXT);
                let control = self.control(if battery {
                    BATTERY_ANIMATION
                } else {
                    ANIMATION
                });
                let index = SendMessageW(control, CB_GETCURSEL, None, None).0;
                let count = ANIMATIONS.len() as isize + isize::from(battery);
                let delta = if matches!(id, ANIMATION_NEXT | BATTERY_NEXT) {
                    1
                } else {
                    -1
                };
                SendMessageW(
                    control,
                    CB_SETCURSEL,
                    Some(WPARAM((index + delta).rem_euclid(count) as usize)),
                    None,
                );
                self.edited();
            }
            PREVIEW | PREVIEW_BATTERY => match self.read_form().preview(id == PREVIEW_BATTERY) {
                Ok(animation) => {
                    self.preview = Some(PreviewPlayback {
                        started: Instant::now(),
                        animation,
                    });
                    self.preview_tick();
                }
                Err(e) => self.set_text(OUTPUT, &format!("Preview: {e}")),
            },
            PREVIEW_CANCEL => {
                self.preview = None;
            }
            RELOAD => self.start(Action::Reload),
            RESET => self.start(Action::Reset),
            DIAGNOSE => self.start(Action::Diagnostics),
            ABOUT => {
                self.show_page(5);
                self.set_text(OUTPUT, UPSTREAM_LICENSE);
                let _ = SetFocus(Some(self.control(OUTPUT)));
                self.ensure_focus_visible();
            }
            CLOSE => {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            _ => {}
        }
    }

    unsafe fn sync_style(&self) {
        let selected = SendMessageW(self.control(STYLE), CB_GETCURSEL, None, None).0;
        for i in 0..6 {
            SendMessageW(
                self.control(RADIO + i),
                BM_SETCHECK,
                Some(WPARAM(if selected == i as isize {
                    BST_CHECKED.0 as usize
                } else {
                    0
                })),
                None,
            );
        }
    }
    unsafe fn sync_sliders(&self) {
        for (i, id) in [RED, GREEN, BLUE, ALPHA].iter().enumerate() {
            if let Ok(value) = self.text(*id).parse::<u8>() {
                SendMessageW(
                    self.control(SLIDER + i as u16),
                    TBM_SETPOS,
                    Some(WPARAM(1)),
                    Some(LPARAM(value as isize)),
                );
            }
        }
    }
    unsafe fn validated_color(&self) -> Result<[u8; 4], String> {
        let color = self.read_form().rgba().map_err(|e| e.to_string())?;
        hex_rgba(&self.text(COLOR_HEX), color[3]).map_err(|e| e.to_string())?;
        Ok(color)
    }
    unsafe fn set_color(&mut self, color: [u8; 4]) {
        let previous = self.populating;
        self.populating = true;
        for (id, value) in [RED, GREEN, BLUE, ALPHA].into_iter().zip(color) {
            self.set_text(id, &value.to_string());
        }
        self.sync_color_from_rgba();
        self.populating = previous;
        self.update_dirty();
    }
    unsafe fn sync_color_from_rgba(&self) {
        self.sync_sliders();
        if let Ok(color) = self.read_form().rgba() {
            self.set_text(COLOR_HEX, &rgba_hex(color));
        }
        let _ = InvalidateRect(Some(self.control(SWATCH)), None, true);
    }
    unsafe fn sync_color_from_hex(&mut self) {
        let hex_text = self.text(COLOR_HEX);
        let alpha = self.text(ALPHA).trim().parse::<u8>();
        // Eight digits replace alpha and can repair an incomplete alpha edit.
        // Six digits must preserve a valid existing alpha, never invent one.
        if alpha.is_ok() || hex_text.trim().len() == 9 {
            if let Ok(color) = hex_rgba(&hex_text, alpha.unwrap_or(0)) {
                // Keep the entered hex (including six-digit alpha-preserving
                // syntax) while editing; synchronously update every RGBA input.
                let hex = self.text(COLOR_HEX);
                self.set_color(color);
                self.set_text(COLOR_HEX, &hex);
            }
        }
        self.update_dirty();
    }
    unsafe fn complete_color_dialog(
        &mut self,
        color: Option<[u8; 4]>,
        custom: [COLORREF; 16],
        error: u32,
    ) {
        self.color_dialog_requested = false;
        self.custom_colors = custom;
        if let Some(color) = color {
            self.set_color(color);
            self.edited();
        } else if error != 0 {
            self.set_text(
                OUTPUT,
                &format!("Color dialog failed: {error:#x}. Color unchanged."),
            );
        } else {
            self.set_text(OUTPUT, "Color dialog cancelled; color unchanged.");
        }
        self.availability();
    }
    unsafe fn begin_pick(&mut self) {
        match self.validated_color() {
            Ok(original) => {
                self.preview = None;
                self.picker = Some(PickSession {
                    original,
                    hex: self.text(COLOR_HEX),
                });
                SetCapture(self.hwnd);
                if GetCapture() != self.hwnd {
                    self.picker = None;
                    self.set_text(OUTPUT, "Cannot capture pointer for screen picker.");
                    return;
                }
                self.availability();
                self.set_text(OUTPUT, "Screen picker: move to preview; left click accepts. Escape, right click or leaving this page cancels. Alpha is preserved.");
            }
            Err(e) => self.set_text(OUTPUT, &format!("Screen picker: {e}")),
        }
    }
    unsafe fn sample_screen(&mut self) {
        if self.picker.is_none() {
            return;
        }
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_err() {
            self.finish_pick(false);
            return;
        }
        let dc = GetDC(None);
        if dc.is_invalid() {
            self.finish_pick(false);
            return;
        }
        let pixel = GetPixel(dc, point.x, point.y);
        ReleaseDC(None, dc);
        if pixel.0 == u32::MAX {
            self.finish_pick(false);
            return;
        }
        let alpha = self.picker.as_ref().unwrap().original[3];
        self.set_color([
            pixel.0 as u8,
            (pixel.0 >> 8) as u8,
            (pixel.0 >> 16) as u8,
            alpha,
        ]);
    }
    unsafe fn cancel_local_work(&mut self) -> bool {
        if self.picker.is_some() {
            self.finish_pick(false);
            true
        } else {
            self.preview.take().is_some()
        }
    }
    unsafe fn finish_pick(&mut self, accept: bool) {
        if let Some(session) = self.picker.take() {
            if GetCapture() == self.hwnd {
                let _ = ReleaseCapture();
            }
            if !accept {
                self.set_color(session.original);
                self.set_text(COLOR_HEX, &session.hex);
            }
            self.update_dirty();
            self.availability();
            self.set_text(
                OUTPUT,
                if accept {
                    "Screen color selected locally; not saved or applied."
                } else {
                    "Screen picker cancelled; previous color restored."
                },
            );
        }
    }
    unsafe fn preview_tick(&mut self) {
        if let Some(playback) = &self.preview {
            match playback
                .animation
                .sample(playback.started.elapsed().as_millis().min(u64::MAX as u128) as u64)
            {
                Ok((position, done)) => {
                    SendMessageW(
                        self.control(PREVIEW_TRACK),
                        TBM_SETPOS,
                        Some(WPARAM(1)),
                        Some(LPARAM(position as isize)),
                    );
                    if done {
                        self.preview = None;
                    }
                }
                Err(e) => {
                    self.preview = None;
                    self.set_text(OUTPUT, &e);
                }
            }
        }
    }
    unsafe fn draw_text(&self, dc: HDC, text: &str, mut rect: RECT, size: i32, color: COLORREF) {
        let font = CreateFontW(
            -px(size, self.dpi),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            if ICONS.contains(&text) {
                w!("Segoe MDL2 Assets")
            } else {
                w!("Segoe UI")
            },
        );
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, color);
        let mut text: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(
            dc,
            &mut text,
            &mut rect,
            DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_END_ELLIPSIS,
        );
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
    }
    unsafe fn draw_chrome(&self, draw: &DRAWITEMSTRUCT) {
        let id = draw.CtlID as u16;
        let r = draw.rcItem;
        if id == TITLE {
            FillRect(draw.hDC, &r, HBRUSH(GetStockObject(WHITE_BRUSH).0));
            self.draw_text(
                draw.hDC,
                PAGES[self.page as usize],
                r,
                24,
                COLORREF(0x202020),
            );
            return;
        }
        let index = (id - NAV) as usize;
        let selected = index == self.page as usize;
        let brush = CreateSolidBrush(COLORREF(if selected { 0x00eee7de } else { 0x00f3f3f3 }));
        FillRect(draw.hDC, &r, brush);
        let _ = DeleteObject(brush.into());
        if selected {
            let accent = CreateSolidBrush(COLORREF(0x00d77800));
            FillRect(
                draw.hDC,
                &RECT {
                    left: r.left,
                    top: r.top + px(10, self.dpi),
                    right: r.left + px(3, self.dpi),
                    bottom: r.bottom - px(10, self.dpi),
                },
                accent,
            );
            let _ = DeleteObject(accent.into());
        }
        self.draw_text(
            draw.hDC,
            ICONS[index],
            RECT {
                left: r.left + px(14, self.dpi),
                right: r.left + px(45, self.dpi),
                ..r
            },
            20,
            COLORREF(0x303030),
        );
        self.draw_text(
            draw.hDC,
            PAGES[index],
            RECT {
                left: r.left + px(56, self.dpi),
                right: r.right - px(6, self.dpi),
                ..r
            },
            14,
            COLORREF(0x202020),
        );
        if draw.itemState.0 & ODS_FOCUS.0 != 0 {
            let _ = DrawFocusRect(
                draw.hDC,
                &RECT {
                    left: r.left + 4,
                    top: r.top + 3,
                    right: r.right - 4,
                    bottom: r.bottom - 3,
                },
            );
        }
    }

    unsafe fn swatch(&self, draw: &DRAWITEMSTRUCT) {
        let rect = draw.rcItem;
        let color = self.read_form().rgba();
        let tile = px(10, self.dpi).max(1);
        for y in (rect.top..rect.bottom).step_by(tile as usize) {
            for x in (rect.left..rect.right).step_by(tile as usize) {
                let background = if ((x - rect.left) / tile + (y - rect.top) / tile) % 2 == 0 {
                    235u32
                } else {
                    180
                };
                let rgb = if let Ok([r, g, b, a]) = color {
                    [r, g, b]
                        .map(|c| (c as u32 * a as u32 + background * (255 - a as u32) + 127) / 255)
                } else {
                    [background; 3]
                };
                let brush = CreateSolidBrush(COLORREF(rgb[0] | rgb[1] << 8 | rgb[2] << 16));
                if !brush.is_invalid() {
                    let tile_rect = RECT {
                        left: x,
                        top: y,
                        right: (x + tile).min(rect.right),
                        bottom: (y + tile).min(rect.bottom),
                    };
                    FillRect(draw.hDC, &tile_rect, brush);
                    let _ = DeleteObject(brush.into());
                }
            }
        }
        if color.is_err() {
            let mut text = wide("Enter valid RGBA");
            let mut rect = rect;
            SetBkMode(draw.hDC, TRANSPARENT);
            text.pop();
            DrawTextW(
                draw.hDC,
                &mut text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}

fn is_form_control(id: u16) -> bool {
    matches!(
        id,
        CENTER
            | PRIMARY
            | SECONDARY
            | BETWEEN
            | PRIMARY_OFFSET
            | SECONDARY_OFFSET
            | ANIMATION
            | DURATION
            | POLL
            | RED
            | GREEN
            | BLUE
            | ALPHA
            | PRIMARY_OPACITY
            | SECONDARY_OPACITY
            | RADIUS
            | SEGMENTS
            | MAXIMIZED
            | BATTERY_ANIMATION
            | BATTERY_POLL
            | SKIP1
            | SKIP2
            | SKIP3
            | FULLSCREEN
            | HIDE_PRIMARY_START
            | HIDE_SECONDARY_START
            | HIDE_PRIMARY_TRAY
            | HIDE_SECONDARY_TRAY
            | OVERLAP
            | TOOLBARS
            | TRAY_ICON
            | DELAY
            | MEASUREMENT
            | CONSOLE
            | COLOR_HEX
    )
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as _);
    }
    if msg == WM_NCDESTROY {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<Ui>;
    if !state.is_null() {
        if let Ok(mut ui) = (*state).try_borrow_mut() {
            match msg {
                WM_COMMAND => {
                    let id = (wp.0 & 0xffff) as u16;
                    let notification = ((wp.0 >> 16) & 0xffff) as u32;
                    if (NAV..NAV + 9).contains(&id) && notification == BN_CLICKED {
                        ui.show_page(i32::from(id - NAV));
                    } else if (RADIO..RADIO + 6).contains(&id) && notification == BN_CLICKED {
                        SendMessageW(
                            ui.control(STYLE),
                            CB_SETCURSEL,
                            Some(WPARAM((id - RADIO) as usize)),
                            None,
                        );
                        ui.sync_style();
                        ui.edited();
                    } else if ((SAVE..=PREVIEW_CANCEL).contains(&id) || id == PREVIEW)
                        && notification == BN_CLICKED
                    {
                        ui.action(id);
                    } else if id == IDCANCEL.0 as u16 {
                        if !ui.cancel_local_work() {
                            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    } else if is_form_control(id)
                        && !ui.populating
                        && (notification == EN_CHANGE
                            || notification == CBN_SELCHANGE
                            || notification == BN_CLICKED)
                    {
                        if id == COLOR_HEX {
                            ui.sync_color_from_hex();
                        } else if [RED, GREEN, BLUE, ALPHA].contains(&id) {
                            ui.sync_color_from_rgba();
                        }
                        ui.edited();
                    }
                    return LRESULT(0);
                }
                WM_MOUSEMOVE if ui.picker.is_some() => {
                    ui.sample_screen();
                    return LRESULT(0);
                }
                WM_LBUTTONDOWN if ui.picker.is_some() => {
                    ui.sample_screen();
                    ui.finish_pick(true);
                    return LRESULT(0);
                }
                WM_RBUTTONDOWN | WM_CANCELMODE | WM_CAPTURECHANGED if ui.picker.is_some() => {
                    ui.finish_pick(false);
                    return LRESULT(0);
                }
                WM_ACTIVATEAPP if wp.0 == 0 => {
                    ui.finish_pick(false);
                    ui.preview = None;
                }
                WM_TIMER => {
                    ui.drain();
                    ui.preview_tick();
                    return LRESULT(0);
                }
                WM_SIZE => {
                    ui.layout();
                    return LRESULT(0);
                }
                WM_HSCROLL if lp.0 != 0 => {
                    let id = GetDlgCtrlID(HWND(lp.0 as *mut _)) as u16;
                    if (SLIDER..SLIDER + 4).contains(&id) {
                        let value = SendMessageW(HWND(lp.0 as *mut _), TBM_GETPOS, None, None).0;
                        ui.set_text(
                            [RED, GREEN, BLUE, ALPHA][(id - SLIDER) as usize],
                            &value.to_string(),
                        );
                        ui.sync_color_from_rgba();
                        ui.edited();
                    }
                    return LRESULT(0);
                }
                WM_VSCROLL | WM_HSCROLL => {
                    ui.scroll(msg == WM_HSCROLL, (wp.0 & 0xffff) as u16);
                    return LRESULT(0);
                }
                WM_MOUSEWHEEL => {
                    let delta = ((wp.0 >> 16) as u16) as i16;
                    ui.scroll_y -= i32::from(delta) * px(72, ui.dpi) / 120;
                    ui.layout();
                    return LRESULT(0);
                }
                WM_DPICHANGED => {
                    ui.dpi = (wp.0 & 0xffff) as u32;
                    if let Err(error) = ui.replace_font() {
                        ui.set_text(OUTPUT, &error);
                    }
                    let suggested = &*(lp.0 as *const RECT);
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                    ui.layout();
                    return LRESULT(0);
                }
                WM_ERASEBKGND => {
                    let dc = HDC(wp.0 as *mut _);
                    let mut r = RECT::default();
                    let _ = GetClientRect(hwnd, &mut r);
                    FillRect(dc, &r, HBRUSH(GetStockObject(WHITE_BRUSH).0));
                    let brush = CreateSolidBrush(COLORREF(0xf3f3f3));
                    FillRect(
                        dc,
                        &RECT {
                            right: px(178, ui.dpi) - ui.scroll_x,
                            ..r
                        },
                        brush,
                    );
                    let _ = DeleteObject(brush.into());
                    return LRESULT(1);
                }
                WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                    let dc = HDC(wp.0 as *mut _);
                    SetBkMode(dc, TRANSPARENT);
                    SetTextColor(dc, COLORREF(0x303030));
                    if ui
                        .items
                        .iter()
                        .any(|i| i.hwnd == HWND(lp.0 as *mut _) && i.x < 178)
                    {
                        SetDCBrushColor(dc, COLORREF(0xf3f3f3));
                        return LRESULT(GetStockObject(DC_BRUSH).0 as isize);
                    }
                    return LRESULT(GetStockObject(WHITE_BRUSH).0 as isize);
                }
                WM_DRAWITEM
                    if wp.0 == TITLE as usize
                        || (NAV as usize..NAV as usize + 9).contains(&wp.0) =>
                {
                    ui.draw_chrome(&*(lp.0 as *const DRAWITEMSTRUCT));
                    return LRESULT(1);
                }
                WM_DRAWITEM if wp.0 == SWATCH as usize => {
                    ui.swatch(&*(lp.0 as *const DRAWITEMSTRUCT));
                    return LRESULT(1);
                }
                WM_CLOSE => {
                    if ui.busy {
                        ui.set_text(OUTPUT, "A requested command is still in progress. Please wait for its bounded completion before closing. Closing does not issue stop or start.");
                        return LRESULT(0);
                    }
                    ui.finish_pick(false);
                    ui.preview = None;
                    // Release RefCell borrow before synchronous destruction.
                    drop(ui);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                _ => {}
            }
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

// The common dialog runs a nested message loop: never hold RefCell<Ui> across it.
// Its COLORREF and custom-color buffers are stack owned until it returns.
unsafe fn service_color_dialog(state: &RefCell<Ui>) {
    let (hwnd, color, mut custom) = {
        let ui = state.borrow();
        if !ui.color_dialog_requested {
            return;
        }
        let Ok(color) = ui.validated_color() else {
            drop(ui);
            let mut ui = state.borrow_mut();
            ui.color_dialog_requested = false;
            ui.availability();
            return;
        };
        (ui.hwnd, color, ui.custom_colors)
    };
    let mut choose = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        rgbResult: COLORREF(
            u32::from(color[0]) | u32::from(color[1]) << 8 | u32::from(color[2]) << 16,
        ),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..Default::default()
    };
    let selected = ChooseColorW(&mut choose).as_bool();
    let error = if selected {
        0
    } else {
        CommDlgExtendedError().0
    };
    let mut ui = state.borrow_mut();
    let selected = if selected {
        let rgb = choose.rgbResult.0;
        Some([rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8, color[3]])
    } else {
        None
    };
    ui.complete_color_dialog(selected, custom, error);
}

pub fn run(options: Options) -> Result<(), String> {
    unsafe {
        // May already be set by a host/manifest. A failure does not make the
        // pointer or control layout unsafe; GetDpiForWindow remains authoritative.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = HINSTANCE(GetModuleHandleW(None).map_err(winerr)?.0);
        let class = w!("TaskbarX.NativeConfigurator.v1");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(winerr)?,
            hbrBackground: GetSysColorBrush(COLOR_BTNFACE),
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err("Could not register the native window class".into());
        }
        let result = run_window(options, instance, class);
        let _ = UnregisterClassW(class, Some(instance));
        result
    }
}

fn command_worker<P: Process + Send + 'static>(
    options: Options,
    process: P,
) -> Result<(Sender<Action>, Receiver<Event>), String> {
    let (tx, requests) = mpsc::channel();
    let (events, rx) = mpsc::channel();
    let worker_options = options;
    thread::Builder::new()
        .name("taskbarx-cli".into())
        .spawn(move || {
            let mut controller = Controller::new(worker_options, process);
            loop {
                match requests.recv_timeout(Duration::from_millis(200)) {
                    Ok(action) => {
                        if events
                            .send(Event::Done(Box::new(Completion {
                                result: controller.perform(action),
                                saved: controller.saved.clone(),
                                startup: controller.startup.clone(),
                            })))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                for message in controller.process.poll() {
                    if events.send(Event::Runtime(message)).is_err() {
                        return;
                    }
                }
            }
        })
        .map_err(|e| format!("Cannot create engine-command worker: {e}"))?;
    Ok((tx, rx))
}

unsafe fn run_window(options: Options, instance: HINSTANCE, class: PCWSTR) -> Result<(), String> {
    let (tx, rx) = command_worker(options.clone(), SystemProcess::default())?;
    let state = Box::new(RefCell::new(Ui {
        hwnd: HWND::default(),
        page: 0,
        building_page: -1,
        preview: None,
        picker: None,
        color_dialog_requested: false,
        custom_colors: [COLORREF(0); 16],
        saved: None,
        schema_version: 1,
        items: Vec::new(),
        dpi: GetDpiForSystem().max(96),
        font: HFONT::default(),
        scroll_x: 0,
        scroll_y: 0,
        busy: false,
        has_form: false,
        populating: false,
        alert: None,
        tx,
        rx,
    }));
    let mut work = RECT::default();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut work as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .map_err(winerr)?;
    let dpi = state.borrow().dpi;
    let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN;
    let mut outer = RECT {
        left: 0,
        top: 0,
        right: px(CONTENT_W, dpi),
        bottom: px(CONTENT_H, dpi),
    };
    AdjustWindowRectExForDpi(&mut outer, style, false, WS_EX_CONTROLPARENT, dpi).map_err(winerr)?;
    let hwnd = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class,
        w!("TaskbarX Configurator"),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        (outer.right - outer.left).min(work.right - work.left),
        (outer.bottom - outer.top).min(work.bottom - work.top),
        None,
        None,
        Some(instance),
        Some((&*state as *const RefCell<Ui>).cast()),
    )
    .map_err(winerr)?;
    let setup = (|| {
        let mut ui = state.borrow_mut();
        ui.hwnd = hwnd;
        ui.dpi = GetDpiForWindow(hwnd).max(96);
        ui.build(&options)?;
        if SetTimer(Some(hwnd), 1, 16, None) == 0 {
            return Err("Cannot create UI completion timer".into());
        }
        ui.start(Action::Initialize);
        Ok(())
    })();
    if let Err(error) = setup {
        let _ = DestroyWindow(hwnd);
        return Err(error);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = UpdateWindow(hwnd);
    let _ = SetFocus(Some(state.borrow().control(NAV)));
    let mut msg = MSG::default();
    let mut loop_error = None;
    let mut previous_focus = GetFocus();
    loop {
        let result = GetMessageW(&mut msg, None, 0, 0).0;
        if result == 0 {
            break;
        }
        if result == -1 {
            loop_error = Some("Native message loop failed".into());
            break;
        }
        // Cancellation wins even when a child edit/button owns keyboard focus.
        if msg.message == WM_KEYDOWN
            && msg.wParam.0 == VK_ESCAPE.0 as usize
            && state.borrow_mut().cancel_local_work()
        {
            continue;
        }
        // Tab, Shift+Tab, arrows, Space, access keys, and Escape work exactly as
        // a dialog. Buttons are not defaulted to Apply (Enter cannot auto-apply).
        if !IsDialogMessageW(hwnd, &msg).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if IsWindow(Some(hwnd)).as_bool() {
            let focus = GetFocus();
            if focus != previous_focus {
                if let Ok(mut ui) = state.try_borrow_mut() {
                    ui.ensure_focus_visible();
                }
                previous_focus = focus;
            }
            service_color_dialog(&state);
            let alert = state.borrow_mut().alert.take();
            if let Some(alert) = alert {
                // No Rust borrow crosses this nested message loop.
                let _ = MessageBoxW(
                    Some(hwnd),
                    PCWSTR(wide(&alert).as_ptr()),
                    w!("Check settings"),
                    MB_OK | MB_ICONWARNING,
                );
            }
        }
    }
    let _ = KillTimer(Some(hwnd), 1);
    if IsWindow(Some(hwnd)).as_bool() {
        let _ = DestroyWindow(hwnd);
    }
    // `state` drops only after WM_NCDESTROY cleared the borrowed callback pointer.
    if let Some(error) = loop_error {
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    };
    use taskbarx_configurator::{process::ProcessReply, protocol::CommandSpec};

    struct FakeProcess {
        calls: Arc<Mutex<Vec<CommandSpec>>>,
        saved: Settings,
    }
    impl Process for FakeProcess {
        fn execute(&mut self, spec: &CommandSpec) -> Result<ProcessReply, String> {
            self.calls.lock().unwrap().push(spec.clone());
            let mut reply = ProcessReply {
                code: Some(0),
                ..Default::default()
            };
            match spec.args[0].to_str().unwrap() {
                "config" if spec.args[1] == "show" => {
                    reply.stdout = taskbarx_config::to_json(&self.saved)?.into_bytes()
                }
                "config" if spec.args[1] == "import" => {
                    self.saved = taskbarx_config::from_json(
                        std::str::from_utf8(spec.stdin.as_ref().unwrap()).unwrap(),
                    )?
                }
                "stop" => {}
                "run" => {
                    reply.code = None;
                    reply.pid = Some(42);
                }
                "startup" => {
                    reply.stdout = serde_json::to_vec(&taskbarx_shell::StartupStatus {
                        registered: false,
                        enabled: false,
                        owned: false,
                        task_name: "Fixture only".into(),
                        launch: None,
                        delay_secs: None,
                        warnings: Vec::new(),
                    })
                    .unwrap()
                }
                "shortcut" => {
                    reply.stdout =
                        serde_json::json!({"ok":true,"path":PathBuf::from(&spec.args[3])})
                            .to_string()
                            .into_bytes()
                }
                "start-menu" => {}
                other => panic!("unexpected fixture command: {other}"),
            }
            Ok(reply)
        }
    }
    // Owns the hidden HWND AND its callback allocation, even on assertion failure.
    // Every command goes through the same off-thread worker as production with a
    // fake process. No taskbar/desktop window is found, moved, hidden or sampled.
    struct Fixture {
        state: Box<RefCell<Ui>>,
        class: Vec<u16>,
        instance: HINSTANCE,
        calls: Arc<Mutex<Vec<CommandSpec>>>,
    }
    impl Fixture {
        unsafe fn new() -> Self {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let instance = HINSTANCE(GetModuleHandleW(None).unwrap().0);
            let class = wide(&format!(
                "TaskbarX.Configurator.HiddenFixture.{}",
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: PCWSTR(class.as_ptr()),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&wc), 0);
            let calls = Arc::new(Mutex::new(Vec::new()));
            let options = Options {
                engine: std::env::temp_dir().join("never-executed \u{6d4b}\u{8bd5}/taskbarx.exe"),
                config: Some(std::env::temp_dir().join("fixture config \u{989c}\u{8272}.json")),
            };
            let (tx, rx) = command_worker(
                options.clone(),
                FakeProcess {
                    calls: calls.clone(),
                    saved: Settings::default(),
                },
            )
            .unwrap();
            let state = Box::new(RefCell::new(Ui {
                hwnd: HWND::default(),
                page: 0,
                building_page: -1,
                preview: None,
                picker: None,
                color_dialog_requested: false,
                custom_colors: [COLORREF(0); 16],
                saved: None,
                schema_version: 1,
                items: vec![],
                dpi: 96,
                font: HFONT::default(),
                scroll_x: 0,
                scroll_y: 0,
                busy: false,
                has_form: false,
                populating: false,
                alert: None,
                tx,
                rx,
            }));
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT,
                PCWSTR(class.as_ptr()),
                w!("Hidden controls - fake worker"),
                WS_OVERLAPPEDWINDOW | WS_VSCROLL | WS_HSCROLL,
                0,
                0,
                900,
                950,
                None,
                None,
                Some(instance),
                Some((&*state as *const RefCell<Ui>).cast()),
            )
            .unwrap();
            let fixture = Self {
                state,
                class,
                instance,
                calls,
            };
            {
                let mut ui = fixture.state.borrow_mut();
                ui.hwnd = hwnd;
                ui.build(&options).unwrap();
                ui.start(Action::Initialize);
            }
            fixture.wait();
            fixture
        }
        unsafe fn wait(&self) {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let mut ui = self.state.borrow_mut();
                ui.drain();
                if !ui.busy {
                    break;
                }
                assert!(Instant::now() < deadline, "fake worker deadline");
                drop(ui);
                thread::sleep(Duration::from_millis(5));
            }
        }
        fn count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            unsafe {
                let hwnd = self.state.borrow().hwnd;
                if IsWindow(Some(hwnd)).as_bool() {
                    let _ = DestroyWindow(hwnd);
                }
                let _ = UnregisterClassW(PCWSTR(self.class.as_ptr()), Some(self.instance));
                // Remove only our thread's WM_QUIT from this fixture's destruction.
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE);
            }
        }
    }
    fn all_settings() -> Settings {
        taskbarx_config::from_json(
            r#"{
            "center":false,"primary":false,"secondary":false,"center_between":true,
            "primary_offset":-134,"secondary_offset":99,"animation":"linear","animation_ms":0,
            "poll_ms":53,"style":"transparent-gradient","color":[255,0,254,7],
            "primary_opacity":0,"secondary_opacity":100,"corner_radius":10000,
            "taskbar_segments":true,"default_style_on_maximized":true,"battery_animation":"none",
            "battery_poll_ms":10000,"skip_resolutions":[1,1920,65535],"pause_on_fullscreen":true,
            "hide_primary_start":true,"hide_secondary_start":true,"hide_primary_tray":true,
            "hide_secondary_tray":true,"revert_on_tray_overlap":true,"toolbar_compensation":false,
            "tray_icon":true,"startup_delay_secs":86400,"measurement":"uia","show_console":true
        }"#,
        )
        .unwrap()
    }
    #[test]
    fn hidden_controls_bind_every_field_preserve_edits_and_roundtrip_saved_json() {
        unsafe {
            let fixture = Fixture::new();
            assert_eq!(fixture.count(), 1); // config show ONLY, no startup registration
            {
                let mut ui = fixture.state.borrow_mut();
                let form = Form::from(all_settings());
                ui.fill(&form);
                assert_eq!(ui.read_form(), form);
                assert_eq!(ui.read_form().settings().unwrap(), all_settings());
                let full = serde_json::to_value(all_settings()).unwrap();
                let defaults = serde_json::to_value(Settings::default()).unwrap();
                // Individually flip every field, especially the four independent
                // Start/tray flags. An all-true fixture alone misses swapped IDs.
                for key in full.as_object().unwrap().keys() {
                    let mut value = full.clone();
                    value[key] = defaults[key].clone();
                    let settings = taskbarx_config::from_json(&value.to_string()).unwrap();
                    ui.fill(&settings.clone().into());
                    assert_eq!(ui.read_form().settings().unwrap(), settings, "{key}");
                }
                for &animation in ANIMATIONS {
                    for &style in STYLES {
                        for battery in [String::new(), animation.into()] {
                            let form = Form {
                                animation: animation.into(),
                                battery_animation: battery,
                                style: style.into(),
                                ..form.clone()
                            };
                            ui.fill(&form);
                            assert_eq!(ui.read_form(), form);
                            let settings = ui.read_form().settings().unwrap();
                            let json = taskbarx_config::to_json(&settings).unwrap();
                            assert_eq!(taskbarx_config::from_json(&json).unwrap(), settings);
                        }
                    }
                }
                for opacity in [None, Some(0), Some(100)] {
                    for backend in MEASUREMENT_BACKENDS {
                        let settings = Settings {
                            primary_opacity: opacity,
                            secondary_opacity: opacity,
                            measurement: (*backend).into(),
                            ..all_settings()
                        };
                        ui.fill(&settings.clone().into());
                        assert_eq!(ui.read_form().settings().unwrap(), settings);
                    }
                }
                ui.fill(&form);
                ui.saved = Some(all_settings());
                ui.update_dirty();
                assert!(ui.text(DIRTY).starts_with("Form matches"));
                ui.set_text(PRIMARY_OFFSET, "-99");
                ui.edited();
                assert!(ui.text(DIRTY).starts_with("UNSAVED"));
                for page in 0..9 {
                    ui.show_page(page);
                }
                let mut expected = all_settings();
                expected.primary_offset = -99;
                assert_eq!(ui.read_form().settings().unwrap(), expected);
                assert_eq!(fixture.count(), 1); // all editing/page navigation is local
                ui.action(SAVE);
            }
            fixture.wait();
            let saved = fixture.state.borrow().read_form().settings().unwrap();
            assert_eq!(fixture.state.borrow().saved, Some(saved.clone()));
            assert!(fixture
                .state
                .borrow()
                .text(DIRTY)
                .starts_with("Form matches"));
            fixture.state.borrow_mut().action(RESET);
            fixture.wait();
            assert_eq!(fixture.state.borrow().read_form(), Form::default());
            assert!(fixture.state.borrow().text(DIRTY).starts_with("UNSAVED"));
            fixture.state.borrow_mut().action(RELOAD);
            fixture.wait();
            assert_eq!(
                fixture.state.borrow().read_form().settings().unwrap(),
                saved
            );
            let calls = fixture.calls.lock().unwrap();
            assert_eq!(
                calls.iter().map(CommandSpec::label).collect::<Vec<_>>(),
                ["config show", "config import --stdin", "config show"]
            );
            assert_eq!(
                taskbarx_config::from_json(
                    std::str::from_utf8(calls[1].stdin.as_ref().unwrap()).unwrap()
                )
                .unwrap(),
                saved
            );
        }
    }

    #[test]
    fn hidden_color_preview_cancel_validation_and_navigation_never_dispatch_commands() {
        unsafe {
            let fixture = Fixture::new();
            let mut ui = fixture.state.borrow_mut();
            ui.fill(&all_settings().into());
            ui.set_text(COLOR_HEX, "#12aBEf");
            ui.sync_color_from_hex();
            assert_eq!(ui.read_form().rgba().unwrap(), [18, 171, 239, 7]);
            ui.set_text(ALPHA, "incomplete");
            ui.set_text(COLOR_HEX, "#010203FE");
            ui.sync_color_from_hex();
            assert_eq!(ui.read_form().rgba().unwrap(), [1, 2, 3, 254]);
            ui.set_text(RED, "99");
            ui.sync_color_from_rgba();
            assert_eq!(ui.text(COLOR_HEX), "#630203FE");
            for invalid in ["#", "#XYZ000", "#1234567", "123456", "#\u{e9}12345"] {
                ui.set_text(COLOR_HEX, invalid);
                ui.sync_color_from_hex();
                ui.action(SAVE);
                assert!(ui.alert.take().unwrap().contains("color_hex"));
                assert_eq!(ui.read_form().rgba().unwrap(), [99, 2, 3, 254]);
                assert!(!ui.busy);
            }
            ui.sync_color_from_rgba();
            let previous = ui.read_form();
            let hex = ui.text(COLOR_HEX);
            // Dialog and screen samples are injected; never open a real dialog or
            // capture/sample the user's desktop during tests.
            ui.color_dialog_requested = true;
            ui.complete_color_dialog(None, [COLORREF(0); 16], 0);
            assert_eq!(ui.read_form(), previous);
            ui.picker = Some(PickSession {
                original: previous.rgba().unwrap(),
                hex: hex.clone(),
            });
            ui.set_color([40, 50, 60, 254]);
            ui.finish_pick(false);
            assert_eq!(ui.read_form(), previous);
            assert_eq!(ui.text(COLOR_HEX), hex);
            ui.picker = Some(PickSession {
                original: previous.rgba().unwrap(),
                hex,
            });
            ui.set_color([11, 22, 33, 254]);
            ui.show_page(1); // leaving Style cancels
            assert_eq!(ui.read_form(), previous);
            ui.picker = Some(PickSession {
                original: previous.rgba().unwrap(),
                hex: ui.text(COLOR_HEX),
            });
            ui.set_color([30, 40, 50, 254]);
            let hwnd = ui.hwnd;
            drop(ui);
            SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(IDCANCEL.0 as usize)), None);
            let mut ui = fixture.state.borrow_mut();
            assert!(ui.picker.is_none());
            assert_eq!(ui.read_form(), previous);
            assert!(IsWindow(Some(hwnd)).as_bool()); // Escape did not close the fixture
            ui.complete_color_dialog(Some([10, 20, 30, 254]), [COLORREF(0); 16], 0);
            assert_eq!(ui.read_form().rgba().unwrap(), [10, 20, 30, 254]);
            ui.action(ANIMATION_NEXT);
            ui.action(ANIMATION_PREV);
            assert_eq!(ui.read_form().animation, "linear");
            ui.action(BATTERY_PREV);
            assert_eq!(ui.read_form().battery_animation, "");
            ui.action(BATTERY_NEXT);
            assert_eq!(ui.read_form().battery_animation, "none");
            ui.action(PREVIEW_BATTERY);
            assert!(ui.preview.is_none());
            assert_eq!(
                SendMessageW(ui.control(PREVIEW_TRACK), TBM_GETPOS, None, None).0,
                1000
            );
            ui.set_text(DURATION, "10000");
            ui.action(PREVIEW);
            assert!(ui.preview.is_some());
            ui.action(PREVIEW_CANCEL);
            assert!(ui.preview.is_none());
            ui.action(PREVIEW);
            ui.show_page(2);
            assert!(ui.preview.is_none());
            ui.set_text(PRIMARY_OFFSET, "-");
            ui.action(APPLY);
            assert!(ui.alert.take().unwrap().contains("primary_offset"));
            ui.set_text(DURATION, "10001");
            ui.action(PREVIEW);
            assert!(ui.preview.is_none());
            assert_eq!(fixture.count(), 1);
        }
    }

    #[test]
    fn hidden_dpi_pages_keyboard_order_and_deferred_child_repaint() {
        unsafe {
            let fixture = Fixture::new();
            let mut ui = fixture.state.borrow_mut();
            for item in &ui.items {
                let mut text = vec![0u16; GetWindowTextLengthW(item.hwnd) as usize + 1];
                let len = GetWindowTextW(item.hwnd, &mut text);
                let text = String::from_utf16_lossy(&text[..len as usize]);
                assert!(
                    !text.contains('\u{fffd}')
                        && !text.contains('\u{e2}')
                        && !text.contains('\u{c3}'),
                    "damaged UI text: {text}"
                );
            }
            for dpi in [96, 144, 192] {
                ui.dpi = dpi;
                ui.replace_font().unwrap();
                ui.layout();
                let mut rect = RECT::default();
                GetWindowRect(ui.control(RED), &mut rect).unwrap();
                assert_eq!(rect.right - rect.left, px(70, dpi));
                for page in 0..9 {
                    ui.show_page(page);
                    assert_eq!(ui.text(TITLE), PAGES[page as usize]);
                    for item in &ui.items {
                        let visible =
                            GetWindowLongW(item.hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0;
                        assert_eq!(
                            visible,
                            item.page == -1 || item.page == page,
                            "page {page}, id {}",
                            item.id
                        );
                    }
                }
            }
            ui.dpi = 96;
            ui.layout();
            ui.show_page(0);
            ui.availability();
            // The indicator is decorative and remains disabled after busy cycles.
            for busy in [true, false] {
                ui.busy = busy;
                ui.availability();
                assert!(
                    !windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(
                        ui.control(PREVIEW_TRACK)
                    )
                    .as_bool()
                );
            }
            assert_ne!(
                GetNextDlgTabItem(ui.hwnd, Some(ui.control(NAV)), false).unwrap(),
                HWND::default()
            );
            assert!(!IsWindowVisible(ui.hwnd).as_bool()); // test never showed a window
                                                          // Windows suppresses update regions for hidden parents. Record the
                                                          // exact flags passed to the real API instead: every child must be
                                                          // invalidated, with no synchronous paint while Ui is borrowed.
            LAYOUT_REPAINT_FLAGS.with(|record| record.set(0));
            ui.layout();
            LAYOUT_REPAINT_FLAGS.with(|record| {
                assert_eq!(
                    record.get(),
                    (RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN).0
                );
                assert_eq!(record.get() & (RDW_UPDATENOW | RDW_ERASENOW).0, 0);
            });
            assert_eq!(fixture.count(), 1);
        }
    }

    #[test]
    fn hidden_explicit_actions_use_fake_worker_without_overwriting_unsaved_form() {
        unsafe {
            let fixture = Fixture::new();
            {
                let mut ui = fixture.state.borrow_mut();
                ui.fill(&all_settings().into());
                ui.set_text(DELAY, "19");
                ui.set_text(POLL, "incomplete");
                ui.edited();
                ui.action(STARTUP_ENABLE);
            }
            fixture.wait();
            assert_eq!(fixture.state.borrow().text(POLL), "incomplete");
            assert_eq!(fixture.state.borrow().text(DELAY), "19");
            assert!(fixture.state.borrow().text(DIRTY).starts_with("UNSAVED"));
            fixture.state.borrow_mut().action(RESTART);
            fixture.wait();
            fixture.state.borrow_mut().action(STARTUP_REFRESH);
            fixture.wait();
            fixture.state.borrow_mut().action(REMOVE_STARTUP);
            fixture.wait();
            fixture.state.borrow_mut().action(REMOVE_STOP);
            fixture.wait();
            let path = std::env::temp_dir().join("fixture Start \u{989c}\u{8272}.lnk");
            {
                let mut ui = fixture.state.borrow_mut();
                ui.set_text(SHORTCUT_PATH, path.to_str().unwrap());
                ui.action(SHORTCUT_CREATE);
            }
            fixture.wait();
            fixture.state.borrow_mut().action(SHORTCUT_REMOVE);
            fixture.wait();
            fixture.state.borrow_mut().action(OPEN_START);
            fixture.wait();
            let calls = fixture.calls.lock().unwrap();
            assert!(calls.iter().all(|s| s.stdin.is_none()));
            assert_eq!(
                calls[1].args[0..5],
                ["startup", "enable", "--delay-secs", "19", "--json"].map(std::ffi::OsString::from)
            );
            assert_eq!(calls[2].label(), "stop --timeout-ms 5000");
            assert_eq!(calls[3].label(), "run");
            assert_eq!(calls[7].args[3], path.as_os_str());
            assert_eq!(calls[8].args[3], path.as_os_str());
            assert_eq!(calls[9].label(), "start-menu");
            assert_eq!(fixture.state.borrow().text(POLL), "incomplete");
        }
    }
}
