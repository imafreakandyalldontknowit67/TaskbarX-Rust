//! Windows 10 classic taskbar access. All rectangles are physical screen pixels;
//! positions are immediate-parent client coordinates. No background loops run.
//!
//! A session is thread-bound (COM apartment ownership) and restores only its own
//! successful mutations. Discovery and dropping a read-only session never alter
//! the shell. Native Windows 11 taskbars are deliberately unsupported.
//! Run discovery on a dedicated thread: traversal has depth/node/time budgets,
//! but an individual out-of-process MSAA call has no per-call timeout.
//!
//! Accent APIs are undocumented and dynamically resolved. If the original accent
//! cannot be queried, restoration disables our accent and sends WM_THEMECHANGED
//! once, with a 200 ms timeout. This requests Windows-managed appearance, **not**
//! exact restoration of unqueryable third-party styling. Restoration logs that
//! limitation as a non-fatal warning. Callers should call `restore` explicitly to inspect
//! failures; Drop can only attempt best-effort cleanup.
//! No Win32 operation can atomically validate and pin a foreign HWND; identities
//! are revalidated immediately before each mutation to minimize shell races.
//!
//! Discovery/topology and accent-policy adaptations are based on TaskbarX by
//! Chris Andriessen. The original MIT notice is included below and in the source.
#![doc = include_str!("upstream-license.txt")]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskbarId {
    pub root: isize,
    pub task_list: isize,
    pub parent: isize,
    pub process_id: u32,
    /// Explorer creation FILETIME (100 ns ticks since 1601), not a PID alone.
    pub process_started: u64,
}

#[derive(Debug, Clone)]
pub struct TaskbarSnapshot {
    pub id: TaskbarId,
    pub primary: bool,
    pub vertical: bool,
    pub dpi: u32,
    pub taskbar_rect: Rect,
    pub parent_rect: Rect,
    pub list_rect: Rect,
    pub available_rect: Rect,
    pub content_rect: Option<Rect>,
    pub position: Point,
}

#[derive(Debug, Clone)]
pub struct Discovery {
    pub build_number: u32,
    pub native_windows11: bool,
    pub taskbars: Vec<TaskbarSnapshot>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Default,
    Transparent,
    Blur,
    Acrylic,
    Opaque,
}

#[cfg(windows)]
mod accessibility;
#[cfg(any(windows, test))]
mod geometry;
#[cfg(windows)]
mod platform;
#[cfg(any(windows, test))]
mod state;

/// Thread-bound owner of position/style changes, not a global shell controller.
pub struct Session {
    #[cfg(windows)]
    inner: state::Controller<platform::Windows>,
    #[cfg(not(windows))]
    _private: (),
}

impl Session {
    pub fn new() -> Result<Self, String> {
        #[cfg(windows)]
        {
            Ok(Self {
                inner: state::Controller::new(platform::Windows::new()?),
            })
        }
        #[cfg(not(windows))]
        {
            Err(unsupported())
        }
    }

    pub fn discover(&mut self) -> Result<Discovery, String> {
        #[cfg(windows)]
        {
            self.inner.discover()
        }
        #[cfg(not(windows))]
        {
            Err(unsupported())
        }
    }

    /// Move along the taskbar's major axis only. The other component is ignored;
    /// the current cross-axis coordinate, window size and styles are preserved.
    /// Refuses uncertain measurements or a destination that clips task buttons.
    pub fn move_task_list(&mut self, id: &TaskbarId, position: Point) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.inner.move_task_list(id, position)
        }
        #[cfg(not(windows))]
        {
            let _ = (id, position);
            Err(unsupported())
        }
    }

    pub fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.inner.restore_position(id)
        }
        #[cfg(not(windows))]
        {
            let _ = id;
            Err(unsupported())
        }
    }

    /// `Default` is a no-op unless this session previously changed this root.
    /// Reapplication happens only on an explicit call to this method.
    pub fn set_style(&mut self, id: &TaskbarId, style: Style, rgba: [u8; 4]) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.inner.set_style(id, style, rgba)
        }
        #[cfg(not(windows))]
        {
            let _ = (id, style, rgba);
            Err(unsupported())
        }
    }

    /// Restores independent changes even if one fails. Successfully restored
    /// entries are removed; failed entries can be retried. Repeated success is a
    /// no-op. Fallback warnings go to stderr, not the returned failure list.
    pub fn restore(&mut self) -> Vec<String> {
        #[cfg(windows)]
        {
            self.inner.restore()
        }
        #[cfg(not(windows))]
        {
            vec![unsupported()]
        }
    }
}

#[cfg(not(windows))]
fn unsupported() -> String {
    "taskbarx-win32 is unsupported on this platform; Windows 10 classic taskbars are required"
        .into()
}
