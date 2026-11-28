//! The graph editor's canvas widget and its keyboard focus

use crate::MaterialCraftApp;
use crate::framework::{canvas_id, focus_canvas, focused_widget, host_canvas, overlay_open};
use crate::shell::modality::Modality;
use egui::{Id, PointerButton, Popup, Rect, Response, Sense, SetOpenCommand, Ui, Vec2};

/// The passes a graph that opens asks for the focus in: the widget that opened it keeps egui's focus for up to two
/// passes, the one it was last drawn in and the one after it went away.
const FOCUS_PASSES: u8 = 3;

/// Allocate and draw the canvas widget: every editor (material, function, FX-Map) draws the one that has the focus
pub(crate) fn allocate(app: &MaterialCraftApp, ui: &mut Ui, size: Vec2) -> (Rect, Response) {
    let (_, rect) = ui.allocate_space(size);
    let response = ui.interact(rect, canvas_id(), Sense::click_and_drag());
    // Declared before the widgets on the canvas, for its place in Tab order
    host_canvas(ui.ctx(), false);
    focus_on_press(app, ui, &response);
    (rect, response)
}

/// A press anywhere on the canvas gives it the keyboard focus. Real presses only: with the canvas focused, egui makes
/// Enter and Space clicks of it.
fn focus_on_press(app: &MaterialCraftApp, ui: &Ui, canvas: &Response) {
    let ctx = ui.ctx();
    if !ui.input(|input| input.pointer.any_pressed()) || !ui.rect_contains_pointer(canvas.rect) || Modality::capture(app, ctx).is_blocked() {
        return;
    }

    // A press in the value being edited on a node stays there
    let in_editor = ctx.text_edit_focused() && focused_widget(ctx).and_then(|id| ctx.read_response(id)).is_some_and(|field| field.is_pointer_button_down_on());
    if !in_editor {
        focus_canvas(ctx);
    }
}

/// The canvas's context menu. Real clicks only, as for [`focus_on_press`]: Enter picks in the Add node menu and must not
/// close it.
pub(crate) fn context_menu(canvas: &Response, contents: impl FnOnce(&mut Ui)) {
    let open = if canvas.secondary_clicked() {
        Some(SetOpenCommand::Bool(true))
    } else if canvas.clicked_by(PointerButton::Primary) {
        Some(SetOpenCommand::Bool(false))
    } else {
        None
    };
    Popup::context_menu(canvas).open_memory(open).show(contents);
}

/// Whether the pointer is over the canvas: `hovered`, or over the node search `search`, which still covers the canvas in
/// the passes after it closes
pub(crate) fn under_pointer(ui: &Ui, canvas: &Response, hovered: bool, search: Id) -> bool {
    let ctx = ui.ctx();
    hovered
        || ctx.pointer_interact_pos().is_some_and(|pointer| canvas.rect.contains(pointer) && ctx.layer_id_at(pointer).is_some_and(|layer| layer.id == search))
}

/// Settle the focus and Tab once the canvas has handled its input. A graph that opens or becomes active asks for the
/// focus, and so does the canvas while nothing is focused and Tab opens its node search (`opens_search`). It waits for a
/// dialog, menu or popup to close, and leaves the focus with a text field and, after [`FOCUS_PASSES`], with a widget
/// of another window.
pub(crate) fn settle(app: &mut MaterialCraftApp, ui: &Ui, opens_search: bool) {
    let ctx = ui.ctx();
    let opens_search = opens_search && app.ui.prefs.tab_opens_node_search && !Modality::capture(app, ctx).is_blocked();
    let shown = Some((app.session.document_id, app.session.graph, app.graph.annotation_target.clone()));
    if app.graph.shown != shown {
        app.graph.shown = shown;
        app.graph.focus_passes = FOCUS_PASSES;
    }

    let focused = ctx.memory(|memory| memory.focused());
    let unfocused = opens_search && focused.is_none() && !ui.input(|input| input.pointer.any_down());
    if (app.graph.focus_passes > 0 || unfocused) && !overlay_open(app, ctx) {
        let same_window = focused.is_none_or(|id| ctx.read_response(id).is_some_and(|widget| widget.layer_id == ui.layer_id()));
        if ctx.text_edit_focused() {
            app.graph.focus_passes = 0;
        } else if same_window {
            app.graph.focus_passes = 0;
            focus_canvas(ctx);
        } else {
            app.graph.focus_passes = app.graph.focus_passes.saturating_sub(1);
            ctx.request_repaint();
        }
    }
    host_canvas(ctx, opens_search);
}
