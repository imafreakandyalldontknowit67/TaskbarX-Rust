// Pure fake feature primitives: never construct Windows/Session or touch Explorer.
use super::*;
use crate::effects::{ControlGeometry, ControlKind, LAYERED};

struct FeatureFake {
    base: Fake,
    controls: HashMap<isize, Vec<ControlGeometry>>,
    exstyle: HashMap<isize, u32>,
    attributes: HashMap<isize, LayeredAttributes>,
    regions: HashMap<isize, Option<Vec<u8>>>,
    applied: HashMap<isize, Vec<RegionPart>>,
    fail: HashSet<&'static str>,
    partial: HashSet<&'static str>,
    provider: MeasurementBackend,
    ui_available: bool,
    invalid_children: bool,
    reclaim: bool,
}
impl FeatureFake {
    fn new() -> Self {
        let mut f = Self {
            base: Fake::new(),
            controls: HashMap::new(),
            exstyle: HashMap::new(),
            attributes: HashMap::new(),
            regions: HashMap::new(),
            applied: HashMap::new(),
            fail: HashSet::new(),
            partial: HashSet::new(),
            provider: MeasurementBackend::Msaa,
            ui_available: true,
            invalid_children: false,
            reclaim: false,
        };
        for root in [1, 2] {
            f.exstyle.insert(root, 0x80);
            f.attributes.insert(
                root,
                LayeredAttributes {
                    key: 0x123456,
                    alpha: 79,
                    flags: 3,
                },
            );
            f.regions.insert(root, Some(vec![root as u8, 42]));
            f.controls.insert(
                root,
                vec![
                    control(root, ControlKind::Start, -1000, -950),
                    control(root, ControlKind::Tray, -100, 0),
                ],
            );
        }
        f
    }
    fn guard(&self, name: &'static str) -> Result<(), String> {
        if self.fail.contains(name) {
            Err(format!("capture/write failure: {name}"))
        } else {
            Ok(())
        }
    }
    fn finish(&self, name: &'static str) -> Result<(), String> {
        if self.partial.contains(name) {
            Err(format!("partial write: {name}"))
        } else {
            Ok(())
        }
    }
    fn check(&mut self, id: &TaskbarId, c: &ChildIdentity) -> Result<usize, String> {
        self.base.validate_root(id)?;
        if self.invalid_children {
            return Err("child query access failure".into());
        }
        self.controls[&id.root]
            .iter()
            .position(|x| x.identity == *c)
            .ok_or_else(|| "obsolete child".into())
    }
}
fn control(root: isize, kind: ControlKind, left: i32, right: i32) -> ControlGeometry {
    let (class, h) = match kind {
        ControlKind::Start => ("Start", 100),
        ControlKind::Tray => ("TrayNotifyWnd", 200),
        ControlKind::Toolbar => ("ToolbarWindow32", 300),
        ControlKind::Reserved => ("DynamicContent1", 400),
    };
    ControlGeometry {
        identity: ChildIdentity {
            handle: root * 1000 + h,
            ancestry: vec![(root, "Shell_TrayWnd".into())],
            class: class.into(),
            process_id: 42,
            process_started: 123456789,
        },
        kind,
        rect: Rect {
            left,
            top: 100,
            right,
            bottom: 140,
        },
        visible: true,
        position: Point {
            x: left + 1000,
            y: 0,
        },
        parent_clip: snapshot(id(root)).taskbar_rect,
    }
}
impl Api for FeatureFake {
    fn check_accent(&mut self, _: Accent) -> Result<(), String> {
        self.guard("accent_support")
    }
    fn discover(&mut self) -> Result<Discovery, String> {
        self.base.discover()
    }
    fn environment(&mut self) -> Result<EnvironmentSnapshot, String> {
        Ok(EnvironmentSnapshot {
            on_battery: None,
            monitors: vec![],
            warnings: vec!["unknown power".into()],
        })
    }
    fn set_measurement_backend(&mut self, p: MeasurementBackend) -> Result<(), String> {
        if p == MeasurementBackend::UiAutomation && !self.ui_available {
            return Err("UIA unavailable".into());
        }
        self.provider = p;
        Ok(())
    }
    fn validate(&mut self, id: &TaskbarId) -> Result<(), String> {
        self.base.validate(id)
    }
    fn validate_root(&mut self, id: &TaskbarId) -> Result<(), String> {
        self.base.validate_root(id)
    }
    fn root_obsolete(&mut self, id: &TaskbarId) -> bool {
        !self
            .base
            .live
            .iter()
            .any(|other| effects::same_root(id, other))
    }
    fn obsolete(&mut self, id: &TaskbarId) -> bool {
        !self.base.live.contains(id)
    }
    fn snapshot(&mut self, id: &TaskbarId) -> Result<TaskbarSnapshot, String> {
        self.guard("measurement")?;
        let mut s = self.base.snapshot(id)?;
        let reservations: Vec<_> = self.controls[&id.root].iter().map(|c| c.rect).collect();
        if let Some(free) =
            crate::geometry::available(s.available_rect, &reservations, s.content_rect, s.vertical)
        {
            s.available_rect = free;
        } else {
            s.content_rect = None;
        }
        Ok(s)
    }
    fn position(&mut self, id: &TaskbarId) -> Result<(Point, bool), String> {
        self.base.position(id)
    }
    fn move_to(&mut self, id: &TaskbarId, p: Point, expected: (Point, bool)) -> Result<(), String> {
        // Region has been suspended BEFORE every animation move.
        assert!(!self.applied.contains_key(&id.root));
        self.base.move_to(id, p, expected)?;
        self.finish("move")
    }
    fn query_accent(&mut self, id: &TaskbarId) -> Result<Option<Accent>, String> {
        self.guard("accent_capture")?;
        self.base.query_accent(id)
    }
    fn apply_accent(&mut self, id: &TaskbarId, a: Accent) -> Result<(), String> {
        self.base.apply_accent(id, a)?;
        self.finish("accent")
    }
    fn restore_accent(
        &mut self,
        id: &TaskbarId,
        a: Option<Accent>,
    ) -> Result<Option<String>, String> {
        self.base.restore_accent(id, a)
    }
    fn layout(&mut self, id: &TaskbarId) -> Result<Layout, String> {
        self.guard("layout")?;
        let mut snapshot = self.snapshot(id)?;
        if self.reclaim
            && self.controls[&id.root]
                .iter()
                .any(|c| c.kind == ControlKind::Tray && !c.visible)
        {
            snapshot.available_rect.right = -50;
        }
        Ok(Layout {
            client: snapshot.taskbar_rect,
            snapshot,
            controls: self.controls[&id.root].clone(),
        })
    }
    fn capture_opacity(&mut self, id: &TaskbarId) -> Result<OpacityOriginal, String> {
        self.base.validate_root(id)?;
        self.guard("opacity_capture")?;
        let layered = self.exstyle[&id.root] & LAYERED != 0;
        Ok(OpacityOriginal {
            layered,
            attributes: layered.then_some(self.attributes[&id.root]),
        })
    }
    fn set_layered_bit(&mut self, id: &TaskbarId, enabled: bool) -> Result<(), String> {
        self.base.validate_root(id)?;
        self.guard("bit")?;
        self.base.record("bit", id);
        let bits = self.exstyle.get_mut(&id.root).unwrap();
        *bits = (*bits & !LAYERED) | if enabled { LAYERED } else { 0 };
        self.finish("bit")
    }
    fn set_layered(&mut self, id: &TaskbarId, a: LayeredAttributes) -> Result<(), String> {
        self.base.validate_root(id)?;
        self.guard("alpha")?;
        self.base.record("alpha", id);
        self.attributes.insert(id.root, a);
        self.finish("alpha")
    }
    fn child_obsolete(&mut self, id: &TaskbarId, c: &ChildIdentity) -> bool {
        !self.invalid_children && !self.controls[&id.root].iter().any(|x| x.identity == *c)
    }
    fn child_visible(&mut self, id: &TaskbarId, c: &ChildIdentity) -> Result<bool, String> {
        self.guard("visibility_capture")?;
        let n = self.check(id, c)?;
        Ok(self.controls[&id.root][n].visible)
    }
    fn set_child_visible(
        &mut self,
        id: &TaskbarId,
        c: &ChildIdentity,
        visible: bool,
    ) -> Result<(), String> {
        self.guard("visibility")?;
        let n = self.check(id, c)?;
        self.base.record("visible", id);
        self.controls.get_mut(&id.root).unwrap()[n].visible = visible;
        self.finish("visibility")
    }
    fn child_position(&mut self, id: &TaskbarId, c: &ChildIdentity) -> Result<Point, String> {
        let n = self.check(id, c)?;
        Ok(self.controls[&id.root][n].position)
    }
    fn move_child(
        &mut self,
        id: &TaskbarId,
        c: &ChildIdentity,
        p: Point,
        expected: Point,
    ) -> Result<(), String> {
        let n = self.check(id, c)?;
        self.guard("toolbar")?;
        assert_eq!(self.controls[&id.root][n].position, expected);
        self.base.record("toolbar", id);
        let c = &mut self.controls.get_mut(&id.root).unwrap()[n];
        c.rect = c
            .rect
            .translated(p.x - c.position.x, p.y - c.position.y)
            .unwrap();
        c.position = p;
        self.finish("toolbar")
    }
    fn capture_region(&mut self, id: &TaskbarId) -> Result<Option<Vec<u8>>, String> {
        self.base.validate_root(id)?;
        self.guard("region_capture")?;
        Ok(self.regions[&id.root].clone())
    }
    fn apply_region(
        &mut self,
        id: &TaskbarId,
        parts: &[RegionPart],
        _: Rect,
    ) -> Result<(), String> {
        self.base.validate_root(id)?;
        self.guard("region")?;
        self.base.record("region", id);
        self.applied.insert(id.root, parts.to_vec());
        self.regions.insert(id.root, Some(vec![99]));
        self.finish("region")
    }
    fn restore_region(&mut self, id: &TaskbarId, data: Option<&[u8]>) -> Result<(), String> {
        self.base.validate_root(id)?;
        self.guard("restore_region")?;
        self.base.record("restore_region", id);
        self.applied.remove(&id.root);
        self.regions.insert(id.root, data.map(Vec::from));
        self.finish("restore_region")
    }
}
fn features_session() -> Controller<FeatureFake> {
    let mut c = Controller::new(FeatureFake::new());
    c.discover().unwrap();
    c
}
fn full() -> TaskbarFeatures {
    TaskbarFeatures {
        style: Style::Blur,
        opacity: Some(45),
        corner_radius: 9,
        segments: true,
        hide_start: true,
        hide_tray: true,
        ..Default::default()
    }
}

