//! The window's own frame where the desktop leaves it to the app (GNOME on Wayland): which frame the runtime has, and
//! the resize zones along the window's edges. The menu bar is the title bar there (`menus::menu_bar`).

use craft_ui::runtime::windows::WindowHost;
use craft_window::policy::Frame;
use egui::viewport::ResizeDirection;
use egui::{CursorIcon, Id, LayerId, Order, PointerButton, Rect, Sense, Ui, ViewportCommand, pos2, vec2};

/// Whether the app's menu bar is the window's title bar: the frame the runtime has for the window is the app's.
pub(crate) fn app_bar_is_title_bar(ctx: &egui::Context) -> bool {
    !matches!(WindowHost::of(ctx).frame(), Frame::Native)
}

/// Whether the window has no frame of the system's, so the app gives it its resize edges (macOS keeps its own edges
/// under an inset title bar).
pub(crate) fn app_draws_frame(ctx: &egui::Context) -> bool {
    matches!(WindowHost::of(ctx).frame(), Frame::Integrated(_))
}

const EDGE: f32 = 5.0;
const CORNER: f32 = 12.0;

/// The edge and corner zones of a window with content rect `w`; corners come last and win where they overlap the edges
fn zones(w: Rect) -> [(Rect, ResizeDirection, CursorIcon); 8] {
    use ResizeDirection::*;
    [
        (Rect::from_min_max(w.left_top(), pos2(w.right(), w.top() + EDGE)), North, CursorIcon::ResizeVertical),
        (Rect::from_min_max(pos2(w.left(), w.bottom() - EDGE), w.right_bottom()), South, CursorIcon::ResizeVertical),
        (Rect::from_min_max(w.left_top(), pos2(w.left() + EDGE, w.bottom())), West, CursorIcon::ResizeHorizontal),
        (Rect::from_min_max(pos2(w.right() - EDGE, w.top()), w.right_bottom()), East, CursorIcon::ResizeHorizontal),
        (Rect::from_min_size(w.left_top(), vec2(CORNER, CORNER)), NorthWest, CursorIcon::ResizeNwSe),
        (Rect::from_min_size(w.right_top() - vec2(CORNER, 0.0), vec2(CORNER, CORNER)), NorthEast, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(w.left_bottom() - vec2(0.0, CORNER), vec2(CORNER, CORNER)), SouthWest, CursorIcon::ResizeNeSw),
        (Rect::from_min_size(w.right_bottom() - vec2(CORNER, CORNER), vec2(CORNER, CORNER)), SouthEast, CursorIcon::ResizeNwSe),
    ]
}

/// Invisible resize zones along the window's edges, above everything else; none while the window is maximized or full screen
pub(crate) fn resize_zones(ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let (maximized, fullscreen) = ctx.input(|input| (input.viewport().maximized.unwrap_or(false), input.viewport().fullscreen.unwrap_or(false)));
    if maximized || fullscreen {
        return;
    }
    let window = ctx.content_rect();
    let layer = LayerId::new(Order::Foreground, Id::unique("window-resize-zones"));
    let zones_ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(window));
    for (index, (rect, direction, cursor)) in zones(window).into_iter().enumerate() {
        let response = zones_ui.interact(rect, Id::unique(("window-resize-zone", index)), Sense::DRAG).on_hover_cursor(cursor);
        if response.drag_started_by(PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
            // The window manager takes the pointer and on some desktops its release as well
            ctx.stop_dragging();
            ctx.input_mut(|input| input.pointer = Default::default());
        }
    }
}

#[cfg(test)]
#[path = "window_controls.test.rs"]
mod tests;
