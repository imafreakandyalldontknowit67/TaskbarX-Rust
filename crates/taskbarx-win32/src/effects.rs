//! Root-scoped feature journal. No OS handles/resources are owned by this layer;
//! native region snapshots are serialized and each write transfers a fresh HRGN.
use crate::{
    geometry::region_parts, state::Api, Point, Rect, TaskbarFeatures, TaskbarId, TaskbarSnapshot,
};
use std::collections::HashMap;

pub(crate) const LAYERED: u32 = 0x0008_0000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayeredAttributes {
    pub key: u32,
    pub alpha: u8,
    pub flags: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpacityOriginal {
    pub layered: bool,
    pub attributes: Option<LayeredAttributes>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlKind {
    Start,
    Tray,
    Toolbar,
    Reserved,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ChildIdentity {
    pub handle: isize,
    /// Immediate parent through root, with classes; child class is separate.
    pub ancestry: Vec<(isize, String)>,
    pub class: String,
    pub process_id: u32,
    pub process_started: u64,
}
#[derive(Debug, Clone)]
pub(crate) struct ControlGeometry {
    pub identity: ChildIdentity,
    pub kind: ControlKind,
    pub rect: Rect,
    pub visible: bool,
    pub position: Point,
    pub parent_clip: Rect,
}
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub snapshot: TaskbarSnapshot,
    pub client: Rect,
    pub controls: Vec<ControlGeometry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegionPart {
    pub rect: Rect,
    pub radius: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Root(pub isize, pub u32, pub u64);
impl From<&TaskbarId> for Root {
    fn from(id: &TaskbarId) -> Self {
        Self(id.root, id.process_id, id.process_started)
    }
}
pub(crate) fn same_root(a: &TaskbarId, b: &TaskbarId) -> bool {
    Root::from(a) == Root::from(b)
}
#[derive(Default)]
struct ChildOriginal {
    visible: Option<bool>,
    position: Option<(Point, u32, bool)>,
}
struct Record {
    id: TaskbarId,
    opacity: Option<OpacityOriginal>,
    region: Option<Option<Vec<u8>>>,
    children: HashMap<ChildIdentity, ChildOriginal>,
    requested: TaskbarFeatures,
    layout: Option<Layout>,
}
impl Record {
    fn new(id: TaskbarId) -> Self {
        Self {
            id,
            opacity: None,
            region: None,
            children: HashMap::new(),
            requested: TaskbarFeatures::default(),
            layout: None,
        }
    }
}
#[derive(Default)]
pub(crate) struct Journal {
    records: HashMap<Root, Record>,
}
impl Journal {
    pub fn roots(&self) -> Vec<TaskbarId> {
        self.records.values().map(|r| r.id).collect()
    }
    pub fn suspend_region(&mut self, api: &mut impl Api, id: &TaskbarId) -> Result<(), String> {
        if let Some(original) = self
            .records
            .get(&Root::from(id))
            .and_then(|r| r.region.as_ref())
        {
            api.restore_region(id, original.as_deref())?;
        }
        Ok(())
    }
    pub fn update_region(&mut self, api: &mut impl Api, id: &TaskbarId) -> Result<(), String> {
        let Some(record) = self.records.get_mut(&Root::from(id)) else {
            return Ok(());
        };
        if record.requested.corner_radius == 0 && !record.requested.segments {
            return Ok(());
        }
        let result = (|| {
            let layout = api.layout(id)?;
            let parts = region_parts(&layout, &record.requested)?;
            if record.region.is_none() {
                record.region = Some(api.capture_region(id)?);
            }
            api.apply_region(id, &parts, layout.client)?;
            record.layout = Some(layout);
            Ok(())
        })();
        if let Err(e) = result {
            // Never leave a stale region clipping new icons after uncertainty.
            if let Some(original) = &record.region {
                api.restore_region(id, original.as_deref())
                    .map_err(|cleanup| format!("{e}; region suspension failed: {cleanup}"))?;
            }
            return Err(e);
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        api: &mut impl Api,
        id: &TaskbarId,
        f: &TaskbarFeatures,
    ) -> Result<bool, String> {
        if f.opacity.is_some_and(|p| p > 100) {
            return Err("Opacity must be 0..100 percent".into());
        }
        self.suspend_region(api, id)?;
        let r = self
            .records
            .entry(Root::from(id))
            .or_insert_with(|| Record::new(*id));
        // Only root identity is shared across task-list replacements.
        if r.id != *id {
            r.layout = None;
            r.id = *id;
        }
        let mut changed =
            r.requested.corner_radius != f.corner_radius || r.requested.segments != f.segments;
        match f.opacity {
            Some(percent) => {
                if r.opacity.is_none() {
                    r.opacity = Some(api.capture_opacity(id)?);
                }
                let original = r.opacity.unwrap();
                let old = original.attributes.unwrap_or(LayeredAttributes {
                    key: 0,
                    alpha: 255,
                    flags: 0,
                });
                api.set_layered_bit(id, true)?;
                api.set_layered(
                    id,
                    LayeredAttributes {
                        alpha: ((u16::from(percent) * 255 + 50) / 100) as u8,
                        flags: old.flags | 2,
                        ..old
                    },
                )?;
            }
            None => Self::restore_opacity(api, r)?,
        }
        // Measure before and after visibility writes. Hiding does NOT imply that
        // Explorer reclaimed the space, or that its parent clipping area grew.
        let before = api.layout(id)?;
        for (kind, hide) in [
            (ControlKind::Start, f.hide_start),
            (ControlKind::Tray, f.hide_tray),
        ] {
            if hide {
                let controls: Vec<_> = before.controls.iter().filter(|c| c.kind == kind).collect();
                if controls.is_empty() {
                    return Err(format!("No validated {kind:?} control was found"));
                }
                for c in controls {
                    let visible = api.child_visible(id, &c.identity)?;
                    let entry = r.children.entry(c.identity.clone()).or_default();
                    entry.visible.get_or_insert(visible);
                    if visible {
                        api.set_child_visible(id, &c.identity, false)?;
                        changed = true;
                    }
                }
            } else {
                let keys: Vec<_> = r
                    .children
                    .keys()
                    .filter(|c| control_kind(c) == kind)
                    .cloned()
                    .collect();
                for child in keys {
                    if api.child_obsolete(id, &child) {
                        r.children.remove(&child);
                        continue;
                    }
                    let entry = r.children.get_mut(&child).unwrap();
                    if let Some(visible) = entry.visible {
                        api.set_child_visible(id, &child, visible)?;
                        entry.visible = None;
                        changed = true;
                    }
                }
            }
        }
        let mut after = api.layout(id)?;
        changed |= Self::compensate(api, id, r, &mut after, f.toolbar_compensation)?;
        changed |= before.snapshot.parent_rect != after.snapshot.parent_rect
            || before.snapshot.available_rect != after.snapshot.available_rect;
        r.layout = Some(after);
        r.requested = f.clone();
        if f.corner_radius == 0 && !f.segments {
            if let Some(original) = &r.region {
                api.restore_region(id, original.as_deref())?;
                r.region = None;
            }
        } else {
            self.update_region(api, id)?;
        }
        Ok(changed)
    }
    pub fn prepare_move(&mut self, api: &mut impl Api, id: &TaskbarId) -> Result<(), String> {
        let Some(r) = self.records.get_mut(&Root::from(id)) else {
            return Ok(());
        };
        // Never reuse a prior child's geometry as compensation input.
        if r.id != *id {
            r.id = *id;
            r.layout = None;
        }
        if !r.requested.toolbar_compensation {
            return Ok(());
        }
        if !r.layout.as_ref().is_some_and(|l| {
            l.controls
                .iter()
                .any(|c| c.visible && c.kind == ControlKind::Toolbar)
        }) {
            // No previous toolbar geometry exists to preserve. The mandatory
            // movement snapshot still remeasures every reservation below.
            return Ok(());
        }
        let mut after = api.layout(id)?;
        Self::compensate(api, id, r, &mut after, true)?;
        r.layout = Some(after);
        Ok(())
    }
    fn compensate(
        api: &mut impl Api,
        id: &TaskbarId,
        r: &mut Record,
        after: &mut Layout,
        enabled: bool,
    ) -> Result<bool, String> {
        let mut changed = false;
        if enabled {
            if let Some(previous) = &r.layout {
                if tray_rects(previous) != tray_rects(after)
                    && previous.snapshot.dpi == after.snapshot.dpi
                    && previous.client == after.client
                    && previous.snapshot.vertical == after.snapshot.vertical
                {
                    changed = true;
                    // One bounded pass, no redraw suppression, messages or busy loop.
                    let candidates: Vec<_> = after
                        .controls
                        .iter()
                        .filter(|c| c.visible && c.kind == ControlKind::Toolbar)
                        .take(64)
                        .map(|c| c.identity.clone())
                        .collect();
                    for identity in candidates {
                        // Prior writes can cause Explorer to relayout other bands.
                        // Re-measure between attempts, never execute a stale batch.
                        *after = api.layout(id)?;
                        let Some(c) = after.controls.iter().find(|c| c.identity == identity) else {
                            continue;
                        };
                        let Some(old) = previous
                            .controls
                            .iter()
                            .find(|o| o.identity == c.identity && o.visible)
                        else {
                            continue;
                        };
                        if let Some(p) = compensation_target(old, c, after) {
                            r.children
                                .entry(c.identity.clone())
                                .or_default()
                                .position
                                .get_or_insert((
                                    c.position,
                                    after.snapshot.dpi,
                                    after.snapshot.vertical,
                                ));
                            api.validate(id)?;
                            api.move_child(id, &c.identity, p, c.position)?;
                        }
                    }
                    *after = api.layout(id)?;
                }
            }
        }
        Ok(changed)
    }
    fn restore_opacity(api: &mut impl Api, r: &mut Record) -> Result<(), String> {
        if let Some(original) = r.opacity {
            if original.layered {
                api.set_layered_bit(&r.id, true)?;
                api.set_layered(
                    &r.id,
                    original.attributes.ok_or("Missing layered baseline")?,
                )?;
            } else {
                api.set_layered_bit(&r.id, false)?;
            }
            r.opacity = None;
        }
        Ok(())
    }
    pub fn restore(&mut self, api: &mut impl Api, id: &TaskbarId) -> Vec<String> {
        let key = Root::from(id);
        let Some(r) = self.records.get_mut(&key) else {
            return vec![];
        };
        if api.root_obsolete(&r.id) {
            self.records.remove(&key);
            return vec![];
        }
        let mut errors = Vec::new();
        // Remove clipping FIRST, then restore control geometry and visibility.
        if let Some(original) = &r.region {
            match api.restore_region(&r.id, original.as_deref()) {
                Ok(()) => r.region = None,
                Err(e) => errors.push(format!("Region: {e}")),
            }
        }
        let keys: Vec<_> = r.children.keys().cloned().collect();
        for child in keys {
            if api.child_obsolete(&r.id, &child) {
                r.children.remove(&child);
                continue;
            }
            let entry = r.children.get_mut(&child).unwrap();
            if let Some((p, dpi, vertical)) = entry.position {
                let result = (|| {
                    if api.layout_stamp(&r.id)? != (dpi, vertical) {
                        return Err(
                            "Control DPI/orientation changed; refusing obsolete coordinates".into(),
                        );
                    }
                    let current = api.child_position(&r.id, &child)?;
                    api.move_child(&r.id, &child, p, current)
                })();
                match result {
                    Ok(()) => entry.position = None,
                    Err(e) => errors.push(format!("Control position: {e}")),
                }
            }
            if let Some(visible) = entry.visible {
                match api.set_child_visible(&r.id, &child, visible) {
                    Ok(()) => entry.visible = None,
                    Err(e) => errors.push(format!("Visibility: {e}")),
                }
            }
            if entry.position.is_none() && entry.visible.is_none() {
                r.children.remove(&child);
            }
        }
        if let Err(e) = Self::restore_opacity(api, r) {
            errors.push(format!("Opacity: {e}"));
        }
        if r.region.is_none() && r.opacity.is_none() && r.children.is_empty() {
            self.records.remove(&key);
        }
        errors
    }
}
fn control_kind(c: &ChildIdentity) -> ControlKind {
    match c.class.as_str() {
        "Start" => ControlKind::Start,
        "TrayNotifyWnd" | "ClockButton" | "ClockButtonWindow" => ControlKind::Tray,
        "ToolbarWindow32" => ControlKind::Toolbar,
        _ => ControlKind::Reserved,
    }
}
fn tray_rects(l: &Layout) -> Vec<Rect> {
    l.controls
        .iter()
        .filter(|c| c.kind == ControlKind::Tray && c.visible)
        .map(|c| c.rect)
        .collect()
}
fn compensation_target(
    old: &ControlGeometry,
    current: &ControlGeometry,
    l: &Layout,
) -> Option<Point> {
    let dx = old.rect.left.checked_sub(current.rect.left)?;
    let dy = old.rect.top.checked_sub(current.rect.top)?;
    if dx == 0 && dy == 0 {
        return None;
    }
    let target = current.rect.translated(dx, dy)?;
    if !current.parent_clip.contains(target)
        || !l.client.contains(target)
        || l.snapshot.content_rect?.intersect(target).is_some()
    {
        return None;
    }
    if l.controls
        .iter()
        .any(|c| c.visible && c.identity != current.identity && c.rect.intersect(target).is_some())
    {
        return None;
    }
    Some(Point {
        x: current.position.x.checked_add(dx)?,
        y: current.position.y.checked_add(dy)?,
    })
}
