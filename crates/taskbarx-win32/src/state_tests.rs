// All tests use this in-memory API. Never create a Windows backend/session or
// touch Explorer. The journal exercised here is the production Controller.
use super::*;
use crate::Rect;
use std::{cell::RefCell, rc::Rc};

fn id(root: isize) -> TaskbarId {
    TaskbarId {
        root,
        task_list: root + 10,
        parent: root + 20,
        process_id: 42,
        process_started: 123456789,
    }
}
fn snapshot(id: TaskbarId) -> TaskbarSnapshot {
    TaskbarSnapshot {
        id,
        primary: id.root == 1,
        vertical: false,
        dpi: 144,
        taskbar_rect: Rect {
            left: -1000,
            top: 100,
            right: 0,
            bottom: 140,
        },
        parent_rect: Rect {
            left: -900,
            top: 100,
            right: -100,
            bottom: 140,
        },
        list_rect: Rect {
            left: -900,
            top: 103,
            right: -100,
            bottom: 140,
        },
        available_rect: Rect {
            left: -900,
            top: 100,
            right: -100,
            bottom: 140,
        },
        content_rect: Some(Rect {
            left: -900,
            top: 103,
            right: -700,
            bottom: 140,
        }),
        position: Point { x: 0, y: 3 },
    }
}

struct Fake {
    snapshots: HashMap<TaskbarId, TaskbarSnapshot>,
    styles: HashMap<TaskbarId, Accent>,
    log: Rc<RefCell<Vec<String>>>,
    live: HashSet<TaskbarId>,
    fail_move: HashSet<TaskbarId>,
    fail_style: HashSet<TaskbarId>,
    fail_refresh: bool,
    queryable: bool,
    invalid_classes: bool,
    invalid_ancestry: bool,
}
impl Fake {
    fn new() -> Self {
        let ids = [id(1), id(2)];
        Self {
            snapshots: ids.into_iter().map(|id| (id, snapshot(id))).collect(),
            styles: ids
                .into_iter()
                .map(|id| {
                    (
                        id,
                        Accent {
                            state: 3,
                            flags: 17,
                            color: 123,
                            animation: 4,
                        },
                    )
                })
                .collect(),
            log: Rc::default(),
            live: ids.into_iter().collect(),
            fail_move: HashSet::new(),
            fail_style: HashSet::new(),
            fail_refresh: false,
            queryable: true,
            invalid_classes: false,
            invalid_ancestry: false,
        }
    }
    fn record(&self, kind: &str, id: &TaskbarId) {
        self.log.borrow_mut().push(format!("{kind}:{}", id.root));
    }
}
impl Api for Fake {
    fn validate_root(&mut self, id: &TaskbarId) -> Result<(), String> {
        if !self.invalid_classes
            && self.live.iter().any(|i| {
                i.root == id.root
                    && i.process_id == id.process_id
                    && i.process_started == id.process_started
            })
        {
            Ok(())
        } else {
            Err("stale root".into())
        }
    }

    fn discover(&mut self) -> Result<Discovery, String> {
        Ok(Discovery {
            build_number: 19045,
            native_windows11: false,
            taskbars: self.snapshots.values().cloned().collect(),
            warnings: vec![],
        })
    }
    fn validate(&mut self, id: &TaskbarId) -> Result<(), String> {
        if self.live.contains(id) && !self.invalid_classes && !self.invalid_ancestry {
            Ok(())
        } else {
            Err("stale".into())
        }
    }
    fn snapshot(&mut self, id: &TaskbarId) -> Result<TaskbarSnapshot, String> {
        self.validate(id)?;
        Ok(self.snapshots[id].clone())
    }
    fn position(&mut self, id: &TaskbarId) -> Result<(Point, bool), String> {
        self.validate(id)?;
        let s = &self.snapshots[id];
        Ok((s.position, s.vertical))
    }
    fn move_to(&mut self, id: &TaskbarId, p: Point, expected: (Point, bool)) -> Result<(), String> {
        self.validate(id)?;
        if self.position(id)? != expected {
            return Err("Geometry changed since measurement".into());
        }
        if self.fail_move.contains(id) {
            return Err("move failed".into());
        }
        self.record("move", id);
        let s = self.snapshots.get_mut(id).unwrap();
        let (dx, dy) = (p.x - s.position.x, p.y - s.position.y);
        s.list_rect = s.list_rect.translated(dx, dy).unwrap();
        s.content_rect = s.content_rect.and_then(|r| r.translated(dx, dy));
        s.position = p;
        Ok(())
    }
    fn query_accent(&mut self, id: &TaskbarId) -> Result<Option<Accent>, String> {
        self.validate(id)?;
        self.record("query", id);
        Ok(self.queryable.then_some(self.styles[id]))
    }
    fn apply_accent(&mut self, id: &TaskbarId, a: Accent) -> Result<(), String> {
        self.validate_root(id)?;
        if self.fail_style.contains(id) {
            return Err("style failed".into());
        }
        self.record("style", id);
        self.styles.insert(*id, a);
        Ok(())
    }
    fn restore_accent(
        &mut self,
        id: &TaskbarId,
        a: Option<Accent>,
    ) -> Result<Option<String>, String> {
        self.apply_accent(id, a.unwrap_or_default())?;
        if a.is_none() && self.fail_refresh {
            return Err("theme refresh failed after accent disabled".into());
        }
        Ok(a.is_none()
            .then(|| "Windows-managed fallback; exact third-party style not restored".into()))
    }
}
fn session() -> Controller<Fake> {
    let mut c = Controller::new(Fake::new());
    c.discover().unwrap();
    c
}
fn move_it(c: &mut Controller<Fake>, i: TaskbarId, x: i32) {
    c.move_task_list(&i, Point { x, y: 999 }).unwrap();
}

