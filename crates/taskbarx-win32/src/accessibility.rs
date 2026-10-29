//! Bounded MSAA walk. Unlike the upstream's IAccessible[] assumption, children
//! can be VT_I4 IDs or VT_DISPATCH objects. Never infer a button's extent from the
//! task-list window or the number/index of children.
use crate::{
    geometry::{is_button, is_hidden},
    Rect,
};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use windows::{
    core::{IUnknown, Interface, HRESULT},
    Win32::{
        Foundation::HWND,
        System::{
            Com::IDispatch,
            Variant::{VariantClear, VARIANT, VT_DISPATCH, VT_I4},
        },
        UI::Accessibility::{AccessibleChildren, AccessibleObjectFromWindow, IAccessible},
    },
};

const MAX_NODES: usize = 2048;
const MAX_DEPTH: usize = 24;
const MAX_ELAPSED: Duration = Duration::from_millis(500);

// windows 0.61's Win32 VARIANT is a raw struct, NOT an owning windows-core
// variant. Its generated Clone is a shallow copy. Do not clone it or rely on
// Rust to release embedded IDispatch/BSTR values without VariantClear.
struct OwnedVariant(VARIANT);
impl Drop for OwnedVariant {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.0);
        }
    }
}
struct Children(Vec<VARIANT>);
impl Drop for Children {
    fn drop(&mut self) {
        for v in &mut self.0 {
            unsafe {
                let _ = VariantClear(v);
            }
        }
    }
}
fn integer(v: &VARIANT) -> Result<i32, String> {
    unsafe {
        let data = &v.Anonymous.Anonymous;
        if data.vt != VT_I4 {
            return Err("MSAA returned a non-integer role/state/child ID".into());
        }
        Ok(data.Anonymous.lVal)
    }
}
fn child_id(id: i32) -> OwnedVariant {
    let mut v = VARIANT::default();
    unsafe {
        let data = &mut *v.Anonymous.Anonymous;
        data.vt = VT_I4;
        data.Anonymous.lVal = id;
    }
    OwnedVariant(v)
}
fn dispatch(v: &VARIANT) -> Result<IAccessible, String> {
    unsafe {
        // Borrow the interface while its VARIANT owns it; QueryInterface returns
        // a separately owned reference. VariantClear releases the original.
        v.Anonymous
            .Anonymous
            .Anonymous
            .pdispVal
            .as_ref()
            .ok_or("MSAA returned a null dispatch child")?
            .cast::<IAccessible>()
            .map_err(|e| format!("MSAA dispatch child: {e}"))
    }
}

/// The generated get_accChild wrapper turns S_FALSE/null into an error. Using
/// the vtable here preserves the difference between a simple integer child and
/// an actual failed provider call. Release even a non-null failure output.
fn object_child(parent: &IAccessible, id: &VARIANT) -> Result<Option<IAccessible>, String> {
    unsafe {
        let mut raw = std::ptr::null_mut();
        let hr =
            (parent.vtable().get_accChild)(parent.as_raw(), std::mem::transmute_copy(id), &mut raw);
        let object = if raw.is_null() {
            None
        } else {
            Some(IDispatch::from_raw(raw))
        };
        hr.ok().map_err(|e| format!("MSAA get_accChild: {e}"))?;
        if hr.0 == 0 && object.is_none() {
            return Err("MSAA get_accChild reported success with a null object".into());
        }
        object
            .map(|d| {
                d.cast::<IAccessible>()
                    .map_err(|e| format!("MSAA child QueryInterface: {e}"))
            })
            .transpose()
    }
}

#[derive(Clone)]
enum Child<O> {
    Integer(i32),
    Object(O),
}

