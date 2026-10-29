use crate::{
    geometry::{axis_position, checked_destination},
    Discovery, Point, Style, TaskbarId, TaskbarSnapshot,
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
        Style::Transparent => 2, // transparent gradient, not undocumented state 6
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
}

impl<A: Api> Controller<A> {
    pub fn new(api: A) -> Self {
        Self {
            api,
            known: HashSet::new(),
            changes: HashMap::new(),
            diagnostics: Vec::new(),
        }
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
            return Ok(());
        }
        self.api.move_to(id, p, (s.position, s.vertical))?;
        self.changes
            .entry(*id)
            .or_default()
            .position
            .get_or_insert((s.position, s.vertical));
        Ok(())
    }
    pub fn restore_position(&mut self, id: &TaskbarId) -> Result<(), String> {
        let Some((original, vertical)) = self.changes.get(id).and_then(|c| c.position) else {
            return Ok(());
        };
        self.api.validate(id)?;
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
        self.api.apply_accent(id, a)?;
        self.changes
            .entry(*id)
            .or_default()
            .accent
            .get_or_insert(original);
        Ok(())
    }
    fn restore_style(&mut self, id: &TaskbarId) -> Result<(), String> {
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
        let ids: Vec<_> = self.changes.keys().copied().collect();
        let mut errors = Vec::new();
        for id in ids {
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
