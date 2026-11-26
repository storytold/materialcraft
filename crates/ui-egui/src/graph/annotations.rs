//! The annotations of the active editor: backdrops, notes and markers on the framework node graph.

use craft_ui::graph::{AnnotationEvent, AnnotationKey};
use egui::Rect;

use crate::MaterialCraftApp;

/// The owned annotation items of the active editor and the selected key.
#[allow(dead_code)] // P3(graph-w10): ge.A9
pub(crate) struct AnnotationLayers {
    /// The selected annotation, at most one.
    pub(crate) selected: Option<AnnotationKey>,
}

/// The backdrops, notes and markers of the active editor's annotation target.
#[allow(dead_code)] // P3(graph-w10): ge.A9
pub(crate) fn layers(app: &MaterialCraftApp) -> AnnotationLayers {
    let _ = app;
    todo!("ge.A9")
}

/// Applies the annotation events: select, move, edit, then `finish_edit`.
#[allow(dead_code)] // P3(graph-w10): ge.A9
pub(crate) fn apply(app: &mut MaterialCraftApp, events: &[AnnotationEvent]) {
    let _ = (app, events);
    todo!("ge.A9")
}

/// The frames', comments' and pins' rects in screen points, for `GraphView::publish`.
#[allow(dead_code)] // P3(graph-w10): ge.A9
pub(crate) fn screen_rects(layers: &AnnotationLayers, view: &craft_behavior::canvas::CanvasView) -> (Vec<(u64, Rect)>, Vec<(u64, Rect)>, Vec<(u64, Rect)>) {
    let _ = (layers, view);
    todo!("ge.A9")
}