#[test]
fn abgr_and_all_style_mappings() {
    assert_eq!(std::mem::size_of::<Accent>(), 16);
    assert_eq!(pack_abgr([0x12, 0x34, 0x56, 0x78]), 0x78563412);
    assert_eq!(accent(Style::Default, [0; 4]), None);
    for (s, n) in [
        (Style::Transparent, 2),
        (Style::Blur, 3),
        (Style::Acrylic, 4),
        (Style::Opaque, 1),
    ] {
        let a = accent(s, [1, 2, 3, 4]).unwrap();
        assert_eq!(a.state, n);
        assert_eq!(a.flags, 2);
        assert_eq!(
            a.color,
            if s == Style::Opaque {
                0xff030201
            } else {
                0x04030201
            }
        );
    }
}
#[test]
fn discovery_default_restore_and_drop_make_zero_mutations() {
    let mut c = session();
    let log = c.api.log.clone();
    c.discover().unwrap();
    c.set_style(&id(1), Style::Default, [0; 4]).unwrap();
    c.restore_position(&id(1)).unwrap();
    assert!(c.restore().is_empty());
    drop(c);
    assert!(log.borrow().is_empty());
}
#[test]
fn discovery_never_rebaselines_modified_positions() {
    let mut c = session();
    move_it(&mut c, id(1), 100);
    c.discover().unwrap();
    move_it(&mut c, id(1), 200);
    assert_eq!(c.api.snapshots[&id(1)].position, Point { x: 200, y: 3 });
    assert!(c.restore().is_empty());
    assert_eq!(c.api.snapshots[&id(1)].position, Point { x: 0, y: 3 });
    let n = c.api.log.borrow().len();
    assert!(c.restore().is_empty());
    assert_eq!(c.api.log.borrow().len(), n);
}
#[test]
fn cross_axis_preserved_on_restoration_and_vertical_movement() {
    let mut c = session();
    move_it(&mut c, id(1), 100);
    c.api.snapshots.get_mut(&id(1)).unwrap().position.y = 7;
    c.restore_position(&id(1)).unwrap();
    assert_eq!(c.api.snapshots[&id(1)].position, Point { x: 0, y: 7 });
    let s = c.api.snapshots.get_mut(&id(2)).unwrap();
    s.vertical = true;
    s.available_rect = Rect {
        left: -900,
        top: -1000,
        right: -100,
        bottom: 1000,
    };
    c.move_task_list(&id(2), Point { x: 999, y: 30 }).unwrap();
    assert_eq!(c.api.snapshots[&id(2)].position, Point { x: 0, y: 30 });
}
#[test]
fn unavailable_measurements_and_invalid_destinations_do_not_move() {
    let mut c = session();
    assert!(c.move_task_list(&id(1), Point { x: 700, y: 3 }).is_err());
    c.api.snapshots.get_mut(&id(1)).unwrap().content_rect = None;
    assert!(c.move_task_list(&id(1), Point { x: 100, y: 3 }).is_err());
    assert!(c.api.log.borrow().is_empty());
    assert!(c.changes.is_empty());
}
#[test]
fn unchanged_position_is_not_journaled() {
    let mut c = session();
    move_it(&mut c, id(1), 0);
    assert!(c.changes.is_empty());
    assert!(c.api.log.borrow().is_empty());
}
#[test]
fn style_original_is_queried_once_and_default_restores_it() {
    let mut c = session();
    let before = c.api.styles[&id(1)];
    c.set_style(&id(1), Style::Acrylic, [1; 4]).unwrap();
    c.discover().unwrap();
    c.set_style(&id(1), Style::Opaque, [2; 4]).unwrap();
    assert_eq!(
        c.api
            .log
            .borrow()
            .iter()
            .filter(|e| e.starts_with("query"))
            .count(),
        1
    );
    c.set_style(&id(1), Style::Default, [0; 4]).unwrap();
    assert_eq!(c.api.styles[&id(1)], before);
    assert!(c.changes.is_empty());
}
#[test]
fn failed_mutations_are_not_restored() {
    let mut c = session();
    c.api.fail_move.insert(id(1));
    c.api.fail_style.insert(id(1));
    assert!(c.move_task_list(&id(1), Point { x: 100, y: 3 }).is_err());
    assert!(c.set_style(&id(1), Style::Blur, [0; 4]).is_err());
    assert!(c.changes.is_empty());
    let n = c.api.log.borrow().len();
    c.restore();
    assert_eq!(c.api.log.borrow().len(), n);
}
#[test]
fn partial_restore_failure_does_not_prevent_other_objects_or_effects() {
    let mut c = session();
    move_it(&mut c, id(1), 100);
    move_it(&mut c, id(2), 100);
    c.set_style(&id(1), Style::Blur, [0; 4]).unwrap();
    c.api.fail_move.insert(id(1));
    assert_eq!(c.restore().len(), 1);
    assert_eq!(c.api.snapshots[&id(2)].position.x, 0);
    assert!(c.changes[&id(1)].accent.is_none());
    assert!(c.changes[&id(1)].position.is_some());
    c.api.fail_move.clear();
    assert!(c.restore().is_empty());
    assert!(c.changes.is_empty());
}
#[test]
fn fallback_reports_limitation_and_retries_partial_theme_failure() {
    let mut c = session();
    c.api.queryable = false;
    c.set_style(&id(1), Style::Transparent, [0; 4]).unwrap();
    c.api.fail_refresh = true;
    assert_eq!(c.restore().len(), 1);
    assert!(!c.changes.is_empty());
    c.api.fail_refresh = false;
    let report = c.restore();
    assert!(
        report.is_empty(),
        "successful fallback is not a restoration error"
    );
    assert!(c.changes.is_empty());
    assert!(c.restore().is_empty());
}
#[test]
fn stale_identity_pid_creation_parent_and_handles_never_restore() {
    for field in 0..5 {
        let mut c = session();
        move_it(&mut c, id(1), 100);
        c.set_style(&id(1), Style::Blur, [0; 4]).unwrap();
        let mut replacement = id(1);
        match field {
            0 => replacement.process_started += 1,
            1 => replacement.process_id += 1,
            2 => replacement.parent += 1,
            3 => replacement.task_list += 1,
            _ => replacement.root += 1,
        };
        c.api.live.remove(&id(1));
        c.api.live.insert(replacement);
        let n = c.api.log.borrow().len();
        let errors = c.restore();
        let root_survives = field == 2 || field == 3;
        assert_eq!(errors.len(), if root_survives { 1 } else { 2 });
        assert_eq!(c.api.log.borrow().len(), n + usize::from(root_survives));
        let log = c.api.log.clone();
        drop(c);
        assert_eq!(log.borrow().len(), n + usize::from(root_survives));
    }
}
#[test]
fn class_and_ancestry_must_still_match_before_each_mutation() {
    for bad_class in [true, false] {
        let mut c = session();
        c.api.invalid_classes = bad_class;
        c.api.invalid_ancestry = !bad_class;
        assert!(c.move_task_list(&id(1), Point { x: 100, y: 3 }).is_err());
        assert!(c.set_style(&id(1), Style::Opaque, [0; 4]).is_err());
        assert!(c.api.log.borrow().is_empty());
    }
}
#[test]
fn orientation_changes_are_not_restored_using_obsolete_axis() {
    let mut c = session();
    move_it(&mut c, id(1), 100);
    c.api.snapshots.get_mut(&id(1)).unwrap().vertical = true;
    let n = c.api.log.borrow().len();
    assert!(c.restore_position(&id(1)).is_err());
    assert_eq!(c.api.log.borrow().len(), n);
}
#[test]
fn forged_or_no_longer_discovered_identity_cannot_mutate() {
    let mut c = session();
    assert!(c.set_style(&id(99), Style::Blur, [0; 4]).is_err());
    c.api.snapshots.clear();
    c.discover().unwrap();
    assert!(c.set_style(&id(1), Style::Blur, [0; 4]).is_err());
}
#[test]
fn drop_restores_only_successful_owned_changes() {
    let mut c = session();
    let log = c.api.log.clone();
    move_it(&mut c, id(2), 100);
    drop(c);
    assert_eq!(&*log.borrow(), &["move:2", "move:2"]);
}

