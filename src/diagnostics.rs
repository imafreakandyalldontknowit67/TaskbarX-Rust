use crate::runtime::{self, Backend};
use serde_json::{json, Value};
use taskbarx_config::Settings;
use taskbarx_win32::Rect;

fn rect(r: &Rect) -> Value {
    json!({"left": r.left, "top": r.top, "right": r.right, "bottom": r.bottom})
}

/// No instance-control objects, restoration calls, or mutating methods here.
/// Dropping the untouched native Session is read-only by backend contract.
pub(crate) fn report(backend: &mut impl Backend, settings: &Settings) -> Result<Value, String> {
    settings.validate()?;
    backend.set_measurement_backend(runtime::measurement(&settings.measurement)?)?;
    let discovery = backend.discover()?;
    let taskbars: Vec<_> = discovery.taskbars.iter().map(|s| {
        let proposal = if discovery.native_windows11 { None } else { runtime::proposal(s, settings) };
        let proposed = proposal.map(|(_, target, (min, max))| json!({
            "position": if s.vertical { json!({"x": s.position.x, "y": target}) } else { json!({"x": target, "y": s.position.y}) },
            "legal_min": min, "legal_max": max,
        }));
        let measurement = runtime::layout(s).map(|a| json!({
            "bar_start": a.bar_start, "bar_end": a.bar_end, "free_start": a.free_start,
            "free_end": a.free_end, "parent_start": a.parent_start,
            "content_offset": a.content_offset, "content_len": a.content_len,
        }));
        json!({
            "id": {"root": s.id.root, "task_list": s.id.task_list, "parent": s.id.parent,
                "process_id": s.id.process_id, "process_started": s.id.process_started},
            "primary": s.primary, "selected": runtime::selected(s, settings), "vertical": s.vertical, "dpi": s.dpi,
            "taskbar_rect": rect(&s.taskbar_rect), "parent_rect": rect(&s.parent_rect),
            "list_rect": rect(&s.list_rect), "available_rect": rect(&s.available_rect),
            "content_rect": s.content_rect.as_ref().map(rect),
            "position": {"x": s.position.x, "y": s.position.y},
            "axis_layout": measurement, "proposed_target": proposed,
            "reason": if discovery.native_windows11 { "unsupported modern Windows 11 taskbar" }
                else if !runtime::selected(s, settings) { "not selected" }
                else if !settings.center { "centering disabled" }
                else if proposed.is_none() { "empty or unsafe measurement; run would restore any previously moved position" }
                else { "safe proposed target; diagnostic made no changes" },
        })
    }).collect();
    let mut warnings = discovery.warnings;
    if discovery.native_windows11 {
        warnings.push(runtime::MODERN_TASKBAR_ERROR.to_owned());
    }
    if taskbars.is_empty() {
        warnings.push("No classic taskbars discovered; Explorer may be absent.".to_owned());
    }
    Ok(json!({
        "read_only": true,
        "coordinate_space": "signed physical screen pixels; positions/targets are parent-client-relative",
        "os": {"build_number": discovery.build_number, "native_windows11": discovery.native_windows11,
            "compatible": !discovery.native_windows11 && !taskbars.is_empty()},
        "taskbars": taskbars, "warnings": warnings,
    }))
}