#[test]
fn optional_percentage_opacity_keeps_existing_layered_attributes_and_unrelated_bits() {
    for layered in [false, true] {
        for percent in [0, 1, 50, 100] {
            let mut c = features_session();
            if layered {
                *c.api.exstyle.get_mut(&1).unwrap() |= LAYERED;
            }
            let original = c.api.attributes[&1];
            let f = TaskbarFeatures {
                opacity: Some(percent),
                ..Default::default()
            };
            c.apply_features(&id(1), &f).unwrap();
            assert_eq!(
                c.api.attributes[&1].alpha,
                ((u16::from(percent) * 255 + 50) / 100) as u8
            );
            assert_eq!(c.api.exstyle[&1] & LAYERED, LAYERED);
            *c.api.exstyle.get_mut(&1).unwrap() |= 0x4000;
            c.apply_features(&id(1), &TaskbarFeatures::default())
                .unwrap();
            assert_eq!(
                c.api.exstyle[&1],
                0x4080 | if layered { LAYERED } else { 0 }
            );
            if layered {
                assert_eq!(c.api.attributes[&1], original);
            }
        }
    }
}
#[test]
fn invalid_opacity_rejected_before_any_write() {
    let mut c = features_session();
    let f = TaskbarFeatures {
        opacity: Some(101),
        ..full()
    };
    assert!(c.apply_features(&id(1), &f).is_err());
    assert!(c.api.base.log.borrow().is_empty());
}
#[test]
fn default_accent_does_not_restore_independent_effects() {
    let mut c = features_session();
    c.apply_features(&id(1), &full()).unwrap();
    c.set_style(&id(1), Style::Default, [0; 4]).unwrap();
    assert_eq!(c.api.exstyle[&1] & LAYERED, LAYERED);
    assert!(c.api.applied.contains_key(&1));
    assert!(!c.api.controls[&1][0].visible);
}
#[test]
fn original_hidden_controls_stay_hidden_and_regions_restore_exactly() {
    let mut c = features_session();
    c.api.controls.get_mut(&1).unwrap()[0].visible = false;
    let baseline = c.api.regions[&1].clone();
    c.apply_features(&id(1), &full()).unwrap();
    c.apply_features(&id(1), &full()).unwrap();
    assert!(c.restore().is_empty());
    assert!(!c.api.controls[&1][0].visible);
    assert!(c.api.controls[&1][1].visible);
    assert_eq!(c.api.regions[&1], baseline);
    let n = c.api.base.log.borrow().len();
    assert!(c.restore().is_empty());
    assert_eq!(n, c.api.base.log.borrow().len());
}
#[test]
fn partial_writes_have_baselines_and_retry_independently() {
    for write in ["bit", "alpha", "visibility", "region", "accent"] {
        let mut c = features_session();
        c.api.partial.insert(write);
        assert!(c.apply_features(&id(1), &full()).is_err(), "{write}");
        c.api.partial.clear();
        assert!(c.restore().is_empty(), "{write}");
        assert_eq!(c.api.exstyle[&1], 0x80);
        assert!(c.api.controls[&1].iter().all(|x| x.visible));
        assert_eq!(c.api.regions[&1], Some(vec![1, 42]));
    }
    let mut c = features_session();
    c.api.partial.insert("move");
    assert!(c.move_task_list(&id(1), Point { x: 100, y: 3 }).is_err());
    c.api.partial.clear();
    assert!(c.restore().is_empty());
    assert_eq!(c.api.base.snapshots[&id(1)].position.x, 0);
}
#[test]
fn capture_failures_do_not_attempt_that_effect() {
    for capture in [
        "opacity_capture",
        "visibility_capture",
        "region_capture",
        "accent_capture",
    ] {
        let mut c = features_session();
        c.api.fail.insert(capture);
        assert!(c.apply_features(&id(1), &full()).is_err());
        c.api.fail.clear();
        assert!(c.restore().is_empty());
        assert_eq!(c.api.exstyle[&1], 0x80);
        assert!(c.api.controls[&1].iter().all(|x| x.visible));
        assert_eq!(c.api.regions[&1], Some(vec![1, 42]));
    }
}
#[test]
fn per_root_cleanup_continues_after_failure_and_retries() {
    let mut c = features_session();
    c.apply_features(&id(1), &full()).unwrap();
    c.apply_features(&id(2), &full()).unwrap();
    c.api.base.log.borrow_mut().clear();
    c.api.fail.insert("restore_region");
    assert_eq!(c.restore_taskbar(&id(1)).len(), 1);
    assert!(c.api.controls[&1][0].visible);
    assert_eq!(c.api.exstyle[&1], 0x80);
    assert!(!c.api.controls[&2][0].visible);
    assert!(c.api.base.log.borrow().iter().all(|s| s.ends_with(":1")));
    c.api.fail.clear();
    assert!(c.restore_taskbar(&id(1)).is_empty());
    assert!(c.api.applied.contains_key(&2));
    assert!(c.restore().is_empty());
}
#[test]
fn root_scoped_cleanup_survives_list_replacement_without_rebaselining() {
    let mut c = features_session();
    c.apply_features(&id(1), &full()).unwrap();
    let old = id(1);
    let mut new = old;
    new.task_list += 900;
    c.api.base.live.remove(&old);
    c.api.base.live.insert(new);
    c.api.base.snapshots.remove(&old);
    c.api.base.snapshots.insert(new, snapshot(new));
    c.discover().unwrap();
    c.apply_features(&new, &full()).unwrap();
    assert!(c.restore_taskbar(&new).is_empty());
    assert_eq!(c.api.regions[&1], Some(vec![1, 42]));
    assert_eq!(c.api.exstyle[&1], 0x80);
    assert!(c.api.controls[&1][0].visible);
}
#[test]
fn old_child_baseline_never_applied_to_replacement_and_query_errors_retry() {
    let mut c = features_session();
    c.apply_features(&id(1), &full()).unwrap();
    c.api.invalid_children = true;
    assert_eq!(c.restore_taskbar(&id(1)).len(), 2);
    c.api.invalid_children = false;
    let replacement = &mut c.api.controls.get_mut(&1).unwrap()[0];
    replacement.identity.handle += 1;
    replacement.visible = false;
    assert!(c.restore_taskbar(&id(1)).is_empty());
    assert!(!c.api.controls[&1][0].visible);
    assert!(c.api.controls[&1][1].visible);
}
#[test]
fn reused_root_process_identity_discards_obsolete_effects_without_writes() {
    for pid in [false, true] {
        let mut c = features_session();
        c.apply_features(&id(1), &full()).unwrap();
        let mut next = id(1);
        if pid {
            next.process_id += 1;
        } else {
            next.process_started += 1;
        }
        c.api.base.live.remove(&id(1));
        c.api.base.live.insert(next);
        c.api.base.log.borrow_mut().clear();
        assert!(c.restore_taskbar(&id(1)).is_empty());
        assert!(c.api.base.log.borrow().is_empty());
    }
}
#[test]
fn region_cleanup_precedes_visibility_and_list_position() {
    let mut c = features_session();
    c.apply_features(&id(1), &full()).unwrap();
    c.move_task_list(&id(1), Point { x: 100, y: 3 }).unwrap();
    c.api.base.log.borrow_mut().clear();
    assert!(c.restore().is_empty());
    let log = c.api.base.log.borrow();
    assert!(log[0].starts_with("restore_region"));
    assert!(
        log.iter().position(|s| s.starts_with("visible")).unwrap()
            < log.iter().position(|s| s.starts_with("move")).unwrap()
    );
}
#[test]
fn hiding_remeasures_layout_without_assuming_space_was_reclaimed() {
    for reclaim in [false, true] {
        let mut c = features_session();
        c.api.reclaim = reclaim;
        let f = TaskbarFeatures {
            hide_tray: true,
            ..Default::default()
        };
        assert!(c.apply_features(&id(1), &f).unwrap());
        let l = c.api.layout(&id(1)).unwrap();
        assert_eq!(
            l.snapshot.available_rect.right,
            if reclaim { -50 } else { -100 }
        );
    }
}
#[test]
fn each_animation_frame_tracks_displayed_buttons_and_preserves_all_controls() {
    for vertical in [false, true] {
        let mut c = features_session();
        if vertical {
            let swap = |r: Rect| Rect {
                left: r.top,
                top: r.left,
                right: r.bottom,
                bottom: r.right,
            };
            let s = c.api.base.snapshots.get_mut(&id(1)).unwrap();
            s.vertical = true;
            s.taskbar_rect = swap(s.taskbar_rect);
            s.parent_rect = swap(s.parent_rect);
            s.list_rect = swap(s.list_rect);
            s.available_rect = swap(s.available_rect);
            s.content_rect = s.content_rect.map(swap);
            s.position = Point { x: 3, y: 0 };
            for control in c.api.controls.get_mut(&1).unwrap() {
                control.rect = swap(control.rect);
                control.parent_clip = swap(control.parent_clip);
                control.position = Point {
                    x: control.position.y,
                    y: control.position.x,
                };
            }
        }
        let f = TaskbarFeatures {
            segments: true,
            corner_radius: 12,
            ..Default::default()
        };
        c.apply_features(&id(1), &f).unwrap();
        for n in [20, 80, 180, 220] {
            let p = if vertical {
                Point { x: 3, y: n }
            } else {
                Point { x: n, y: 3 }
            };
            c.move_task_list(&id(1), p).unwrap();
            let l = c.api.layout(&id(1)).unwrap();
            let parts = &c.api.applied[&1];
            assert_eq!(*parts, crate::geometry::region_parts(&l, &f).unwrap());
            assert_eq!(parts.len(), 4); // background + full button union + Start + tray
        }
        c.api.base.snapshots.get_mut(&id(1)).unwrap().content_rect = None;
        assert!(c.move_task_list(&id(1), Point { x: 300, y: 300 }).is_err());
        assert!(!c.api.applied.contains_key(&1));
    }
}
#[test]
fn empty_unsafe_or_failed_measurement_suspends_region_instead_of_hiding_taskbar() {
    let mut c = features_session();
    let f = TaskbarFeatures {
        segments: true,
        ..Default::default()
    };
    c.apply_features(&id(1), &f).unwrap();
    c.api.controls.get_mut(&1).unwrap()[0].rect.left -= 30;
    assert!(c.apply_features(&id(1), &f).is_err());
    assert!(!c.api.applied.contains_key(&1));
    assert_eq!(c.api.regions[&1], Some(vec![1, 42]));
    assert!(c.restore().is_empty());
}
#[test]
fn toolbar_compensation_is_bounded_and_optional_with_safe_retargeting() {
    for enabled in [false, true] {
        let mut c = features_session();
        c.api
            .controls
            .get_mut(&1)
            .unwrap()
            .push(control(1, ControlKind::Toolbar, -500, -450));
        let f = TaskbarFeatures {
            toolbar_compensation: enabled,
            ..Default::default()
        };
        c.apply_features(&id(1), &f).unwrap();
        let controls = c.api.controls.get_mut(&1).unwrap();
        controls[1].rect.left = -120;
        controls[2].rect = controls[2].rect.translated(-20, 0).unwrap();
        controls[2].position.x -= 20;
        c.apply_features(&id(1), &f).unwrap();
        assert_eq!(
            c.api.controls[&1][2].rect.left,
            if enabled { -500 } else { -520 }
        );
        let n = c
            .api
            .base
            .log
            .borrow()
            .iter()
            .filter(|s| s.starts_with("toolbar"))
            .count();
        assert_eq!(n, usize::from(enabled));
        assert!(c.restore().is_empty());
        assert_eq!(c.api.controls[&1][2].rect.left, -520); // captured before attempted compensation
    }
}
#[test]
fn providers_select_explicitly_and_environment_is_read_only() {
    let mut c = features_session();
    c.environment().unwrap();
    assert_eq!(c.api.provider, MeasurementBackend::Msaa);
    c.api.ui_available = false;
    assert!(c
        .set_measurement_backend(MeasurementBackend::UiAutomation)
        .is_err());
    assert_eq!(c.api.provider, MeasurementBackend::Msaa);
    c.api.ui_available = true;
    c.set_measurement_backend(MeasurementBackend::UiAutomation)
        .unwrap();
    assert_eq!(c.api.provider, MeasurementBackend::UiAutomation);
    c.discover().unwrap();
    assert!(c.api.base.log.borrow().is_empty());
}

