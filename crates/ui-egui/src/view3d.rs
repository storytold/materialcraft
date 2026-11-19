//! The 3D view: the assigned material channels on a mesh, rendered by `materialcraft-render`.
//! A menu bar (Scene, Materials, Lights, Camera, Environment, Display, Renderer), a strip of tool
//! and display buttons, the picture with its helpers drawn over it, and a bar with the channels,
//! the color space and the scene. The parts of the view (material, lights, camera, environment,
//! renderer) open in the Properties panel.
//!
//! Dragging orbits the camera, the middle button pans, the wheel and the right button zoom; see
//! [`navigation`] for the rest. The picture is kept up to date by [`frame`].

mod display_bar;
mod frame;
mod frame_job;
mod frame_scene;
mod menus;
mod navigation;
mod output;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use output::displayed_mesh;
mod overlay;
mod overlay_line;
mod overlay_mark;
mod overlays;
pub(crate) mod properties;
mod relief_job;
mod scene_browser;
mod sight;
mod subject;
mod tool;
mod tool_strip;
mod view3d_state;

use std::hash::{DefaultHasher, Hash, Hasher};

use egui::{Color32, Rect, Sense, Ui, pos2, vec2};
use materialcraft_engine::view_cmds::SCENES;
use serde_json::json;

pub use frame::SETTLE;
pub use subject::Subject;
pub use tool::Tool;
pub use view3d_state::View3dState;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Height of the bar under the picture: its controls and the margin around them
const DISPLAY_BAR_HEIGHT: f32 = 26.0;
const MIN_PICTURE_HEIGHT: f32 = 40.0;
/// The narrowest the picture gets beside the scene browser
const MIN_PICTURE_WIDTH: f32 = 160.0;
/// What shows around a picture that does not fill the view
const SURROUND: Color32 = Color32::from_rgb(0x1b, 0x1c, 0x1e);

/// The name of the shown mesh: a loaded mesh's file name, else the built-in scene's label
pub fn scene_label(app: &MaterialCraftApp) -> String {
    let id = &app.session.view3d.scene;
    if id == "custom"
        && let Some((name, _)) = &app.session.custom_mesh
    {
        return name.clone();
    }

    SCENES.iter().find(|(scene, _)| scene == id).map_or(id.as_str(), |(_, label)| label).to_string()
}

/// What the Properties panel shows besides the 3D view: when it changes, the panel goes back to it
fn panel_context(app: &MaterialCraftApp) -> u64 {
    let mut hasher = DefaultHasher::new();
    let session = &app.session;
    (session.document_id, session.graph, &session.selection, app.graph.inspected_node, &app.inspected_folder).hash(&mut hasher);
    format!("{:?} {:?}", app.inspected_graph, session.annotation_selection).hash(&mut hasher);
    (app.function_editor.is_some(), app.pattern_editor.is_some()).hash(&mut hasher);
    app.function_editor.as_ref().map(|editor| (&editor.selection, editor.inspected_node)).hash(&mut hasher);
    app.pattern_editor.as_ref().map(|editor| (&editor.selection, editor.inspected_node)).hash(&mut hasher);
    hasher.finish()
}

/// Show a part of the 3D view in the Properties panel, until something else is selected
pub(crate) fn inspect(app: &mut MaterialCraftApp, subject: Subject) {
    app.view3d.inspected = Some((subject, panel_context(app)));
    app.ui.dock.focus(crate::docking::Panel::Properties);
}

/// The part of the 3D view the Properties panel shows, if it still shows one
pub(crate) fn inspected(app: &MaterialCraftApp) -> Option<Subject> {
    let (subject, context) = app.view3d.inspected?;
    (context == panel_context(app)).then_some(subject)
}

pub(crate) fn clear_stale_inspection(app: &mut MaterialCraftApp) {
    if app.view3d.inspected.is_some_and(|(_, context)| context != panel_context(app)) {
        app.view3d.inspected = None;
    }
}

pub(super) fn frame_view(app: &mut MaterialCraftApp) {
    let view = &app.session.view3d;
    let [width, height] = if view.override_resolution && !view.viewport_resolution {
        view.render_resolution
    } else {
        app.view3d.size.map_or([1, 1], |(width, height)| [width, height])
    };
    let aspect = width as f32 / height.max(1) as f32;
    app.run("view.3d.frame", json!({"aspect": aspect}));
}

/// Make the current view the one a reset and a new document start from, now and after a restart
fn save_default(app: &mut MaterialCraftApp) {
    if app.run("view.3d.saveDefault", json!({})).is_some() {
        app.ui.prefs.default_view3d = app.session.default_view3d.clone();
        app.toast(tr("Saved as the default 3D view"), false);
    }
}

/// Split the scene browser and rendered viewport along the current reading edge.
/// The scene itself is never mirrored, only the surrounding panel geometry.
fn view3d_scene_regions(below: Rect, bar_top: f32, browser_width: f32, rtl: bool) -> (Rect, Rect) {
    let split = if rtl { below.right() - browser_width } else { below.left() + browser_width };
    if rtl {
        (Rect::from_min_max(pos2(split, below.top()), pos2(below.right(), bar_top)), Rect::from_min_max(pos2(below.left(), below.top()), pos2(split, bar_top)))
    } else {
        (Rect::from_min_max(pos2(below.left(), below.top()), pos2(split, bar_top)), Rect::from_min_max(pos2(split, below.top()), pos2(below.right(), bar_top)))
    }
}

#[cfg(test)]
mod hebrew_scene_browser_layout_tests {
    use super::*;