// Narrow provider seam: the actual traversal/filtering is exercised with a fake
// tree below, including mixed ID/dispatch children and incomplete providers.
trait Provider {
    type Object;
    fn identity(&mut self, object: &Self::Object) -> Result<usize, String>;
    fn state(&mut self, object: &Self::Object, child: i32) -> Result<i32, String>;
    fn role(&mut self, object: &Self::Object, child: i32) -> Result<i32, String>;
    fn location(&mut self, object: &Self::Object, child: i32) -> Result<Rect, String>;
    fn children(
        &mut self,
        object: &Self::Object,
        limit: usize,
    ) -> Result<Vec<Child<Self::Object>>, String>;
    fn object_child(
        &mut self,
        object: &Self::Object,
        child: i32,
    ) -> Result<Option<Self::Object>, String>;
}

struct Native {
    // Keep canonical IUnknown references alive so identity pointers cannot be
    // recycled while a traversal is still in progress.
    identities: Vec<IUnknown>,
}

/// Own the output before invoking the vtable so VariantClear also runs when a
/// failing provider writes a BSTR/interface into its out parameter.
fn property(acc: &IAccessible, child: i32, role: bool) -> Result<i32, String> {
    let id = child_id(child);
    let mut output = OwnedVariant(VARIANT::default());
    let hr: HRESULT = unsafe {
        let method = if role {
            acc.vtable().get_accRole
        } else {
            acc.vtable().get_accState
        };
        method(acc.as_raw(), std::mem::transmute_copy(&id.0), &mut output.0)
    };
    hr.ok().map_err(|e| format!("MSAA role/state: {e}"))?;
    integer(&output.0)
}

impl Provider for Native {
    type Object = IAccessible;
    fn identity(&mut self, acc: &IAccessible) -> Result<usize, String> {
        let identity = acc.cast::<IUnknown>().map_err(|e| e.to_string())?;
        let key = identity.as_raw() as usize;
        self.identities.push(identity);
        Ok(key)
    }
    fn state(&mut self, acc: &IAccessible, child: i32) -> Result<i32, String> {
        property(acc, child, false)
    }
    fn role(&mut self, acc: &IAccessible, child: i32) -> Result<i32, String> {
        property(acc, child, true)
    }
    fn location(&mut self, acc: &IAccessible, child: i32) -> Result<Rect, String> {
        let id = child_id(child);
        let (mut x, mut y, mut w, mut h) = (0, 0, 0, 0);
        unsafe { acc.accLocation(&mut x, &mut y, &mut w, &mut h, &id.0) }
            .map_err(|e| format!("MSAA button location: {e}"))?;
        Rect::from_xywh(x, y, w, h)
            .ok_or_else(|| "MSAA button has invalid/overflowing geometry".into())
    }
    fn children(
        &mut self,
        acc: &IAccessible,
        limit: usize,
    ) -> Result<Vec<Child<IAccessible>>, String> {
        let count = unsafe { acc.accChildCount() }.map_err(|e| format!("MSAA child count: {e}"))?;
        if count < 0 || count as usize > limit {
            return Err("MSAA child count exceeds traversal limit".into());
        }
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut children = Children((0..count).map(|_| VARIANT::default()).collect());
        let mut obtained = 0;
        unsafe { AccessibleChildren(acc, 0, &mut children.0, &mut obtained) }
            .map_err(|e| format!("AccessibleChildren: {e}"))?;
        if obtained != count {
            return Err("MSAA children changed during enumeration".into());
        }
        children
            .0
            .iter()
            .map(|v| match unsafe { v.Anonymous.Anonymous.vt } {
                VT_DISPATCH => Ok(Child::Object(dispatch(v)?)),
                VT_I4 => Ok(Child::Integer(integer(v)?)),
                _ => Err("AccessibleChildren returned an unsupported VARIANT type".into()),
            })
            .collect()
        // All raw VARIANTs are cleared, including on partial conversion failure.
        // Each converted dispatch object now owns its own QueryInterface reference.
    }
    fn object_child(
        &mut self,
        acc: &IAccessible,
        child: i32,
    ) -> Result<Option<IAccessible>, String> {
        object_child(acc, &child_id(child).0)
    }
}

