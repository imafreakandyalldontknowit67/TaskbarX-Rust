use crate::{
    effects::{self, ChildIdentity, LayeredAttributes, Layout, OpacityOriginal, RegionPart},
    geometry::{axis_position, checked_destination},
    Discovery, EnvironmentSnapshot, MeasurementBackend, Point, Rect, Style, TaskbarFeatures,
    TaskbarId, TaskbarSnapshot,
};
use std::collections::{HashMap, HashSet};

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Accent {
    pub state: i32,
    pub flags: u32,
    pub color: u32,
    pub animation: u32,
}

pub(crate) fn pack_abgr(rgba: [u8; 4]) -> u32 {
    u32::from_le_bytes(rgba)
}
pub(crate) fn accent(style: Style, mut rgba: [u8; 4]) -> Option<Accent> {
    let state = match style {
        Style::Default => return None,
        Style::Transparent => 6, // distinct classic-taskbar fully transparent composition
        Style::TransparentGradient => 2,
        Style::Blur => 3,
        Style::Acrylic => 4,
        Style::Opaque => {
            rgba[3] = 255;
            1
        }
    };
    Some(Accent {
        state,
        flags: 2,
        color: pack_abgr(rgba),
        animation: 0,
    })
}

/// All mutation implementations must validate identity again immediately before
/// calling the OS, not merely rely on a previous discovery/measurement.
pub(crate) trait Api {
    fn layout_stamp(&mut self, id: &TaskbarId) -> Result<(u32, bool), String> {
        let s = self.snapshot(id)?;
        Ok((s.dpi, s.vertical))
    }
    fn environment(&mut self) -> Result<EnvironmentSnapshot, String> {
        Err("Environment inspection unsupported".into())
    }
    fn set_measurement_backend(&mut self, _backend: MeasurementBackend) -> Result<(), String> {
        Err("Measurement provider unsupported".into())
    }
    fn layout(&mut self, _id: &TaskbarId) -> Result<Layout, String> {
        Err("Feature layout unsupported".into())
    }
    fn capture_opacity(&mut self, _id: &TaskbarId) -> Result<OpacityOriginal, String> {
        Err("Opacity capture unsupported".into())
    }
    fn set_layered_bit(&mut self, _id: &TaskbarId, _enabled: bool) -> Result<(), String> {
        Err("Layered windows unsupported".into())
    }
    fn set_layered(&mut self, _id: &TaskbarId, _a: LayeredAttributes) -> Result<(), String> {
        Err("Layered attributes unsupported".into())
    }
    fn child_obsolete(&mut self, _id: &TaskbarId, _child: &ChildIdentity) -> bool {
        false
    }
    fn child_visible(&mut self, _id: &TaskbarId, _child: &ChildIdentity) -> Result<bool, String> {
        Err("Visibility capture unsupported".into())
    }
    fn set_child_visible(
        &mut self,
        _id: &TaskbarId,
        _child: &ChildIdentity,
        _visible: bool,
    ) -> Result<(), String> {
        Err("Visibility mutation unsupported".into())
    }
    fn child_position(&mut self, _id: &TaskbarId, _child: &ChildIdentity) -> Result<Point, String> {
        Err("Control position unsupported".into())
    }
    fn move_child(
        &mut self,
        _id: &TaskbarId,
        _child: &ChildIdentity,
        _p: Point,
        _expected: Point,
    ) -> Result<(), String> {
        Err("Control movement unsupported".into())
    }
    fn capture_region(&mut self, _id: &TaskbarId) -> Result<Option<Vec<u8>>, String> {
        Err("Region capture unsupported".into())
    }
    fn apply_region(
        &mut self,
        _id: &TaskbarId,
        _parts: &[RegionPart],
        _client: Rect,
    ) -> Result<(), String> {
        Err("Regions unsupported".into())
    }
    fn restore_region(&mut self, _id: &TaskbarId, _data: Option<&[u8]>) -> Result<(), String> {
        Err("Region restoration unsupported".into())
    }
    fn discover(&mut self) -> Result<Discovery, String>;
    fn validate(&mut self, id: &TaskbarId) -> Result<(), String>;
    // True only for positively identified destruction/replacement, not query errors.
    fn validate_root(&mut self, id: &TaskbarId) -> Result<(), String> {
        self.validate(id)
    }
    fn root_obsolete(&mut self, _id: &TaskbarId) -> bool {
        false
    }
    fn obsolete(&mut self, _id: &TaskbarId) -> bool {
        false
    }
    fn snapshot(&mut self, id: &TaskbarId) -> Result<TaskbarSnapshot, String>;
    fn position(&mut self, id: &TaskbarId) -> Result<(Point, bool), String>;
    fn move_to(&mut self, id: &TaskbarId, p: Point, expected: (Point, bool)) -> Result<(), String>;
    fn query_accent(&mut self, id: &TaskbarId) -> Result<Option<Accent>, String>;
    fn check_accent(&mut self, _a: Accent) -> Result<(), String> {
        Ok(())
    }
    fn apply_accent(&mut self, id: &TaskbarId, a: Accent) -> Result<(), String>;
    fn restore_accent(
        &mut self,
        id: &TaskbarId,
        a: Option<Accent>,
    ) -> Result<Option<String>, String>;
}

