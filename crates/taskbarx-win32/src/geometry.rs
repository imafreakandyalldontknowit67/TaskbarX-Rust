use crate::{Point, Rect, TaskbarSnapshot};

impl Rect {
    pub(crate) fn normalized(self) -> Self {
        Self {
            left: self.left.min(self.right),
            right: self.left.max(self.right),
            top: self.top.min(self.bottom),
            bottom: self.top.max(self.bottom),
        }
    }
    pub(crate) fn valid(self) -> bool {
        self.left < self.right && self.top < self.bottom
    }
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        let r = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        r.valid().then_some(r)
    }
    pub(crate) fn contains(self, other: Self) -> bool {
        other.valid()
            && self.left <= other.left
            && self.top <= other.top
            && self.right >= other.right
            && self.bottom >= other.bottom
    }
    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
    pub(crate) fn translated(self, dx: i32, dy: i32) -> Option<Self> {
        Some(Self {
            left: self.left.checked_add(dx)?,
            right: self.right.checked_add(dx)?,
            top: self.top.checked_add(dy)?,
            bottom: self.bottom.checked_add(dy)?,
        })
    }
    pub(crate) fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Option<Self> {
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(Self {
            left: x,
            top: y,
            right: x.checked_add(w)?,
            bottom: y.checked_add(h)?,
        })
    }
}

/// Subtract reservations on the major axis, never guessing which side of a
/// middle toolbar is usable. Select the one contiguous span holding all buttons.
pub(crate) fn available(
    base: Rect,
    reservations: &[Rect],
    content: Option<Rect>,
    vertical: bool,
) -> Option<Rect> {
    if !base.valid() {
        return None;
    }
    let axis = |r: Rect| {
        if vertical {
            (r.top, r.bottom)
        } else {
            (r.left, r.right)
        }
    };
    let mut spans = vec![axis(base)];
    for reservation in reservations {
        let Some(r) = base.intersect(*reservation) else {
            continue;
        };
        let (a, b) = axis(r);
        spans = spans
            .into_iter()
            .flat_map(|(l, h)| {
                if b <= l || a >= h {
                    return vec![(l, h)];
                }
                let mut v = Vec::new();
                if a > l {
                    v.push((l, a));
                }
                if b < h {
                    v.push((b, h));
                }
                v
            })
            .collect();
    }
    let span = match content {
        Some(c) => {
            if !base.contains(c) {
                return None;
            }
            let (a, b) = axis(c);
            spans.iter().copied().find(|(l, h)| *l <= a && *h >= b)?
        }
        None if spans.len() == 1 => spans[0],
        None => return None,
    };
    Some(if vertical {
        Rect {
            top: span.0,
            bottom: span.1,
            ..base
        }
    } else {
        Rect {
            left: span.0,
            right: span.1,
            ..base
        }
    })
}

pub(crate) fn axis_position(current: Point, requested: Point, vertical: bool) -> Point {
    if vertical {
        Point {
            x: current.x,
            y: requested.y,
        }
    } else {
        Point {
            x: requested.x,
            y: current.y,
        }
    }
}

pub(crate) fn checked_destination(s: &TaskbarSnapshot, requested: Point) -> Result<Point, String> {
    let c = s
        .content_rect
        .ok_or("Task buttons were not measured reliably; refusing to move")?;
    let p = axis_position(s.position, requested, s.vertical);
    let dx = p.x.checked_sub(s.position.x).ok_or("Position overflow")?;
    let dy = p.y.checked_sub(s.position.y).ok_or("Position overflow")?;
    let moved = c.translated(dx, dy).ok_or("Rectangle overflow")?;
    if !s.available_rect.contains(moved) {
        return Err("Destination would clip task buttons or overlap reserved controls".into());
    }
    Ok(p)
}

// MSAA constants: pushbutton, checkbutton, radiobutton, splitbutton and button
// dropdown variants. Toolbars/panes/text/separators are containers, not buttons.
pub(crate) fn is_button(role: i32) -> bool {
    matches!(role, 0x2b..=0x2d | 0x38..=0x3a | 0x3e)
}
pub(crate) fn is_hidden(state: i32) -> bool {
    state & (0x8000 | 0x10000) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(l: i32, t: i32, r: i32, b: i32) -> Rect {
        Rect {
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }
    #[test]
    fn negative_origin_and_normalization() {
        assert_eq!(r(-10, 4, -200, -20).normalized(), r(-200, -20, -10, 4));
        assert_eq!(
            r(-200, -20, -10, 4).translated(10, 20),
            Some(r(-190, 0, 0, 24))
        );
        assert_eq!(Rect::from_xywh(i32::MAX, 0, 10, 10), None);
        assert_eq!(Rect::from_xywh(10, 10, -10, 5), None);
    }
    #[test]
    fn complete_last_button_not_stretching_container() {
        let buttons = [r(-190, 0, -150, 40), r(-150, 0, -85, 40)];
        assert_eq!(buttons[0].union(buttons[1]), r(-190, 0, -85, 40));
    }
    #[test]
    fn reservations_horizontal_vertical_and_middle() {
        assert_eq!(
            available(
                r(-1000, 0, 0, 40),
                &[r(-1000, 0, -900, 40), r(-80, 0, 0, 40)],
                Some(r(-850, 0, -700, 40)),
                false
            ),
            Some(r(-900, 0, -80, 40))
        );
        assert_eq!(
            available(
                r(-40, -900, 0, 0),
                &[r(-40, -900, 0, -850), r(-40, -100, 0, 0)],
                Some(r(-40, -800, 0, -600)),
                true
            ),
            Some(r(-40, -850, 0, -100))
        );
        assert_eq!(
            available(r(0, 0, 900, 40), &[r(400, 0, 500, 40)], None, false),
            None
        );
        assert_eq!(
            available(
                r(0, 0, 900, 40),
                &[r(400, 0, 500, 40)],
                Some(r(450, 0, 600, 40)),
                false
            ),
            None
        );
    }
    #[test]
    fn filtering_excludes_regions_and_hidden_bits() {
        for r in [0x2b, 0x2c, 0x2d, 0x38, 0x39, 0x3a, 0x3e] {
            assert!(is_button(r));
        }
        for r in [0x16, 0x10, 0x0a, 0x29, 0x2a, 0x1e] {
            assert!(!is_button(r));
        }
        assert!(is_hidden(0x108000));
        assert!(is_hidden(0x10000));
        assert!(!is_hidden(0x100008)); // focused/pressed/disabled still occupy space
    }
}