#[test]
fn independently_hidden_primary_start_and_secondary_tray_restore_only_their_baselines() {
    for (start, tray) in [(true, false), (false, true)] {
        let mut c = features_session();
        c.api.controls.get_mut(&2).unwrap()[1].identity.class = "ClockButton".into();
        let f = TaskbarFeatures {
            hide_start: start,
            hide_tray: tray,
            ..Default::default()
        };
        c.apply_features(&id(2), &f).unwrap();
        assert_eq!(c.api.controls[&2][0].visible, !start);
        assert_eq!(c.api.controls[&2][1].visible, !tray);
        assert!(c.api.controls[&1].iter().all(|c| c.visible));
        assert!(c.restore_taskbar(&id(2)).is_empty());
        assert!(c.api.controls[&2].iter().all(|c| c.visible));
    }
}
#[test]
fn all_reserved_control_branches_survive_segment_unions() {
    let mut c = features_session();
    for (n, name) in [
        "TrayButton",
        "SearchBox",
        "TaskViewButton",
        "DynamicContent1",
        "ToolbarWindow32",
        "TrayNotifyWnd",
    ]
    .into_iter()
    .enumerate()
    {
        let mut extra = control(
            1,
            ControlKind::Reserved,
            -650 + n as i32 * 60,
            -600 + n as i32 * 60,
        );
        extra.identity.handle += n as isize;
        extra.identity.class = name.into();
        c.api.controls.get_mut(&1).unwrap().push(extra);
    }
    let f = TaskbarFeatures {
        segments: true,
        corner_radius: 1000,
        ..Default::default()
    };
    assert!(c.apply_features(&id(1), &f).unwrap());
    let parts = &c.api.applied[&1];
    assert_eq!(parts[0].radius, 20);
    for control in &c.api.controls[&1] {
        let local = control.rect.translated(1000, -100).unwrap();
        assert!(parts.iter().any(|p| p.rect == local && p.radius == 0));
    }
    c.apply_features(&id(1), &TaskbarFeatures::default())
        .unwrap();
    assert!(!c.api.applied.contains_key(&1));
    assert_eq!(c.api.regions[&1], Some(vec![1, 42]));
}
#[test]
fn child_class_ancestry_pid_creation_replacements_do_not_receive_old_visibility() {
    for n in 0..4 {
        let mut c = features_session();
        c.apply_features(&id(1), &full()).unwrap();
        let child = &mut c.api.controls.get_mut(&1).unwrap()[0];
        match n {
            0 => child.identity.class = "UnrelatedWindow".into(),
            1 => child.identity.ancestry[0].0 += 900,
            2 => child.identity.process_id += 1,
            _ => child.identity.process_started += 1,
        };
        child.visible = false;
        assert!(c.restore_taskbar(&id(1)).is_empty());
        assert!(!c.api.controls[&1][0].visible);
        assert!(c.api.controls[&1][1].visible);
    }
}
#[test]
fn default_style_finds_original_root_owner_after_child_replacement() {
    let mut c = features_session();
    let original = c.api.base.styles[&id(1)];
    c.set_style(&id(1), Style::Blur, [0; 4]).unwrap();
    let mut replacement = id(1);
    replacement.task_list += 99;
    c.api.base.live.remove(&id(1));
    c.api.base.live.insert(replacement);
    c.set_style(&replacement, Style::Default, [0; 4]).unwrap();
    assert_eq!(c.api.base.styles[&id(1)], original);
    assert!(c.changes.is_empty());
}
#[test]
fn toolbar_compensation_also_runs_before_a_move_and_never_crosses_occupied_buttons() {
    for collision in [false, true] {
        let mut c = features_session();
        c.api
            .controls
            .get_mut(&1)
            .unwrap()
            .push(control(1, ControlKind::Toolbar, -500, -450));
        c.apply_features(&id(1), &TaskbarFeatures::default())
            .unwrap();
        let controls = c.api.controls.get_mut(&1).unwrap();
        controls[1].rect.left = -130;
        controls[2].rect = controls[2].rect.translated(-20, 0).unwrap();
        controls[2].position.x -= 20;
        if collision {
            c.api.base.snapshots.get_mut(&id(1)).unwrap().content_rect = Some(Rect {
                left: -650,
                top: 103,
                right: -400,
                bottom: 140,
            });
        }
        let result = c.move_task_list(&id(1), Point { x: 10, y: 3 });
        assert_eq!(result.is_err(), collision);
        assert_eq!(
            c.api.controls[&1][2].rect.left,
            if collision { -520 } else { -500 }
        );
    }
}
#[test]
fn toolbar_partial_write_and_dpi_cleanup_failure_remain_retryable() {
    let mut c = features_session();
    c.api
        .controls
        .get_mut(&1)
        .unwrap()
        .push(control(1, ControlKind::Toolbar, -500, -450));
    c.apply_features(&id(1), &TaskbarFeatures::default())
        .unwrap();
    let controls = c.api.controls.get_mut(&1).unwrap();
    controls[1].rect.left = -130;
    controls[2].rect = controls[2].rect.translated(-20, 0).unwrap();
    controls[2].position.x -= 20;
    c.api.partial.insert("toolbar");
    assert!(c
        .apply_features(&id(1), &TaskbarFeatures::default())
        .is_err());
    c.api.partial.clear();
    c.api.base.snapshots.get_mut(&id(1)).unwrap().dpi = 192;
    assert_eq!(c.restore().len(), 1);
    c.api.base.snapshots.get_mut(&id(1)).unwrap().dpi = 144;
    assert!(c.restore().is_empty());
    assert_eq!(c.api.controls[&1][2].rect.left, -520);
}

