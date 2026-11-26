mod comment_properties;
mod frame_properties;
mod pin_properties;

use crate::MaterialCraftApp;
use craft_behavior::canvas::CanvasView;
use egui::Ui;
use materialcraft_engine::AnnotationSelection;

pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, selection: AnnotationSelection) {
    match selection {
        AnnotationSelection::Frame(id) => frame_properties::show(app, ui, id),
        AnnotationSelection::Comment(id) => comment_properties::show(app, ui, id),
        AnnotationSelection::Pin(id) => pin_properties::show(app, ui, id),
    }
}

/// Resolve the destination before an editor is temporarily borrowed for drawing.
pub(crate) fn target(app: &MaterialCraftApp) -> materialcraft_engine::AnnotationTarget {
    use materialcraft_engine::{AnnotationTarget, FunctionGraphTarget};
    if let Some(target) = app.function_editor.as_ref().and_then(|editor| editor.command_target()) {
        return match target {
            FunctionGraphTarget::Graph => AnnotationTarget::Graph,
            target => AnnotationTarget::Function { target },
        };
    }
    if crate::pattern::active(app)
        && let Some(editor) = &app.pattern_editor
    {
        return AnnotationTarget::Pattern { owner: editor.node };
    }
    AnnotationTarget::Graph
}

pub(crate) fn activate(app: &mut MaterialCraftApp, target: materialcraft_engine::AnnotationTarget) {
    if app.graph.annotation_target != target {
        app.session.annotation_selection = None;
        // Also closes the search, which adds to the editor it opened over; the canvas calls this every pass, so only a
        // change of editor closes it.
        // PLAN: `GraphView::search` is `pub(super)`; this relies on graph-w6's `cancel_drag` keeping `search = None`.
        app.graph.cancel_drag();
    }
    app.graph.annotation_target = target;
}

/// Sets the active editor's live view: the Function or FX-Map editor's own, else the material graph's saved camera.
pub(crate) fn set_camera(app: &mut MaterialCraftApp, view: CanvasView) {
    if let Some(editor) = &mut app.function_editor {
        editor.view = view;
    } else if let Some(editor) = &mut app.pattern_editor {
        editor.view = view;
    } else {
        app.ui.graph_camera = crate::state::GraphCamera::from_view(&view);
    }
}

/// Read the active editor's live view without replacing the parent graph's saved camera.
pub(crate) fn camera(app: &MaterialCraftApp) -> CanvasView {
    if let Some(editor) = &app.function_editor {
        editor.view
    } else if let Some(editor) = &app.pattern_editor {
        editor.view
    } else {
        app.ui.graph_camera.view(app.graph.view)
    }
}
