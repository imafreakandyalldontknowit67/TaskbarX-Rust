//! A conventional Win32 control tree. The window borrows a stable Box<RefCell<Ui>>
//! owned by run(); WM_NCDESTROY clears that pointer, never frees it. Reentrant
//! control notifications use try_borrow_mut, not aliasing &mut references.
//! Worker threads never hold an HWND or a pointer into window state.
use std::{
    cell::RefCell,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};
use taskbarx_configurator::{
    controller::{Action, Controller, Outcome},
    process::{Process, SystemProcess},
    protocol::Options,
    settings::{Form, ANIMATIONS, STYLES},
    UPSTREAM_LICENSE,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::*,
            HiDpi::*,
            Input::KeyboardAndMouse::{EnableWindow, GetFocus, SetFocus},
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
const SAVE: u16 = 201;
const APPLY: u16 = 202;
const STOP: u16 = 203;
const RELOAD: u16 = 204;
const RESET: u16 = 205;
const DIAGNOSE: u16 = 206;
const ABOUT: u16 = 207;
const CLOSE: u16 = 208;
const OUTPUT: u16 = 301;
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

fn wide(s: &str) -> Vec<u16> {
    s.replace('\0', "\u{fffd}")
        .encode_utf16()
        .chain(Some(0))
        .collect()
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
    Done(Box<Result<Outcome, String>>),
    Runtime(String),
}
struct Ui {
    hwnd: HWND,
    page: i32,
    building_page: i32,
    preview: Option<Instant>,
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
    unsafe fn unavailable(&mut self, text: &str, x: i32, y: i32, width: i32) -> Result<(), String> {
        let h = self.add(
            0,
            w!("BUTTON"),
            text,
            WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            x,
            y,
            width,
            28,
        )?;
        let _ = EnableWindow(h, false);
        Ok(())
    }
    unsafe fn show_page(&mut self, page: i32) {
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

        self.building_page = 0;
        // Keep the protocol's canonical style values in a hidden combo; visible
        // radios use the same validated selection without duplicating settings.
        self.building_page = -2;
        self.combo(STYLE, "", &STYLES, 200, 90)?;
        self.building_page = 0;
        for (index, label, y) in [
            (0, "Default", 94),
            (1, "Transparent", 131),
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
        let h = self.add(
            0,
            w!("BUTTON"),
            "Transparent Gradient",
            WINDOW_STYLE(BS_RADIOBUTTON as u32),
            201,
            168,
            240,
            30,
        )?;
        let _ = EnableWindow(h, false);
        self.label("Total Opacity", 548, 96, 200, 24)?;
        self.label("Primary taskbar", 515, 132, 150, 24)?;
        self.label("Secondary taskbar", 515, 170, 155, 24)?;
        self.label("Not ported", 684, 132, 130, 24)?;
        self.label("Not ported", 684, 170, 130, 24)?;
        self.label("Corner Radius / Taskbar Segments", 515, 224, 300, 24)?;
        self.label("Not available in the Rust engine yet", 515, 252, 300, 38)?;
        self.unavailable("Switch back to Default on maximized window", 201, 322, 600)?;
        self.label(
            "Gray options are not ported. Color controls below work with supported styles.",
            201,
            357,
            620,
            22,
        )?;
        self.add(
            SWATCH,
            w!("STATIC"),
            "Color preview",
            WINDOW_STYLE(SS_OWNERDRAW),
            201,
            392,
            132,
            110,
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
            let y = 389 + i as i32 * 29;
            self.label(label, 350, y + 4, 55, 22)?;
            let slider = self.add(
                SLIDER + i as u16,
                w!("msctls_trackbar32"),
                label,
                WS_TABSTOP | WINDOW_STYLE(TBS_NOTICKS),
                405,
                y,
                310,
                26,
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
                24,
            )?;
        }
        self.building_page = 1;
        self.combo(ANIMATION, "Animation Style", &ANIMATIONS, 201, 96)?;
        self.label("Animation Style OnBattery", 201, 170, 330, 24)?;
        self.label(
            "Uses the same animation - battery override not ported",
            201,
            198,
            580,
            28,
        )?;
        self.edit(DURATION, "Animation Speed (ms)", 201, 251, 180)?;
        self.button(PREVIEW, "Preview", 201, 335, 90)?;
        let preview = self.add(
            PREVIEW_TRACK,
            w!("msctls_trackbar32"),
            "Local animation preview",
            WINDOW_STYLE(TBS_NOTICKS),
            308,
            335,
            490,
            32,
        )?;
        SendMessageW(
            preview,
            TBM_SETRANGE,
            Some(WPARAM(1)),
            Some(LPARAM(1000 << 16)),
        );
        let _ = EnableWindow(preview, false);
        self.label(
            "Preview animates only this slider. It does not move your taskbar.",
            201,
            394,
            610,
            45,
        )?;
        self.label(
            "42 original easing choices - 0 ms means instant",
            201,
            450,
            610,
            24,
        )?;
        self.building_page = 2;
        self.edit(PRIMARY_OFFSET, "Primary Taskbar Offset (px)", 201, 96, 260)?;
        self.edit(
            SECONDARY_OFFSET,
            "Secondary Taskbar Offset (px)",
            201,
            170,
            260,
        )?;
        self.label("Skip Resolution", 201, 245, 260, 24)?;
        self.label(
            "Not ported - resolution-based exceptions are unavailable.",
            201,
            274,
            610,
            40,
        )?;
        self.check(BETWEEN, "Compensate Tray Clock etc...", 201, 374, 590)?;
        self.check(CENTER, "Center Taskbar", 201, 419, 590)?;
        self.label(
            "Offsets are relative to the center. Negative numbers move left / up.",
            201,
            466,
            620,
            30,
        )?;
        self.building_page = 3;
        self.label("Startup", 201, 104, 600, 30)?;
        self.label("The original Configurator used Windows Task Scheduler.\nThis Rust version does not yet provide startup registration.",201,151,600,75)?;
        self.label("Opening this window, saving or applying settings will never create\na scheduled task or silently enable automatic startup.",201,268,610,75)?;
        self.building_page = 4;
        self.check(PRIMARY, "Primary taskbar", 201, 102, 600)?;
        self.check(SECONDARY, "Secondary taskbars", 201, 147, 600)?;
        self.label(
            "Choose which taskbars receive centering and styling.",
            201,
            199,
            600,
            45,
        )?;
        self.unavailable("Hide primary Start button (not ported)", 201, 270, 610)?;
        self.unavailable("Hide secondary Start button (not ported)", 201, 310, 610)?;
        self.unavailable("Hide tray area (not ported)", 201, 350, 610)?;
        self.unavailable(
            "Pause on fullscreen / maximized windows (not ported)",
            201,
            390,
            610,
        )?;
        self.building_page = 5;
        self.edit(POLL, "Refresh Rate (ms)", 201, 96, 260)?;
        self.button(DIAGNOSE, "Read-only diagnostics", 490, 117, 310)?;
        self.label("Engine", 201, 164, 100, 22)?;
        self.add(
            401,
            w!("EDIT"),
            &options.engine.display().to_string(),
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_READONLY as u32 | ES_AUTOHSCROLL as u32),
            201,
            188,
            600,
            25,
        )?;
        self.label("Settings file", 201, 225, 200, 22)?;
        self.add(
            402,
            w!("EDIT"),
            &options
                .config
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "%APPDATA%\\TaskbarX-Rust\\settings.json".into()),
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_READONLY as u32 | ES_AUTOHSCROLL as u32),
            201,
            249,
            600,
            25,
        )?;
        self.label("Status / diagnostics", 201, 288, 600, 22)?;
        let output = self.add(
            OUTPUT,
            w!("EDIT"),
            "Loading settings",
            WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32),
            201,
            314,
            600,
            180,
        )?;
        SendMessageW(output, EM_SETLIMITTEXT, Some(WPARAM(512 * 1024)), None);
        self.building_page = 6;
        self.label("Custom Start Button", 201, 104, 600, 30)?;
        self.label("The original Start-button shortcut workflow is not ported.\nThis configurator will not hide or replace your Start button.",201,160,600,85)?;
        self.label(
            "Your Windows Start menu continues to work normally.",
            201,
            295,
            600,
            45,
        )?;
        self.building_page = 7;
        self.label("TaskbarX", 201, 105, 600, 35)?;
        self.label(
            "Rust rewrite - original interface by Chris Andriessen",
            201,
            153,
            600,
            30,
        )?;
        self.label("An independent rewrite of ChrisAnd1998/TaskbarX.\nNative Rust / Win32 - no .NET or browser runtime.\n\nDesigned for the classic Windows 10 taskbar.\nThe modern Windows 11 taskbar is unsupported.",201,208,600,145)?;
        self.button(ABOUT, "MIT license / attribution", 201, 389, 280)?;
        self.label("Closing the configurator leaves the centering engine running.\nUse Stop TaskbarX to restore and stop it.",201,445,600,50)?;
        self.building_page = 8;
        self.label("Remove TaskbarX Rust", 201, 104, 600, 35)?;
        self.label("1. Click Stop TaskbarX at the bottom left.\n2. Close this configurator.\n3. Delete the portable application folder when you no longer need it.\n\nSettings are stored in %APPDATA%\\TaskbarX-Rust.\nNo startup task or registry startup entry is created by this app.",201,165,610,200)?;
        self.label(
            "This page never deletes files automatically.",
            201,
            416,
            600,
            35,
        )?;
        self.building_page = -1;
        self.show_page(0);
        Ok(())
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
        let _ = RedrawWindow(
            Some(self.hwnd),
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
        );
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
            let height = if item.id == ANIMATION || item.id == STYLE {
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
        Form {
            center: checked(CENTER),
            primary: checked(PRIMARY),
            secondary: checked(SECONDARY),
            center_between: checked(BETWEEN),
            primary_offset: self.text(PRIMARY_OFFSET),
            secondary_offset: self.text(SECONDARY_OFFSET),
            animation: self.selected(ANIMATION, &ANIMATIONS),
            animation_ms: self.text(DURATION),
            poll_ms: self.text(POLL),
            style: self.selected(STYLE, &STYLES),
            color: [
                self.text(RED),
                self.text(GREEN),
                self.text(BLUE),
                self.text(ALPHA),
            ],
        }
    }
    unsafe fn fill(&mut self, form: &Form) {
        self.populating = true;
        for (id, value) in [
            (CENTER, form.center),
            (PRIMARY, form.primary),
            (SECONDARY, form.secondary),
            (BETWEEN, form.center_between),
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
            (RED, &form.color[0]),
            (GREEN, &form.color[1]),
            (BLUE, &form.color[2]),
            (ALPHA, &form.color[3]),
        ] {
            self.set_text(id, text);
        }
        for (id, value, choices) in [
            (ANIMATION, form.animation.as_str(), ANIMATIONS.as_slice()),
            (STYLE, form.style.as_str(), STYLES.as_slice()),
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
        self.sync_style();
        self.sync_sliders();
        self.has_form = true;
        self.populating = false;
        let _ = InvalidateRect(Some(self.control(SWATCH)), None, true);
    }
    unsafe fn availability(&self) {
        for item in &self.items {
            if (RADIO..RADIO + 5).contains(&item.id)
                || (SLIDER..SLIDER + 4).contains(&item.id)
                || item.id == PREVIEW
            {
                let _ = EnableWindow(item.hwnd, !self.busy && self.has_form);
            }
            if (CENTER..=ALPHA).contains(&item.id) || (SAVE..=ABOUT).contains(&item.id) {
                let needs_form =
                    (CENTER..=ALPHA).contains(&item.id) || item.id == SAVE || item.id == APPLY;
                let _ = EnableWindow(item.hwnd, !self.busy && (!needs_form || self.has_form));
            }
        }
    }
    unsafe fn start(&mut self, action: Action) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.availability();
        self.set_text(OUTPUT, "Working… CLI requests run off the UI thread with deadlines. No other action will run concurrently.");
        if self.tx.send(action).is_err() {
            self.busy = false;
            self.availability();
            self.set_text(OUTPUT, "The engine-command worker is unavailable. Close and reopen the configurator. Runtime state is unknown.");
        }
    }
    unsafe fn drain(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(Event::Done(result)) => {
                    self.busy = false;
                    match *result {
                        Ok(outcome) => {
                            if let Some(form) = outcome.form {
                                self.fill(&form);
                            }
                            self.set_text(OUTPUT, &outcome.message);
                        }
                        Err(error) => {
                            self.set_text(OUTPUT, &format!("ERROR — {error}\n\nIf loading failed, fix the file/engine and Reload, or explicitly Reset form to defaults. Nothing was silently replaced."));
                            self.show_page(5);
                            let _ = SetFocus(Some(self.control(OUTPUT)));
                            self.ensure_focus_visible();
                        }
                    }
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
                            "ERROR — command worker disconnected; operation outcome is unknown.",
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
                let form = self.read_form();
                if let Err(error) = form.settings() {
                    self.set_text(
                        OUTPUT,
                        &format!("Validation error — {error}. Nothing was saved or applied."),
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
                self.start(if id == SAVE {
                    Action::Save(form)
                } else {
                    Action::Apply(form)
                });
            }
            STOP => self.start(Action::Stop),
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
        for i in 0..5 {
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
    unsafe fn preview_tick(&mut self) {
        if let Some(start) = self.preview {
            let form = self.read_form();
            let duration = form.animation_ms.parse::<u32>().unwrap_or(300);
            let elapsed = start.elapsed().as_millis() as u64;
            if let Ok(position) =
                taskbarx_core::animated_position(0, 1000, elapsed, duration, &form.animation)
            {
                SendMessageW(
                    self.control(PREVIEW_TRACK),
                    TBM_SETPOS,
                    Some(WPARAM(1)),
                    Some(LPARAM(position.clamp(0, 1000) as isize)),
                );
            }
            if elapsed >= u64::from(duration) || form.animation == "none" {
                self.preview = None;
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

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
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
                    } else if (RADIO..RADIO + 5).contains(&id) && notification == BN_CLICKED {
                        SendMessageW(
                            ui.control(STYLE),
                            CB_SETCURSEL,
                            Some(WPARAM((id - RADIO) as usize)),
                            None,
                        );
                        ui.sync_style();
                        ui.set_text(
                            OUTPUT,
                            "Style selected locally. Click Apply to update the taskbar.",
                        );
                    } else if id == PREVIEW && notification == BN_CLICKED {
                        ui.preview = Some(Instant::now());
                        ui.preview_tick();
                    } else if (SAVE..=CLOSE).contains(&id) && notification == BN_CLICKED {
                        ui.action(id);
                    } else if id == IDCANCEL.0 as u16 {
                        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    } else if (CENTER..=ALPHA).contains(&id)
                        && !ui.populating
                        && (notification == EN_CHANGE
                            || notification == CBN_SELCHANGE
                            || notification == BN_CLICKED)
                    {
                        ui.set_text(OUTPUT, "Form edited locally. Save settings only persists; Apply/start explicitly changes the runtime.");
                        ui.sync_sliders();
                        let _ = InvalidateRect(Some(ui.control(SWATCH)), None, true);
                    }
                    return LRESULT(0);
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
                        ui.set_text(
                            OUTPUT,
                            "Color edited locally. Click Apply to update the taskbar.",
                        );
                        let _ = InvalidateRect(Some(ui.control(SWATCH)), None, true);
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

unsafe fn run_window(options: Options, instance: HINSTANCE, class: PCWSTR) -> Result<(), String> {
    let (tx, requests) = mpsc::channel();
    let (events, rx) = mpsc::channel();
    let worker_options = options.clone();
    thread::Builder::new()
        .name("taskbarx-cli".into())
        .spawn(move || {
            let mut controller = Controller::new(worker_options, SystemProcess::default());
            loop {
                match requests.recv_timeout(Duration::from_millis(200)) {
                    Ok(action) => {
                        if events
                            .send(Event::Done(Box::new(controller.perform(action))))
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
    let state = Box::new(RefCell::new(Ui {
        hwnd: HWND::default(),
        page: 0,
        building_page: -1,
        preview: None,
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

    #[test]
    fn hidden_native_controls_roundtrip_tab_order_dpi_and_local_actions() {
        // No process worker is created in this test. The receiver below records
        // requests only; Apply and Stop are NEVER sent to a real engine.
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(None).unwrap().0);
            let class = w!("TaskbarX.Configurator.ControlTest");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: class,
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&wc), 0);
            let (tx, requests) = mpsc::channel();
            let (_events, rx) = mpsc::channel();
            let state = Box::new(RefCell::new(Ui {
                hwnd: HWND::default(),
                page: 0,
                building_page: -1,
                preview: None,
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
                class,
                w!("Hidden control test"),
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
            {
                let mut ui = state.borrow_mut();
                ui.hwnd = hwnd;
                let options = Options {
                    engine: std::env::temp_dir().join("never-executed-test-engine.exe"),
                    config: None,
                };
                ui.build(&options).unwrap();
                // Check every page, including hidden controls, for encoding damage.
                for item in &ui.items {
                    let mut text = vec![0u16; GetWindowTextLengthW(item.hwnd) as usize + 1];
                    let len = GetWindowTextW(item.hwnd, &mut text);
                    let text = String::from_utf16_lossy(&text[..len as usize]);
                    assert!(
                        !text.contains('?') && !text.contains('\u{fffd}'),
                        "corrupted UI text: {text}"
                    );
                }

                assert!(ui.read_form().settings().is_err());
                for animation in ANIMATIONS {
                    for style in STYLES {
                        let form = Form {
                            animation: animation.into(),
                            style: style.into(),
                            primary_offset: "-134".into(),
                            secondary_offset: "-99".into(),
                            color: ["255".into(), "0".into(), "254".into(), "0".into()],
                            ..Form::default()
                        };
                        ui.fill(&form);
                        assert_eq!(ui.read_form(), form);
                    }
                }
                ui.availability();
                for dpi in [96, 144, 192] {
                    ui.dpi = dpi;
                    ui.replace_font().unwrap();
                    ui.layout();
                    let mut rect = RECT::default();
                    GetWindowRect(ui.control(RED), &mut rect).unwrap();
                    assert_eq!(rect.right - rect.left, px(70, dpi));
                }
                for page in 0..9 {
                    ui.show_page(page);
                    assert_eq!(ui.text(TITLE), PAGES[page as usize]);
                    for item in &ui.items {
                        let visible =
                            GetWindowLongW(item.hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0;
                        assert_eq!(
                            visible,
                            item.page == -1 || item.page == page,
                            "page {page}, control {}",
                            item.id
                        );
                    }
                    // Switching tabs is entirely local, never issues a command.
                    assert!(requests.try_recv().is_err());
                }
                ui.show_page(0);
                ui.sync_sliders();
                assert_eq!(
                    SendMessageW(ui.control(SLIDER), TBM_GETPOS, None, None).0,
                    255
                );
                ui.preview = Some(Instant::now());
                ui.preview_tick();
                assert!(requests.try_recv().is_err());
                // The native Save button submits only Save; core contract tests
                // then verify that this emits only config import --stdin.
                ui.action(SAVE);
                assert!(matches!(requests.try_recv().unwrap(), Action::Save(_)));
                ui.busy = false;
                ui.action(RESET);
                assert!(matches!(requests.try_recv().unwrap(), Action::Reset));
                ui.busy = false;
                ui.set_text(PRIMARY_OFFSET, "-");
                ui.action(APPLY);
                assert!(ui.alert.as_ref().unwrap().contains("primary_offset"));
                assert!(requests.try_recv().is_err());
                assert!(!ui.busy);
            }
            DestroyWindow(hwnd).unwrap();
            drop(state);
            assert!(requests.try_recv().is_err());
            UnregisterClassW(class, Some(instance)).unwrap();
        }
    }
}