#[test]
fn unsupported_composition_is_rejected_before_journaling_or_querying() {
    let mut c = features_session();
    c.api.fail.insert("accent_support");
    assert!(c.set_style(&id(1), Style::Transparent, [0; 4]).is_err());
    assert!(c.changes.is_empty());
    assert!(c.api.base.log.borrow().is_empty());
    assert!(c.restore().is_empty());
}
#[test]
fn disabling_extra_compensation_never_disables_fresh_collision_checks() {
    for extra in [false, true] {
        let mut c = features_session();
        let f = TaskbarFeatures {
            toolbar_compensation: extra,
            ..Default::default()
        };
        c.apply_features(&id(1), &f).unwrap();
        c.api.controls.get_mut(&1).unwrap()[1].rect.left = -650;
        assert!(c.move_task_list(&id(1), Point { x: 200, y: 3 }).is_err());
        assert_eq!(c.api.base.snapshots[&id(1)].position.x, 0);
    }
}
#[test]
fn dpi_changes_remeasure_regions_without_scaling_physical_screen_coordinates_twice() {
    let mut c = features_session();
    let f = TaskbarFeatures {
        segments: true,
        corner_radius: 1000,
        ..Default::default()
    };
    c.apply_features(&id(1), &f).unwrap();
    assert_eq!(c.api.applied[&1][0].radius, 20);
    let s = c.api.base.snapshots.get_mut(&id(1)).unwrap();
    s.dpi = 192;
    s.taskbar_rect.bottom = 180;
    s.parent_rect.bottom = 180;
    s.list_rect.bottom = 180;
    s.available_rect.bottom = 180;
    for control in c.api.controls.get_mut(&1).unwrap() {
        control.rect.bottom = 180;
        control.parent_clip.bottom = 180;
    }
    c.apply_features(&id(1), &f).unwrap();
    assert_eq!(c.api.applied[&1][0].radius, 40);
    assert_eq!(c.api.applied[&1][1].rect.left, 100); // signed screen -900 minus root -1000
}