#[test]
fn new_explorer_can_reuse_a_root_without_reusing_its_original_style() {
    let mut c = session();
    c.set_style(&id(1), Style::Blur, [0; 4]).unwrap();
    let mut new_id = id(1);
    new_id.process_started += 1;
    let new_original = Accent {
        state: 1,
        flags: 99,
        color: 0xff123456,
        animation: 0,
    };
    c.api.live.remove(&id(1));
    c.api.live.insert(new_id);
    c.api.snapshots.remove(&id(1));
    c.api.snapshots.insert(new_id, snapshot(new_id));
    c.api.styles.insert(new_id, new_original);
    c.discover().unwrap();
    c.set_style(&new_id, Style::Transparent, [0; 4]).unwrap();
    let errors = c.restore();
    assert_eq!(errors.len(), 1); // obsolete Explorer never receives restoration
    assert_eq!(c.api.styles[&new_id], new_original);
    assert!(!c.changes.contains_key(&new_id));
}

#[test]
fn changed_task_list_in_same_root_does_not_rebaseline_applied_style() {
    let mut c = session();
    c.set_style(&id(1), Style::Blur, [0; 4]).unwrap();
    let mut new_id = id(1);
    new_id.task_list += 100;
    c.api.live.insert(new_id);
    c.api.snapshots.insert(new_id, snapshot(new_id));
    c.discover().unwrap();
    let n = c.api.log.borrow().len();
    let original = c.changes[&id(1)].accent.unwrap().unwrap();
    c.api.live.remove(&id(1));
    c.set_style(&new_id, Style::Opaque, [0; 4]).unwrap();
    assert_eq!(c.api.log.borrow().len(), n + 1); // no second query/rebaseline
    assert!(c.restore().is_empty());
    assert_eq!(c.api.styles[&new_id], original);
}