#[derive(Default)]
struct Original {
    position: Option<(Point, bool)>,
    // Outer None = untouched; inner None = original accent unqueryable.
    accent: Option<Option<Accent>>,
}

pub(crate) struct Controller<A: Api> {
    pub(crate) api: A,
    known: HashSet<TaskbarId>,
    changes: HashMap<TaskbarId, Original>,
    diagnostics: Vec<String>,
    effects: effects::Journal,
}

impl<A: Api> Controller<A> {
    pub fn new(api: A) -> Self {
        Self {
            api,
            known: HashSet::new(),
            changes: HashMap::new(),
            diagnostics: Vec::new(),
            effects: effects::Journal::default(),
        }
    }
    pub fn environment(&mut self) -> Result<EnvironmentSnapshot, String> {
        self.api.environment()
    }
    pub fn set_measurement_backend(&mut self, backend: MeasurementBackend) -> Result<(), String> {
        self.api.set_measurement_backend(backend)
    }
    pub fn apply_features(
        &mut self,
        id: &TaskbarId,
        features: &TaskbarFeatures,
    ) -> Result<bool, String> {
        if features.opacity.is_some_and(|p| p > 100) {
            return Err("Opacity must be 0..100 percent".into());
        }
        self.require_known(id)?;
        self.set_style(id, features.style, features.color)?;
        self.effects.apply(&mut self.api, id, features)
    }
    pub fn discover(&mut self) -> Result<Discovery, String> {
        let mut d = self.api.discover()?;
        self.known = d.taskbars.iter().map(|s| s.id).collect();
        d.warnings.append(&mut self.diagnostics);
        // No journal updates or style reapplication here, even after topology changes.
        Ok(d)
    }
    fn require_known(&mut self, id: &TaskbarId) -> Result<(), String> {
        if !self.known.contains(id) {
            return Err(
                "Taskbar identity was not discovered by this session (or has changed)".into(),
            );
        }
        self.api.validate(id)
    }
    pub fn move_task_list(&mut self, id: &TaskbarId, p: Point) -> Result<(), String> {
        self.require_known(id)?;
        self.effects.suspend_region(&mut self.api, id)?;
        self.effects.prepare_move(&mut self.api, id)?;
        let s = self.api.snapshot(id)?;
        if self
            .changes
            .get(id)
            .and_then(|c| c.position)
            .is_some_and(|(_, v)| v != s.vertical)
        {
            return Err(
                "Taskbar orientation changed after the original position was captured".into(),
            );
        }
        let p = checked_destination(&s, p)?;
        if p == s.position {
            return self.effects.update_region(&mut self.api, id);
        }
        self.changes
            .entry(*id)
            .or_default()
            .position
            .get_or_insert((s.position, s.vertical));
        self.api.move_to(id, p, (s.position, s.vertical))?;
        self.effects.update_region(&mut self.api, id)
    }
    pub fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String> {
        let Some((original, vertical)) = self.changes.get(id).and_then(|c| c.position) else {
            return Ok(());
        };
        // Runtime retains moved identities across discovery gaps. A positively
        // replaced/destroyed child has no position left to restore. Retire only
        // that position, keeping root appearance/effects owned independently.
        // Query failures are NOT evidence of destruction and still error below.
        if self.api.obsolete(id) {
            self.changes.get_mut(id).unwrap().position = None;
            self.prune(id);
            return Ok(());
        }
        self.api.validate(id)?;
        self.effects.suspend_region(&mut self.api, id)?;
        let (current, now_vertical) = self.api.position(id)?;
        if now_vertical != vertical {
            return Err(
                "Taskbar orientation changed; refusing to restore obsolete coordinates".into(),
            );
        }
        let p = axis_position(current, original, vertical);
        if current != p {
            self.api.move_to(id, p, (current, now_vertical))?;
        }
        self.changes.get_mut(id).unwrap().position = None;
        self.prune(id);
        Ok(())
    }
    pub fn set_style(&mut self, id: &TaskbarId, style: Style, rgba: [u8; 4]) -> Result<(), String> {
        let Some(a) = accent(style, rgba) else {
            return self.restore_style(id);
        };
        self.require_known(id)?;
        // Accent ownership follows the root, not a replaceable task-list child.
        self.api.check_accent(a)?;
        let previous = self.changes.keys().copied().find(|other| {
            other.root == id.root
                && other.process_id == id.process_id
                && other.process_started == id.process_started
                && other != id
                && self.changes[other].accent.is_some()
        });
        if let Some(previous) = previous {
            self.api.validate_root(&previous)?;
            let original = self.changes.get_mut(&previous).unwrap().accent.take();
            self.changes.entry(*id).or_default().accent = original;
            self.prune(&previous);
        }
        let original = match self.changes.get(id).and_then(|c| c.accent) {
            Some(a) => a,
            None => self.api.query_accent(id)?,
        };
        self.changes
            .entry(*id)
            .or_default()
            .accent
            .get_or_insert(original);
        self.api.apply_accent(id, a)
    }
    fn restore_style(&mut self, id: &TaskbarId) -> Result<(), String> {
        let owner =
            self.changes.keys().copied().find(|other| {
                effects::same_root(id, other) && self.changes[other].accent.is_some()
            });
        let Some(id) = owner.as_ref() else {
            return Ok(());
        };
        let Some(original) = self.changes.get(id).and_then(|c| c.accent) else {
            return Ok(());
        };
        self.api.validate_root(id)?;
        if let Some(warning) = self.api.restore_accent(id, original)? {
            self.diagnostics.push(warning);
        }
        self.changes.get_mut(id).unwrap().accent = None;
        self.prune(id);
        Ok(())
    }
    fn prune(&mut self, id: &TaskbarId) {
        if self
            .changes
            .get(id)
            .is_some_and(|c| c.position.is_none() && c.accent.is_none())
        {
            self.changes.remove(id);
        }
    }
    pub fn restore(&mut self) -> Vec<String> {
        let mut ids: Vec<_> = self.changes.keys().copied().collect();
        ids.extend(self.effects.roots());
        let mut seen = HashSet::new();
        let mut errors = Vec::new();
        for id in ids {
            if seen.insert(effects::Root::from(&id)) {
                errors.extend(self.restore_taskbar(&id));
            }
        }
        errors
    }
    pub fn restore_taskbar(&mut self, root: &TaskbarId) -> Vec<String> {
        let mut errors = self.effects.restore(&mut self.api, root);
        let mut ids: Vec<_> = self
            .changes
            .keys()
            .copied()
            .filter(|id| effects::same_root(root, id))
            .collect();
        ids.sort_by_key(|id| (id.task_list, id.parent));
        for id in ids {
            if !self.changes.contains_key(&id) {
                continue;
            }
            if self.api.obsolete(&id) {
                self.changes.get_mut(&id).unwrap().position = None;
                if self.api.root_obsolete(&id) {
                    self.changes.get_mut(&id).unwrap().accent = None;
                }
                self.prune(&id);
            }
            if let Err(e) = self.restore_position(&id) {
                errors.push(format!("Position restoration {id:?}: {e}"));
            }
            if let Err(e) = self.restore_style(&id) {
                errors.push(format!("Style restoration {id:?}: {e}"));
            }
        }
        // Successful fallback diagnostics are warnings, not cleanup failures.
        for message in self.diagnostics.drain(..) {
            use std::io::Write;
            let _ = writeln!(std::io::stderr().lock(), "taskbarx warning: {message}");
        }
        errors
    }
}

impl<A: Api> Drop for Controller<A> {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