    #[test]
    fn scene_browser_mirrors_while_picture_keeps_its_full_remaining_width() {
        let bounds = Rect::from_min_size(pos2(20.0, 30.0), vec2(500.0, 300.0));
        for width in [0.0, 48.0, 176.0] {
            let (en_browser, en_picture) = view3d_scene_regions(bounds, 310.0, width, false);
            let (he_browser, he_picture) = view3d_scene_regions(bounds, 310.0, width, true);
            assert_eq!(en_browser.left(), bounds.left());
            assert_eq!(en_browser.right(), en_picture.left());
            assert_eq!(he_picture.right(), he_browser.left());
            assert_eq!(he_browser.right(), bounds.right());
            for (browser, picture) in [(en_browser, en_picture), (he_browser, he_picture)] {
                assert_eq!(browser.width(), width);
                assert_eq!(picture.width(), bounds.width() - width);
                assert_eq!(browser.top(), bounds.top());
                assert_eq!(picture.top(), bounds.top());
                assert_eq!(picture.bottom(), 310.0);
            }
        }
    }
}

pub fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let panel = ui.max_rect();
    menus::show(app, ui);
    tool_strip::show(app, ui);

    // Exact rectangles inside the panel: the bar ends at the view's edge and a wide menu bar widens nothing
    let below = ui.available_rect_before_wrap();
    let below = Rect::from_min_max(pos2(panel.left(), below.top()), pos2(panel.right(), below.bottom()));
    let bar_top = (below.bottom() - DISPLAY_BAR_HEIGHT).max(below.top() + MIN_PICTURE_HEIGHT.min(below.height()));
    let browser_width = if app.view3d.scene_browser { scene_browser::WIDTH.min(below.width() - MIN_PICTURE_WIDTH).max(0.0) } else { 0.0 };

    let (browser, picture) = view3d_scene_regions(below, bar_top, browser_width, crate::i18n::is_rtl());
    if browser_width > 0.0 {
        let mut browser_ui = ui.new_child(egui::UiBuilder::new().max_rect(browser.shrink2(vec2(4.0, 0.0))));
        browser_ui.set_clip_rect(browser.intersect(ui.clip_rect()));
        scene_browser::show(app, &mut browser_ui);
    }

    let mut picture_ui = ui.new_child(egui::UiBuilder::new().max_rect(picture));
    viewport(app, &mut picture_ui, picture);

    let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(pos2(below.left(), bar_top), below.max)));
    display_bar::show(app, &mut bar_ui);
    ui.advance_cursor_after_rect(below);
}

/// The picture: the rendered frame, what is drawn over it, and the gestures on it
fn viewport(app: &mut MaterialCraftApp, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let response = ui.allocate_rect(rect, Sense::click_and_drag());
    crate::widgets::name(&response, egui::Role::Image, "3D view");

    let graph_hover = response.dnd_hover_payload::<crate::graph_drag_payload::GraphDragPayload>().is_some();
    if let Some(graph) = crate::widgets::dnd_release::<crate::graph_drag_payload::GraphDragPayload>(&response)
        && (graph.document == app.session.document_id || app.run("package.activate", json!({"document": graph.document})).is_some())
    {
        app.run("view.3d.outputs", json!({"graph": graph.name}));
    }

    let moving = navigation::handle(app, ui, &response, frame::shown_rect(app, rect));
    frame::render(app, ui.ctx(), rect, moving || response.is_pointer_button_down_on());

    let painter = ui.painter_at(rect);
    let area = frame::shown_rect(app, rect);
    painter.rect_filled(rect, 0.0, SURROUND);
    if let Some(texture) = &app.view3d.tex {
        painter.image(texture.id(), area, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    overlays::paint(app, ui, rect, area);

    let notice = egui::FontId::proportional(11.0);
    if app.session.view3d.alpha_view {
        painter.text(rect.center_top() + vec2(0.0, 6.0), egui::Align2::CENTER_TOP, tr("Alpha channel"), notice.clone(), tokens.viewport_text);
    }
    if app.session.material_maps().is_empty() {
        let hint = tr("Assign an image with View in 3D view, or connect Output nodes with material usages");
        painter.text(rect.center_top() + vec2(0.0, 20.0), egui::Align2::CENTER_TOP, hint, notice, tokens.viewport_text);
    }
    if graph_hover {
        painter.rect_stroke(rect.shrink(2.0), 4.0, egui::Stroke::new(2.0, tokens.accent), egui::StrokeKind::Inside);
        painter.text(
            rect.center_top() + vec2(0.0, 38.0),
            egui::Align2::CENTER_TOP,
            tr("Drop to apply graph outputs"),
            egui::FontId::proportional(13.0),
            tokens.viewport_text,
        );
    }
}

#[cfg(test)]
mod inspection_tests {
    use super::*;
    use materialcraft_engine::core::NodeId;

    #[test]
    fn another_selection_discards_the_previous_3d_inspection() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        inspect(&mut app, Subject::Camera);

        let previous = app.session.selection.clone();
        app.session.selection = vec![NodeId(u64::MAX)];
        app.reconcile_editors();
        app.session.selection = previous;

        assert_eq!(inspected(&app), None);
    }

    #[test]
    fn pattern_selection_discards_the_previous_3d_inspection() {
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
        app.pattern_editor = Some(crate::pattern::PatternEditor::new(app.session.graph, NodeId(0)));
        inspect(&mut app, Subject::Camera);
        assert_eq!(inspected(&app), Some(Subject::Camera));

        if let Some(editor) = &mut app.pattern_editor {
            editor.selection.push(1);
            editor.inspected_node = Some(1);
        }

        assert_eq!(inspected(&app), None);
    }
}