#[test]
fn positively_obsolete_journal_entries_do_not_block_explorer_recovery() {
    struct Gone(Fake);
    impl Api for Gone {
        fn obsolete(&mut self, id: &TaskbarId) -> bool {
            !self.0.live.contains(id)
        }
        fn root_obsolete(&mut self, id: &TaskbarId) -> bool {
            self.0.validate_root(id).is_err()
        }
        fn discover(&mut self) -> Result<Discovery, String> {
            self.0.discover()
        }
        fn validate(&mut self, id: &TaskbarId) -> Result<(), String> {
            self.0.validate(id)
        }
        fn snapshot(&mut self, id: &TaskbarId) -> Result<TaskbarSnapshot, String> {
            self.0.snapshot(id)
        }
        fn position(&mut self, id: &TaskbarId) -> Result<(Point, bool), String> {
            self.0.position(id)
        }
        fn move_to(&mut self, id: &TaskbarId, p: Point, e: (Point, bool)) -> Result<(), String> {
            self.0.move_to(id, p, e)
        }
        fn query_accent(&mut self, id: &TaskbarId) -> Result<Option<Accent>, String> {
            self.0.query_accent(id)
        }
        fn apply_accent(&mut self, id: &TaskbarId, a: Accent) -> Result<(), String> {
            self.0.apply_accent(id, a)
        }
        fn restore_accent(
            &mut self,
            id: &TaskbarId,
            a: Option<Accent>,
        ) -> Result<Option<String>, String> {
            self.0.restore_accent(id, a)
        }
    }
    let mut c = Controller::new(Gone(Fake::new()));
    c.discover().unwrap();
    c.move_task_list(&id(1), Point { x: 100, y: 3 }).unwrap();
    c.set_style(&id(1), Style::Blur, [0; 4]).unwrap();
    c.api.0.live.remove(&id(1));
    c.api.0.log.borrow_mut().clear();
    assert!(c.restore().is_empty());
    assert!(c.api.0.log.borrow().is_empty());
    assert!(c.changes.is_empty());
    c.api.0.live.insert(id(1));
    c.discover().unwrap();
    c.move_task_list(&id(1), Point { x: 200, y: 3 }).unwrap();
    assert!(c.restore().is_empty());
}

#[test]
fn surviving_root_restores_even_without_reapplying_to_recreated_child() {
    let mut c = session();
    let old = id(1);
    let original = c.api.styles[&old];
    c.set_style(&old, Style::Blur, [0; 4]).unwrap();
    c.api.live.remove(&old);
    let mut replacement = old;
    replacement.task_list += 100;
    c.api.live.insert(replacement);
    assert!(c.restore().is_empty());
    assert_eq!(c.api.styles[&old], original);
    assert!(c.changes.is_empty());
}
