// Adapted from TaskbarX/TaskbarX/TaskbarCenter.vb.
// Copyright (c) 2021 Chris Andriessen. MIT license: UPSTREAM_LICENSE.txt.

/// Geometry along one axis, in physical pixels (x for horizontal, y for vertical).
///
/// Screen spans are half-open: `[start, end)`. Primary and secondary taskbars use
/// the same calculation, including monitors left of or above the primary monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisLayout {
    /// Taskbar's screen-space start.
    pub bar_start: i32,
    /// Taskbar's exclusive screen-space end.
    pub bar_end: i32,
    /// Start of the unobstructed screen-space span, within the taskbar.
    pub free_start: i32,
    /// Exclusive end of the unobstructed screen-space span, within the taskbar.
    pub free_end: i32,
    /// Screen origin of the task-list window's immediate parent client area.
    pub parent_start: i32,
    /// Signed offset of the first occupied button from the task-list window origin.
    pub content_offset: i32,
    /// Occupied length from the first button's start through the last button's end.
    /// Includes inter-button gaps and the entire last button, but not leading margin.
    pub content_len: i32,
}

/// Inclusive range of legal task-list window positions relative to its parent.
///
/// The entire occupied span must lie inside the free span. Leading window margin
/// need not lie inside it. Negative parent-relative positions are valid. The
/// mathematical range is intersected with `i32` so every returned position is
/// representable; `None` means no representable legal position exists.
///
/// Also returns `None` for empty/reversed spans, a free span outside the taskbar,
/// nonpositive content length, or content longer than the free span. All arithmetic
/// is widened before addition/subtraction, including differences of screen origins.
pub fn legal_position_range(layout: &AxisLayout) -> Option<(i32, i32)> {
    if layout.bar_start >= layout.bar_end
        || layout.free_start >= layout.free_end
        || layout.free_start < layout.bar_start
        || layout.free_end > layout.bar_end
        || layout.content_len <= 0
    {
        return None;
    }

    let free_start = i64::from(layout.free_start);
    let free_end = i64::from(layout.free_end);
    let content_len = i64::from(layout.content_len);
    if content_len > free_end - free_start {
        return None;
    }

    // occupied_start = parent_start + window_position + content_offset
    let origin = i64::from(layout.parent_start) + i64::from(layout.content_offset);
    let min = (free_start - origin).max(i64::from(i32::MIN));
    let max = (free_end - origin - content_len).min(i64::from(i32::MAX));
    if min > max {
        return None;
    }
    Some((i32::try_from(min).ok()?, i32::try_from(max).ok()?))
}

/// Centers occupied content on the full taskbar, or on the free span if `between`.
///
/// Applies the signed pixel `offset` *before* clamping to [`legal_position_range`].
/// Returns the task-list window position relative to its immediate parent, not a
/// screen coordinate. Half-pixel centers round toward the span's start, identically
/// on positive and negative monitors. See [`legal_position_range`] for failures.
pub fn center_target(layout: &AxisLayout, offset: i32, between: bool) -> Option<i32> {
    let (min, max) = legal_position_range(layout)?;
    let target = requested_center(layout, offset, between)?;
    i32::try_from(target.clamp(i64::from(min), i64::from(max))).ok()
}

/// Unclamped parent-relative request, widened so offsets cannot wrap. This is
/// NOT a legal movement destination. Use it only for policy decisions before
/// clamping (in particular, testing an already-clamped target hides tray overlap).
pub fn requested_center(layout: &AxisLayout, offset: i32, between: bool) -> Option<i64> {
    legal_position_range(layout)?;
    let (start, end) = if between {
        (layout.free_start, layout.free_end)
    } else {
        (layout.bar_start, layout.bar_end)
    };
    let start = i64::from(start);
    let span = i64::from(end) - start;
    let occupied_start = start + (span - i64::from(layout.content_len)) / 2;
    Some(
        occupied_start - i64::from(layout.parent_start) - i64::from(layout.content_offset)
            + i64::from(offset),
    )
}

/// Optional upstream "revert beyond tray" behavior, without its unsafe zero
/// write. If the requested occupied span crosses the trailing reservation,
/// align its leading edge with the first representable legal position instead.
/// Touching the half-open boundary is safe; crossing it is not. Works on either
/// axis and on negative monitors. With fallback disabled, preserves center_target.
pub fn center_target_with_overlap(
    layout: &AxisLayout,
    offset: i32,
    between: bool,
    revert_on_overlap: bool,
) -> Option<i32> {
    let (min, max) = legal_position_range(layout)?;
    let request = requested_center(layout, offset, between)?;
    let occupied_end = i64::from(layout.parent_start)
        + request
        + i64::from(layout.content_offset)
        + i64::from(layout.content_len);
    if revert_on_overlap && occupied_end > i64::from(layout.free_end) {
        Some(min)
    } else {
        i32::try_from(request.clamp(i64::from(min), i64::from(max))).ok()
    }
}
