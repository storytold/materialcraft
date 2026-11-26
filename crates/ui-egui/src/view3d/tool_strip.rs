//! The strip of buttons under the 3D view's menus: the scene browser, the tools, what lights and
//! raises the mesh, and the helpers drawn over it

use egui::Ui;
use egui_phosphor::regular as icon;
use materialcraft_engine::View3d;
use materialcraft_engine::geom::view::Background;
use serde_json::json;

use super::{Subject, Tool};
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const BUTTON_SIDE: f32 = 22.0;
/// Width of the pop-up with the displacement sliders
const DISPLACEMENT_WIDTH: f32 = 220.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        // A narrow view wraps the strip onto a second row instead of growing past its panel
        ui.horizontal_wrapped(|ui| {
            let browser = app.view3d.scene_browser;
            if crate::widgets::icon_button(ui, icon::TREE_VIEW, tr("Scene browser"), browser, BUTTON_SIDE).clicked() {
                app.view3d.scene_browser = !browser;
            }
            tool(app, ui, icon::CURSOR, "Select: click the mesh (Shift+click with any tool)", Tool::Select);
            tool(app, ui, icon::VIDEO_CAMERA, "Camera: drag to orbit, pan and zoom (C)", Tool::Camera);
            tool(app, ui, icon::LIGHTBULB, "Light: drag to move the first point light, which switches it on (L)", Tool::Light);
            ui.separator();

            let view = app.session.view3d.clone();
            let environment_shown = view.background == Background::Environment;
            if crate::widgets::icon_button(ui, icon::GLOBE_HEMISPHERE_WEST, tr("Show the environment behind the mesh"), environment_shown, BUTTON_SIDE)
                .clicked()
            {
                let background = if environment_shown { Background::default() } else { Background::Environment };
                app.run("view.3d", json!({"background": background.id()}));
            }
            if crate::widgets::icon_button(ui, icon::SLIDERS_HORIZONTAL, tr("Renderer settings"), false, BUTTON_SIDE).clicked() {
                super::inspect(app, Subject::Renderer);
            }
            setting(app, ui, icon::SUN_HORIZON, "The point lights cast shadows", "shadows", view.shadows);
            displacement(app, ui, &view);
            ui.separator();

            let hidden = app.view3d.display_tools_hidden;
            if crate::widgets::icon_button(ui, icon::EYE, tr("Display settings"), !hidden, BUTTON_SIDE).clicked() {
                app.view3d.display_tools_hidden = !hidden;
            }
            if !hidden {
                setting(app, ui, icon::GRID_FOUR, "Grid", "grid", view.show_grid);
                setting(app, ui, icon::LIST_NUMBERS, "Scene stats", "sceneStats", view.scene_stats);
                setting(app, ui, icon::VECTOR_THREE, "Axis", "axis", view.show_axis);
                setting(app, ui, icon::CUBE_TRANSPARENT, "Backface culling", "backfaceCulling", view.backface_culling);
                setting(app, ui, icon::BOUNDING_BOX, "Bounding box", "boundingBox", view.bounding_box);
                setting(app, ui, icon::ARROWS_OUT_CARDINAL, "Vertex tangent space", "vertexTangentSpace", view.vertex_tangent_space);
                setting(app, ui, icon::POLYGON, "Wireframe", "wireframe", view.wireframe);
                setting(app, ui, icon::LIGHTBULB_FILAMENT, "Light", "lightHelpers", view.light_helpers);
            }
        });
    });
}

/// A button that makes `tool` the one dragging uses
fn tool(app: &mut MaterialCraftApp, ui: &mut Ui, icon: &str, tooltip: &str, tool: Tool) {
    if crate::widgets::icon_button(ui, icon, tr(tooltip), app.view3d.tool == tool, BUTTON_SIDE).clicked() {
        app.view3d.tool = tool;
    }
}

/// A toggle button for one boolean `view.3d` setting
fn setting(app: &mut MaterialCraftApp, ui: &mut Ui, icon: &str, tooltip: &str, parameter: &str, on: bool) {
    if crate::widgets::icon_button(ui, icon, tr(tooltip), on, BUTTON_SIDE).clicked() {
        app.run("view.3d", json!({parameter: !on}));
    }
}

/// The button that opens the displacement sliders: how far the height output raises the mesh,
/// which height stays on the surface and how finely the mesh is subdivided
fn displacement(app: &mut MaterialCraftApp, ui: &mut Ui, view: &View3d) {
    let response = crate::widgets::icon_button(ui, icon::MOUNTAINS, tr("Displacement"), view.displacement, BUTTON_SIDE);
    egui::Popup::menu(&response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(DISPLACEMENT_WIDTH);

        let mut enabled = view.displacement;
        if ui.checkbox(&mut enabled, tr("Raise the mesh by the height output")).changed() {
            app.run("view.3d", json!({"displacement": enabled}));
        }
        let mut scale = view.height_scale;
        if crate::widgets::slider(ui, tr("Height scale"), &mut scale, View3d::HEIGHT_SCALE, 3).changed() {
            app.run("view.3d", json!({"heightScale": scale}));
        }
        let mut level = view.height_level;
        if crate::widgets::slider(ui, tr("Height level"), &mut level, 0.0..=1.0, 2).changed() {
            app.run("view.3d", json!({"heightLevel": level}));
        }
        let mut tessellation = view.tessellation;
        if crate::widgets::slider(ui, tr("Tessellation"), &mut tessellation, View3d::TESSELLATION, 0).changed() {
            app.run("view.3d", json!({"tessellation": tessellation.round()}));
        }
    });
}
