//! Native feature primitives. Every mutation revalidates root/child identity;
//! all baselines and retry decisions are held by effects::Journal.
use super::*;
use windows::Win32::{
    Foundation::{GetLastError, SetLastError, COLORREF, ERROR_SUCCESS},
    Graphics::Gdi::*,
};

fn word(w: HWND, index: WINDOW_LONG_PTR_INDEX) -> Result<u32, String> {
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let bits = GetWindowLongW(w, index);
        if bits == 0 && GetLastError() != ERROR_SUCCESS {
            return Err(last_error("GetWindowLongW"));
        }
        Ok(bits as u32)
    }
}
fn bits(w: HWND, index: WINDOW_LONG_PTR_INDEX, mask: u32, value: u32) -> Result<(), String> {
    let current = word(w, index)?;
    let desired = (current & !mask) | (value & mask);
    if desired == current {
        return Ok(());
    }
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let previous = SetWindowLongW(w, index, desired as i32);
        if previous == 0 && GetLastError() != ERROR_SUCCESS {
            return Err(last_error("SetWindowLongW(owned bits)"));
        }
    }
    if word(w, index)? & mask != value & mask {
        return Err("Owned style bits did not stick".into());
    }
    Ok(())
}
fn child_identity(id: &TaskbarId, window: HWND) -> Result<ChildIdentity, String> {
    if !unsafe { IsWindow(Some(window)) }.as_bool()
        || pid(window)? != id.process_id
        || process_started(id.process_id)? != id.process_started
    {
        return Err("Control process identity changed".into());
    }
    let mut ancestry = vec![];
    let mut current = window;
    for _ in 0..24 {
        current = parent(current)?;
        if pid(current)? != id.process_id {
            return Err("Control ancestry crosses processes".into());
        }
        ancestry.push((current.0 as isize, class(current)?));
        if current == hwnd(id.root) {
            return Ok(ChildIdentity {
                handle: window.0 as isize,
                class: class(window)?,
                ancestry,
                process_id: id.process_id,
                process_started: id.process_started,
            });
        }
    }
    Err("Control has no validated taskbar root ancestry".into())
}
fn control_position(c: &ChildIdentity) -> Result<Point, String> {
    let r = window_rect(hwnd(c.handle))?;
    let mut p = POINT {
        x: r.left,
        y: r.top,
    };
    if !unsafe {
        ScreenToClient(
            hwnd(c.ancestry.first().ok_or("Missing control parent")?.0),
            &mut p,
        )
    }
    .as_bool()
    {
        return Err(last_error("ScreenToClient(control)"));
    }
    Ok(Point { x: p.x, y: p.y })
}
impl Windows {
    fn check_child(&mut self, id: &TaskbarId, c: &ChildIdentity) -> Result<(), String> {
        self.validate_root(id)?;
        if child_identity(id, hwnd(c.handle))? != *c {
            return Err("Stale control handle/class/ancestry/process identity".into());
        }
        Ok(())
    }
    pub(super) fn native_layout(&mut self, id: &TaskbarId) -> Result<Layout, String> {
        let _dpi = self.dpi.enter()?;
        let mut warnings = vec![];
        let snapshot = measure(id, self.dpi, self.build, self.uia.as_ref(), &mut warnings)?;
        let root = hwnd(id.root);
        let client = client_rect(root)?;
        let (list, _, chain, primary) = topology(root)?;
        let mut controls = vec![];
        for w in enumerate(Some(root))? {
            if w == list || chain.contains(&w) || unsafe { IsChild(list, w) }.as_bool() {
                continue;
            }
            let p = parent(w)?;
            // Reserve entire non-task-list branches. Their descendants cannot
            // independently contribute outside their parent's clipping region.
            if !chain.contains(&p) {
                continue;
            }
            let identity = child_identity(id, w)?;
            let visible = unsafe { IsWindowVisible(w) }.as_bool();
            let r = window_rect(w)?;
            if !r.valid() && visible {
                return Err("Visible reserved control has empty bounds".into());
            }
            let kind = match identity.class.as_str() {
                "Start" if p == root => ControlKind::Start,
                "TrayNotifyWnd" if p == root => ControlKind::Tray,
                "ClockButton" | "ClockButtonWindow"
                    if !primary && (p == root || p == hwnd(id.parent)) =>
                {
                    ControlKind::Tray
                }
                "ToolbarWindow32" if class(p)? == "ReBarWindow32" => ControlKind::Toolbar,
                _ => ControlKind::Reserved,
            };
            controls.push(ControlGeometry {
                position: control_position(&identity)?,
                parent_clip: client_rect(p)?,
                identity,
                kind,
                rect: r,
                visible,
            });
        }
        validate(id, self.build)?;
        if client_rect(root)? != client {
            return Err("Root client changed during feature measurement".into());
        }
        for c in &controls {
            self.check_child(id, &c.identity)?;
            if window_rect(hwnd(c.identity.handle))? != c.rect
                || unsafe { IsWindowVisible(hwnd(c.identity.handle)) }.as_bool() != c.visible
            {
                return Err("Controls changed during feature measurement; rediscover".into());
            }
        }
        Ok(Layout {
            snapshot,
            client,
            controls,
        })
    }
    pub(super) fn native_capture_opacity(
        &mut self,
        id: &TaskbarId,
    ) -> Result<OpacityOriginal, String> {
        self.validate_root(id)?;
        let layered = word(hwnd(id.root), GWL_EXSTYLE)? & LAYERED != 0;
        let attributes = if layered {
            let (mut key, mut alpha, mut flags) =
                (COLORREF(0), 0, LAYERED_WINDOW_ATTRIBUTES_FLAGS(0));
            unsafe { GetLayeredWindowAttributes(hwnd(id.root), Some(&mut key), Some(&mut alpha), Some(&mut flags)) }.map_err(|e| error("Pre-existing layered state cannot be captured (per-pixel UpdateLayeredWindow is unsupported); opacity left untouched", e))?;
            if flags.0 == 0 || flags.0 & !3 != 0 {
                return Err("Uncapturable layered attribute flags; opacity left untouched".into());
            }
            Some(LayeredAttributes {
                key: key.0,
                alpha,
                flags: flags.0,
            })
        } else {
            None
        };
        self.validate_root(id)?;
        Ok(OpacityOriginal {
            layered,
            attributes,
        })
    }
    pub(super) fn native_layered_bit(
        &mut self,
        id: &TaskbarId,
        enabled: bool,
    ) -> Result<(), String> {
        self.validate_root(id)?;
        bits(
            hwnd(id.root),
            GWL_EXSTYLE,
            LAYERED,
            if enabled { LAYERED } else { 0 },
        )
    }
    pub(super) fn native_layered(
        &mut self,
        id: &TaskbarId,
        a: LayeredAttributes,
    ) -> Result<(), String> {
        self.validate_root(id)?;
        unsafe {
            SetLayeredWindowAttributes(
                hwnd(id.root),
                COLORREF(a.key),
                a.alpha,
                LAYERED_WINDOW_ATTRIBUTES_FLAGS(a.flags),
            )
        }
        .map_err(|e| error("SetLayeredWindowAttributes", e))
    }
    pub(super) fn native_child_obsolete(&mut self, id: &TaskbarId, c: &ChildIdentity) -> bool {
        self.root_obsolete(id)
            || !unsafe { IsWindow(Some(hwnd(c.handle))) }.as_bool()
            || pid(hwnd(c.handle)).is_ok_and(|p| p != c.process_id)
            || process_started(c.process_id).is_ok_and(|p| p != c.process_started)
            || class(hwnd(c.handle)).is_ok_and(|s| s != c.class)
            || child_identity(id, hwnd(c.handle)).is_ok_and(|actual| actual != *c)
    }
    pub(super) fn native_visible(
        &mut self,
        id: &TaskbarId,
        c: &ChildIdentity,
    ) -> Result<bool, String> {
        self.check_child(id, c)?;
        Ok(word(hwnd(c.handle), GWL_STYLE)? & WS_VISIBLE.0 != 0)
    }
    pub(super) fn native_set_visible(
        &mut self,
        id: &TaskbarId,
        c: &ChildIdentity,
        visible: bool,
    ) -> Result<(), String> {
        self.check_child(id, c)?;
        if self.native_visible(id, c)? == visible {
            return Ok(());
        }
        self.check_child(id, c)?;
        unsafe {
            SetWindowPos(
                hwnd(c.handle),
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOZORDER
                    | SWP_NOACTIVATE
                    | SWP_NOOWNERZORDER
                    | if visible {
                        SWP_SHOWWINDOW
                    } else {
                        SWP_HIDEWINDOW
                    },
            )
        }
        .map_err(|e| error("SetWindowPos(visibility)", e))?;
        if self.native_visible(id, c)? != visible {
            return Err("Control visibility did not stick".into());
        }
        Ok(())
    }
    pub(super) fn native_child_position(
        &mut self,
        id: &TaskbarId,
        c: &ChildIdentity,
    ) -> Result<Point, String> {
        let _dpi = self.dpi.enter()?;
        self.check_child(id, c)?;
        control_position(c)
    }
    pub(super) fn native_move_child(
        &mut self,
        id: &TaskbarId,
        c: &ChildIdentity,
        p: Point,
        expected: Point,
    ) -> Result<(), String> {
        let _dpi = self.dpi.enter()?;
        self.check_child(id, c)?;
        if control_position(c)? != expected {
            return Err("Control moved after measurement".into());
        }
        let dx =
            p.x.checked_sub(expected.x)
                .ok_or("Control position overflow")?;
        let dy =
            p.y.checked_sub(expected.y)
                .ok_or("Control position overflow")?;
        let target = rect(hwnd(c.handle))?
            .translated(dx, dy)
            .ok_or("Control rectangle overflow")?;
        if !client_rect(hwnd(c.ancestry.first().ok_or("Missing control parent")?.0))?
            .contains(target)
        {
            return Err(
                "Control target no longer fits its parent; refusing stale relayout coordinates"
                    .into(),
            );
        }
        self.check_child(id, c)?;
        unsafe {
            SetWindowPos(
                hwnd(c.handle),
                None,
                p.x,
                p.y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_NOSENDCHANGING,
            )
        }
        .map_err(|e| error("SetWindowPos(toolbar compensation)", e))
    }
    pub(super) fn native_capture_region(
        &mut self,
        id: &TaskbarId,
    ) -> Result<Option<Vec<u8>>, String> {
        self.validate_root(id)?;
        let region = Region::new(unsafe { CreateRectRgn(0, 0, 0, 0) })?;
        unsafe {
            SetLastError(ERROR_SUCCESS);
        }
        let kind = unsafe { GetWindowRgn(hwnd(id.root), region.0) };
        if kind == RGN_ERROR {
            let err = unsafe { GetLastError() };
            self.validate_root(id)?;
            if err != ERROR_SUCCESS {
                return Err(format!("GetWindowRgn capture failed: {err:?}"));
            }
            // Documented ERROR also denotes a window without an assigned region.
            return Ok(None);
        }
        let n = unsafe { GetRegionData(region.0, 0, None) };
        if n == 0 || n > 4 * 1024 * 1024 {
            return Err("Region snapshot size invalid/excessive".into());
        }
        let mut aligned = vec![0u32; (n as usize).div_ceil(4)];
        if unsafe { GetRegionData(region.0, n, Some(aligned.as_mut_ptr().cast())) } != n {
            return Err(last_error("GetRegionData"));
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(aligned.as_ptr().cast::<u8>(), n as usize) }
                .to_vec();
        self.validate_root(id)?;
        Ok(Some(bytes))
    }
    pub(super) fn native_region(
        &mut self,
        id: &TaskbarId,
        parts: &[RegionPart],
        client: Rect,
    ) -> Result<(), String> {
        let _dpi = self.dpi.enter()?;
        self.validate(id)?;
        if parts.is_empty() || client_rect(hwnd(id.root))? != client {
            return Err("Region layout changed/empty; rediscover".into());
        }
        if word(hwnd(id.root), GWL_EXSTYLE)? & WS_EX_LAYOUTRTL.0 != 0 {
            return Err("Mirrored taskbar regions unsupported; original region retained".into());
        }
        let root = rect(hwnd(id.root))?;
        let dx = client
            .left
            .checked_sub(root.left)
            .ok_or("Region offset overflow")?;
        let dy = client
            .top
            .checked_sub(root.top)
            .ok_or("Region offset overflow")?;
        let region = Region::new(unsafe { CreateRectRgn(0, 0, 0, 0) })?;
        for part in parts {
            let r = part
                .rect
                .translated(dx, dy)
                .ok_or("Region offset overflow")?;
            let diameter = i32::try_from(part.radius.saturating_mul(2))
                .map_err(|_| "Region radius overflow")?;
            let piece = Region::new(unsafe {
                if diameter == 0 {
                    CreateRectRgn(r.left, r.top, r.right, r.bottom)
                } else {
                    CreateRoundRectRgn(r.left, r.top, r.right, r.bottom, diameter, diameter)
                }
            })?;
            if unsafe { CombineRgn(Some(region.0), Some(region.0), Some(piece.0), RGN_OR) }
                == RGN_ERROR
            {
                return Err(last_error("CombineRgn"));
            }
        }
        self.validate(id)?;
        region.transfer(hwnd(id.root))
    }
    pub(super) fn native_restore_region(
        &mut self,
        id: &TaskbarId,
        bytes: Option<&[u8]>,
    ) -> Result<(), String> {
        self.validate_root(id)?;
        if let Some(bytes) = bytes {
            if bytes.len() < size_of::<RGNDATAHEADER>() || bytes.len() > 4 * 1024 * 1024 {
                return Err("Invalid region baseline".into());
            }
            let mut aligned = vec![0u32; bytes.len().div_ceil(4)];
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    aligned.as_mut_ptr().cast(),
                    bytes.len(),
                );
            }
            let region = Region::new(unsafe {
                ExtCreateRegion(None, bytes.len() as u32, aligned.as_ptr().cast())
            })?;
            self.validate_root(id)?;
            region.transfer(hwnd(id.root))
        } else {
            self.validate_root(id)?;
            if unsafe { SetWindowRgn(hwnd(id.root), None, true) } == 0 {
                return Err(last_error("SetWindowRgn(restore none)"));
            }
            Ok(())
        }
    }
}
struct Region(HRGN, fn(HRGN));
fn delete_region(h: HRGN) {
    unsafe {
        let _ = DeleteObject(h.into());
    }
}
impl Region {
    fn new(h: HRGN) -> Result<Self, String> {
        if h.0.is_null() {
            Err(last_error("Create region"))
        } else {
            Ok(Self(h, delete_region))
        }
    }
    fn transfer(self, w: HWND) -> Result<(), String> {
        self.transfer_via(|h| unsafe { SetWindowRgn(w, Some(h), true) } != 0)
    }
    fn transfer_via(self, setter: impl FnOnce(HRGN) -> bool) -> Result<(), String> {
        if !setter(self.0) {
            return Err(last_error("SetWindowRgn"));
        }
        // Only success transfers ownership. Failed calls and all temporary union
        // pieces are deleted by Drop; snapshots never alias transferred handles.
        std::mem::forget(self);
        Ok(())
    }
}
impl Drop for Region {
    fn drop(&mut self) {
        (self.1)(self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    // Fake GDI ownership: no OS calls and no fake handle reaches DeleteObject.
    thread_local! { static DELETED: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) }; }
    fn dispose(h: HRGN) {
        DELETED.with(|d| d.borrow_mut().push(h.0 as isize));
    }
    fn region(n: isize) -> Region {
        Region(HRGN(n as *mut _), dispose)
    }
    #[test]
    fn failed_transfer_deletes_and_success_relinquishes_exactly_once() {
        DELETED.with(|d| d.borrow_mut().clear());
        assert!(region(1).transfer_via(|_| false).is_err());
        region(2)
            .transfer_via(|h| {
                assert_eq!(h.0 as isize, 2);
                true
            })
            .unwrap();
        DELETED.with(|d| assert_eq!(*d.borrow(), vec![1]));
        // The receiver, not our guard, releases the successful transfer.
        dispose(HRGN(2 as *mut _));
        DELETED.with(|d| assert_eq!(*d.borrow(), vec![1, 2]));
    }
    #[test]
    fn temporary_union_pieces_and_failed_union_are_released() {
        DELETED.with(|d| d.borrow_mut().clear());
        let union = region(3);
        {
            let _piece = region(4);
        }
        assert!(union.transfer_via(|_| false).is_err());
        DELETED.with(|d| assert_eq!(*d.borrow(), vec![4, 3]));
    }
}
