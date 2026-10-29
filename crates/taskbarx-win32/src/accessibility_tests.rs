// Exercise the real traversal using a pure provider: no COM server, windows,
// SetWindowPos, composition APIs, or other real taskbar mutations are involved.
use super::*;
use std::collections::HashMap;

struct Node {
    role: i32,
    state: Option<i32>,
    rect: Option<Rect>,
}
struct Fake {
    nodes: HashMap<(usize, i32), Node>,
    children: HashMap<usize, Vec<Child<usize>>>,
    promoted: HashMap<(usize, i32), usize>,
    fail_children: HashSet<usize>,
}
impl Fake {
    fn node(&mut self, object: usize, child: i32, role: i32, state: i32, rect: Option<Rect>) {
        self.nodes.insert(
            (object, child),
            Node {
                role,
                state: Some(state),
                rect,
            },
        );
    }
    fn n(&self, object: usize, child: i32) -> Result<&Node, String> {
        self.nodes
            .get(&(object, child))
            .ok_or_else(|| "missing provider node".into())
    }
}
impl Provider for Fake {
    type Object = usize;
    fn identity(&mut self, o: &usize) -> Result<usize, String> {
        Ok(*o)
    }
    fn state(&mut self, o: &usize, c: i32) -> Result<i32, String> {
        self.n(*o, c)?.state.ok_or_else(|| "missing state".into())
    }
    fn role(&mut self, o: &usize, c: i32) -> Result<i32, String> {
        Ok(self.n(*o, c)?.role)
    }
    fn location(&mut self, o: &usize, c: i32) -> Result<Rect, String> {
        self.n(*o, c)?.rect.ok_or_else(|| "missing location".into())
    }
    fn children(&mut self, o: &usize, limit: usize) -> Result<Vec<Child<usize>>, String> {
        if self.fail_children.contains(o) {
            return Err("partial AccessibleChildren result".into());
        }
        let v = self.children.get(o).cloned().unwrap_or_default();
        if v.len() > limit {
            return Err("node count limit".into());
        }
        Ok(v)
    }
    fn object_child(&mut self, o: &usize, c: i32) -> Result<Option<usize>, String> {
        Ok(self.promoted.get(&(*o, c)).copied())
    }
}
fn r(left: i32, right: i32) -> Rect {
    Rect {
        left,
        top: -40,
        right,
        bottom: 0,
    }
}
fn tree() -> Fake {
    let mut f = Fake {
        nodes: HashMap::new(),
        children: HashMap::new(),
        promoted: HashMap::new(),
        fail_children: HashSet::new(),
    };
    f.node(1, 0, 0x0a, 0, Some(r(-1000, 0))); // stretching client container
    f.children.insert(1, vec![Child::Object(2)]);
    f.node(2, 0, 0x16, 0, Some(r(-1000, 0))); // toolbar
    f.children.insert(
        2,
        vec![
            Child::Integer(91),
            Child::Integer(42),
            Child::Integer(7),
            Child::Object(5),
        ],
    );
    f.node(2, 91, 0x2b, 0, Some(r(-900, -850))); // non-contiguous integer ID
    f.promoted.insert((2, 42), 3); // integer child resolving to dispatch object
    f.node(3, 0, 0x16, 0, Some(r(-1000, 0))); // nested toolbar
    f.children.insert(3, vec![Child::Object(4)]);
    f.node(4, 0, 0x2b, 0, Some(r(-850, -777))); // include entire final button
    f.node(2, 7, 0x15, 0, Some(r(-1000, 0))); // separator must not expand union
    f.node(5, 0, 0x16, 0x8000, None); // invisible subtree, don't even query its children
    f.fail_children.insert(5);
    f
}
fn walk(f: Fake) -> Result<Rect, String> {
    let mut w = Walk::new(f, r(-1000, 0));
    w.object(&1, 0, false)?;
    w.bounds.ok_or_else(|| "no measured buttons".into())
}
#[test]
fn mixed_integer_dispatch_nested_toolbar_and_final_extent() {
    assert_eq!(walk(tree()).unwrap(), r(-900, -777));
}
#[test]
fn hidden_and_offscreen_children_do_not_contribute() {
    for state in [0x8000, 0x10000, 0x108000] {
        let mut f = tree();
        f.nodes.get_mut(&(4, 0)).unwrap().state = Some(state);
        assert_eq!(walk(f).unwrap(), r(-900, -850));
    }
}
#[test]
fn panes_text_and_toolbars_are_never_task_buttons() {
    for role in [0x10, 0x16, 0x29, 0x2a] {
        let mut f = tree();
        f.nodes.get_mut(&(4, 0)).unwrap().role = role;
        assert_eq!(walk(f).unwrap(), r(-900, -850));
    }
}
#[test]
fn incomplete_children_missing_state_and_location_reject_partial_union() {
    let mut f = tree();
    f.fail_children.insert(3);
    assert!(walk(f).is_err());
    let mut f = tree();
    f.nodes.get_mut(&(4, 0)).unwrap().state = None;
    assert!(walk(f).is_err());
    let mut f = tree();
    f.nodes.get_mut(&(4, 0)).unwrap().rect = None;
    assert!(walk(f).is_err());
}
#[test]
fn clipped_button_not_silently_truncated() {
    let mut f = tree();
    f.nodes.get_mut(&(4, 0)).unwrap().rect = Some(r(-850, 10));
    assert!(walk(f).is_err());
}
#[test]
fn no_toolbar_or_no_buttons_cannot_guess_content() {
    let mut f = tree();
    f.nodes.get_mut(&(2, 0)).unwrap().role = 0x10;
    f.nodes.get_mut(&(3, 0)).unwrap().role = 0x10;
    assert!(walk(f).is_err());
    let mut f = tree();
    f.children.clear();
    assert!(walk(f).is_err());
}
#[test]
fn cycles_bad_ids_depth_and_node_limits_fail_closed() {
    let mut f = tree();
    f.children.insert(4, vec![Child::Object(1)]);
    assert!(walk(f).is_err());
    let mut f = tree();
    f.children.insert(4, vec![Child::Integer(0)]);
    assert!(walk(f).is_err());
    let mut f = tree();
    f.children.insert(4, vec![Child::Integer(1); MAX_NODES + 1]);
    assert!(walk(f).is_err());
    let mut f = tree();
    for n in 1..=MAX_DEPTH + 2 {
        f.node(n, 0, 0x16, 0, None);
        f.children.insert(n, vec![Child::Object(n + 1)]);
    }
    f.fail_children.clear();
    assert!(walk(f).is_err());
}
#[test]
fn elapsed_budget_is_checked_without_sleeping() {
    let mut w = Walk::new(tree(), r(-1000, 0));
    w.start = Instant::now() - MAX_ELAPSED - Duration::from_millis(1);
    assert!(w.object(&1, 0, false).is_err());
}

#[test]
fn automation_roles_use_the_same_conservative_occupied_bounds_walk() {
    for (uia, role) in [
        (50000, 0x2b),
        (50002, 0x2b),
        (50013, 0x2b),
        (50007, 0x2b),
        (50021, 0x16),
        (50008, 0x16),
        (50033, 0x10),
        (50020, 0x10),
    ] {
        assert_eq!(automation_role(uia), role);
    }
    let mut f = tree();
    f.nodes.get_mut(&(2, 0)).unwrap().role = automation_role(50021);
    f.nodes.get_mut(&(3, 0)).unwrap().role = automation_role(50008);
    f.nodes.get_mut(&(4, 0)).unwrap().role = automation_role(50007);
    assert_eq!(walk(f).unwrap(), r(-900, -777));
    let mut f = tree();
    f.nodes.get_mut(&(4, 0)).unwrap().rect = Some(r(1, 100));
    assert!(walk(f).is_err()); // physical screen bounds, not DIP-scaled/guessed
}
