//! A conventional Win32 control tree. The window borrows a stable Box<RefCell<Ui>>
//! owned by run(); WM_NCDESTROY clears that pointer, never frees it. Reentrant
//! control notifications use try_borrow_mut, not aliasing &mut references.
//! Worker threads never hold an HWND or a pointer into window state.
use std::{
    cell::RefCell,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::Duration,
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
const CONTENT_W: i32 = 780;
const CONTENT_H: i32 = 842;
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
            Some(LPARAM(1)),
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
    unsafe fn build(&mut self, options: &Options) -> Result<(), String> {
        self.replace_font()?;
        self.label(
            "TaskbarX — Windows 10 classic taskbar controls",
            20,
            12,
            740,
            24,
        )?;
        self.label("Native Windows 11 taskbars have limited/unsupported behavior. Read diagnostics before applying.", 20, 38, 740, 38)?;
        self.label("Engine:", 20, 79, 70, 22)?;
        self.add(
            401,
            w!("EDIT"),
            &options.engine.display().to_string(),
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_READONLY as u32 | ES_AUTOHSCROLL as u32),
            94,
            76,
            666,
            26,
        )?;
        self.label("Config:", 20, 111, 70, 22)?;
        self.add(
            402,
            w!("EDIT"),
            &options
                .config
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Engine default location (no --config override)".into()),
            WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_READONLY as u32 | ES_AUTOHSCROLL as u32),
            94,
            108,
            666,
            26,
        )?;
        self.add(
            CENTER,
            w!("BUTTON"),
            "&Center taskbar icons",
            WS_TABSTOP | WS_GROUP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20,
            146,
            360,
            26,
        )?;
        self.label(
            "Selection affects styling AND centering.",
            400,
            148,
            360,
            40,
        )?;
        self.add(
            PRIMARY,
            w!("BUTTON"),
            "&Primary taskbar",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20,
            177,
            360,
            26,
        )?;
        self.add(
            SECONDARY,
            w!("BUTTON"),
            "Seco&ndary taskbars",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            400,
            177,
            360,
            26,
        )?;
        self.add(
            BETWEEN,
            w!("BUTTON"),
            "Center &between Start/search and the tray",
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            20,
            208,
            740,
            26,
        )?;
        self.edit(
            PRIMARY_OFFSET,
            "Primary &offset (physical px, -10000 to 10000)",
            20,
            244,
            360,
        )?;
        self.edit(
            SECONDARY_OFFSET,
            "Secondary offset (physical p&x, -10000 to 10000)",
            400,
            244,
            360,
        )?;
        self.combo(ANIMATION, "&Easing (42 choices)", &ANIMATIONS, 20, 302)?;
        self.edit(
            DURATION,
            "Animation &duration (0–10000 ms; 0 = instant)",
            400,
            302,
            360,
        )?;
        self.edit(POLL, "Refresh &interval (50–10000 ms)", 20, 360, 360)?;
        self.combo(STYLE, "Taskbar st&yle", &STYLES, 400, 360)?;
        self.label(
            "RGBA color — each component 0–255. Preview is local; it never changes the taskbar.",
            20,
            422,
            740,
            24,
        )?;
        self.edit(RED, "Red", 20, 452, 100)?;
        self.edit(GREEN, "Green", 140, 452, 100)?;
        self.edit(BLUE, "Blue", 260, 452, 100)?;
        self.edit(ALPHA, "Alpha", 380, 452, 100)?;
        self.add(
            SWATCH,
            w!("STATIC"),
            "Local color swatch",
            WINDOW_STYLE(SS_OWNERDRAW),
            520,
            452,
            240,
            50,
        )?;
        self.label("Alpha over checkerboard (preview only)", 520, 506, 240, 38)?;
        for (id, text, x, y, width) in [
            (SAVE, "&Save settings", 20, 550, 170),
            (APPLY, "&Apply / start", 200, 550, 170),
            (STOP, "S&top and restore", 380, 550, 180),
            (RELOAD, "&Reload", 570, 550, 190),
            (RESET, "Reset &form to defaults", 20, 590, 230),
            (DIAGNOSE, "Read-only dia&gnostics", 260, 590, 230),
            (ABOUT, "Attribution", 500, 590, 130),
            (CLOSE, "Close", 640, 590, 120),
        ] {
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
        }
        self.label(
            "Status / engine output (select text and Ctrl+C to copy):",
            20,
            634,
            740,
            22,
        )?;
        let output = self.add(
            OUTPUT,
            w!("EDIT"),
            "Loading settings through config show. No taskbar changes.",
            WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | WINDOW_STYLE(ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32),
            20,
            660,
            740,
            120,
        )?;
        SendMessageW(output, EM_SETLIMITTEXT, Some(WPARAM(512 * 1024)), None);
        self.label("Closing this configurator does NOT stop a separately running engine.\nNothing registers startup or creates scheduled tasks.", 20, 790, 740, 44)?;
        self.layout();
        Ok(())
    }

    unsafe fn replace_font(&mut self) -> Result<(), String> {
        let font = CreateFontW(
            -px(16, self.dpi),
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
                Some(LPARAM(1)),
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
            let _ = MoveWindow(
                item.hwnd,
                px(item.x, self.dpi) - self.scroll_x,
                px(item.y, self.dpi) - self.scroll_y,
                px(item.w, self.dpi),
                px(item.h, self.dpi),
                true,
            );
        }
        let _ = InvalidateRect(Some(self.hwnd), None, true);
    }

    unsafe fn ensure_focus_visible(&mut self) {
        let focus = GetFocus();
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
        self.has_form = true;
        self.populating = false;
        let _ = InvalidateRect(Some(self.control(SWATCH)), None, true);
    }
    unsafe fn availability(&self) {
        for item in &self.items {
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
                    if (SAVE..=CLOSE).contains(&id) && notification == BN_CLICKED {
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
                        let _ = InvalidateRect(Some(ui.control(SWATCH)), None, true);
                    }
                    return LRESULT(0);
                }
                WM_TIMER => {
                    ui.drain();
                    return LRESULT(0);
                }
                WM_SIZE => {
                    ui.layout();
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
    let hwnd = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class,
        w!("TaskbarX Configurator"),
        WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VSCROLL | WS_HSCROLL,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        px(840, dpi).min(work.right - work.left),
        px(900, dpi).min(work.bottom - work.top),
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
        if SetTimer(Some(hwnd), 1, 100, None) == 0 {
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
    let _ = SetFocus(Some(state.borrow().control(401)));
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
                    assert_eq!(rect.right - rect.left, px(100, dpi));
                }
                let order = [
                    401,
                    402,
                    CENTER,
                    PRIMARY,
                    SECONDARY,
                    BETWEEN,
                    PRIMARY_OFFSET,
                    SECONDARY_OFFSET,
                    ANIMATION,
                    DURATION,
                    POLL,
                    STYLE,
                    RED,
                    GREEN,
                    BLUE,
                    ALPHA,
                    SAVE,
                    APPLY,
                    STOP,
                    RELOAD,
                    RESET,
                    DIAGNOSE,
                    ABOUT,
                    CLOSE,
                    OUTPUT,
                ];
                for pair in order.windows(2) {
                    assert_eq!(
                        GetNextDlgTabItem(hwnd, Some(ui.control(pair[0])), false).unwrap(),
                        ui.control(pair[1])
                    );
                    assert_eq!(
                        GetNextDlgTabItem(hwnd, Some(ui.control(pair[1])), true).unwrap(),
                        ui.control(pair[0])
                    );
                }
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