struct Walk<P: Provider> {
    provider: P,
    start: Instant,
    nodes: usize,
    seen: HashSet<usize>,
    clip: Rect,
    bounds: Option<Rect>,
}
impl<P: Provider> Walk<P> {
    fn new(provider: P, clip: Rect) -> Self {
        Self {
            provider,
            start: Instant::now(),
            nodes: 0,
            seen: HashSet::new(),
            clip,
            bounds: None,
        }
    }
    fn budget(&mut self, depth: usize) -> Result<(), String> {
        self.nodes += 1;
        if self.nodes > MAX_NODES || depth > MAX_DEPTH || self.start.elapsed() > MAX_ELAPSED {
            return Err("MSAA traversal limit reached; measurement is incomplete".into());
        }
        Ok(())
    }
    /// Returns false for invisible subtrees. Unknown role/state is a measurement
    /// failure, not an invitation to include an entire pane as a button.
    fn inspect(
        &mut self,
        acc: &P::Object,
        id: i32,
        in_toolbar: bool,
    ) -> Result<(bool, bool), String> {
        if is_hidden(self.provider.state(acc, id)?) {
            return Ok((false, in_toolbar));
        }
        let role = self.provider.role(acc, id)?;
        let toolbar = in_toolbar || role == 0x16;
        if toolbar && is_button(role) {
            let r = self.provider.location(acc, id)?;
            // A provider reporting visible buttons outside its own clipping area
            // is inconsistent. Do not silently compute a too-small partial union.
            if !self.clip.contains(r) {
                return Err(
                    "MSAA button is clipped or outside the task-list; measurement uncertain".into(),
                );
            }
            self.bounds = Some(self.bounds.map_or(r, |b| b.union(r)));
        }
        Ok((true, toolbar))
    }
    fn object(&mut self, acc: &P::Object, depth: usize, toolbar: bool) -> Result<(), String> {
        self.budget(depth)?;
        if !self.seen.insert(self.provider.identity(acc)?) {
            return Err("MSAA cycle/aliased object detected; measurement uncertain".into());
        }
        let (visible, toolbar) = self.inspect(acc, 0, toolbar)?;
        if !visible {
            return Ok(());
        }
        let limit = MAX_NODES.saturating_sub(self.nodes);
        let children = self.provider.children(acc, limit)?;
        if children.len() > limit {
            return Err("MSAA child count exceeds traversal limit".into());
        }
        for child in children {
            self.budget(depth + 1)?;
            match child {
                Child::Object(o) => self.object(&o, depth + 1, toolbar)?,
                Child::Integer(id) => {
                    if id <= 0 {
                        return Err("MSAA returned an invalid child ID".into());
                    }
                    if let Some(child) = self.provider.object_child(acc, id)? {
                        self.object(&child, depth + 1, toolbar)?;
                    } else {
                        self.inspect(acc, id, toolbar)?;
                    }
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn content(hwnd: HWND, clip: Rect) -> Result<Rect, String> {
    let mut raw = std::ptr::null_mut();
    // OBJID_CLIENT = -4. COM initialization and DPI context are owned by caller.
    let result =
        unsafe { AccessibleObjectFromWindow(hwnd, (-4i32) as u32, &IAccessible::IID, &mut raw) };
    let acc = if raw.is_null() {
        None
    } else {
        Some(unsafe { IAccessible::from_raw(raw) })
    };
    result.map_err(|e| format!("AccessibleObjectFromWindow: {e}"))?;
    let acc = acc.ok_or("AccessibleObjectFromWindow returned null")?;
    let mut walk = Walk::new(
        Native {
            identities: Vec::new(),
        },
        clip,
    );
    walk.object(&acc, 0, false)?;
    // This limits traversal work, not the duration of an individual out-of-process
    // COM call. MSAA offers no per-call timeout; callers must not run on a UI thread.
    walk.bounds.ok_or_else(|| {
        "No visible task buttons were measured (empty or unavailable MSAA toolbar)".into()
    })
}

#[cfg(test)]
#[path = "accessibility_tests.rs"]
mod tests;
