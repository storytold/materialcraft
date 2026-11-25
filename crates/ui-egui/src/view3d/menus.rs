//! The 3D view's menu bar: Scene, Materials, Lights, Camera, Environment, Display and Renderer

use egui::Ui;
use materialcraft_engine::Viewpoint;
use materialcraft_engine::geom::view::{HeightTechnique, MeshInfo, Shader};
use materialcraft_engine::view_cmds::{PLAIN_SCENES, SCENES};
use serde_json::json;

use super::Subject;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// The menu bar itself defaults to LTR even when the app language is Hebrew.
/// Override that inner layout so the first menu is anchored on the right in RTL.
fn with_view3d_menu_direction<R>(ui: &mut Ui, rtl: bool, content: impl FnOnce(&mut Ui) -> R) -> R {
    if rtl { ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), content).inner } else { content(ui) }
}

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(4, 1)).show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            with_view3d_menu_direction(ui, crate::i18n::is_rtl(), |ui| {
                ui.menu_button(tr("Scene"), |ui| scene(app, ui));
                ui.menu_button(tr("Materials"), |ui| materials(app, ui));
                ui.menu_button(tr("Lights"), |ui| {
                    edit_properties(app, ui, Subject::Lights);
                    command(app, ui, "Reset Lights", None, "view.3d.resetLights");
                });
                ui.menu_button(tr("Camera"), |ui| camera(app, ui));
                ui.menu_button(tr("Environment"), |ui| {
                    edit_properties(app, ui, Subject::Environment);
                    command(app, ui, "Reset Environment", None, "view.3d.resetEnvironment");
                });
                // The Display menu stays open, so several helpers can be switched in one visit
                let stays_open = egui::containers::menu::MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                egui::containers::menu::MenuButton::new(tr("Display")).config(stays_open).ui(ui, |ui| display(app, ui));
                ui.menu_button(tr("Renderer"), |ui| {
                    if ui.button(tr("Edit settings")).clicked() {
                        super::inspect(app, Subject::Renderer);
                        ui.close();
                    }
                    ui.separator();
                    ui.add_enabled(false, egui::Button::selectable(true, tr("CPU ray tracer")));
                });
            });
        });
    });
}

/// A menu item that runs a command without parameters
fn command(app: &mut MaterialCraftApp, ui: &mut Ui, label: &str, shortcut: Option<&str>, id: &str) {
    let button = egui::Button::new(tr(label));
    let button = match shortcut {
        Some(keys) => button.shortcut_text(keys),
        None => button,
    };
    if ui.add(button).clicked() {
        app.run(id, json!({}));
        ui.close();
    }
}

/// The item that shows a part of the view in the Properties panel
fn edit_properties(app: &mut MaterialCraftApp, ui: &mut Ui, subject: Subject) {
    if ui.button(tr("Edit properties")).clicked() {
        super::inspect(app, subject);
        ui.close();
    }
}

/// A check box that switches one boolean `view.3d` setting
fn toggle(app: &mut MaterialCraftApp, ui: &mut Ui, label: &str, parameter: &str, mut on: bool) {
    if ui.checkbox(&mut on, tr(label)).changed() {
        app.run("view.3d", json!({parameter: on}));
    }
}

fn scene(app: &mut MaterialCraftApp, ui: &mut Ui) {
    for (index, (id, label)) in SCENES.iter().enumerate() {
        if index == PLAIN_SCENES {
            ui.separator();
        }
        if ui.add(egui::Button::selectable(app.session.view3d.scene == *id, tr(label))).clicked() {
            app.run("view.3d", json!({"scene": id}));
            ui.close();
        }
    }
    if let Some((name, _)) = &app.session.custom_mesh
        && ui.add(egui::Button::selectable(app.session.view3d.scene == "custom", name.as_str())).clicked()
    {
        app.run("view.3d", json!({"scene": "custom"}));
        ui.close();
    }
    if ui.button(tr("Load mesh...")).clicked() {
        crate::web_files::load_mesh(app, ui.ctx());
        ui.close();
    }

    ui.separator();
    let mut uv = app.session.view2d.uv;
    if ui.checkbox(&mut uv, tr("Display UVs in 2D View")).changed() {
        app.run("view.2d", json!({"uv": uv}));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        ui.separator();
        for (label, with_mesh) in [("Load State File...", false), ("Load State File With Mesh...", true)] {
            if ui.button(tr(label)).clicked() {
                super::output::load_state(app, with_mesh);
                ui.close();
            }
        }
        ui.separator();
        if ui.button(tr("Save Current State...")).clicked() {
            super::output::save_state(app);
            ui.close();
        }
    }
    if ui.button(tr("Save Current State As Default")).clicked() {
        super::save_default(app);
        ui.close();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        ui.separator();
        if ui.button(tr("Export Tessellated Geometry...")).clicked() {
            super::output::export_geometry(app);
            ui.close();
        }
    }

    ui.separator();
    command(app, ui, "Reset Scene", None, "view.3d.reset");
}

fn materials(app: &mut MaterialCraftApp, ui: &mut Ui) {
    ui.menu_button(tr("Default"), |ui| {
        edit_properties(app, ui, Subject::Material);
        ui.separator();
        for &shader in Shader::ALL {
            shader_item(app, ui, shader);
        }

        ui.separator();
        if ui.button(tr("Reset Common Parameters")).clicked() {
            app.run("view.3d.resetMaterial", json!({"common": true}));
            ui.close();
        }
        if ui.button(tr("Reset Material")).clicked() {
            app.run("view.3d.resetMaterial", json!({}));
            app.run("view.3d.clear", json!({}));
            ui.close();
        }
        command(app, ui, "Clear material assignments", None, "view.3d.clear");
    });
}

/// A shader in the Materials menu: one with height opens its techniques, the mesh information
/// shader opens what it can show
fn shader_item(app: &mut MaterialCraftApp, ui: &mut Ui, shader: Shader) {
    let view = &app.session.view3d;
    let (current, technique, mesh_info) = (view.shader == shader, view.technique, view.mesh_info);
    let label = if shader == Shader::default() { format!("{} [{}]", tr(shader.label()), tr("Default")) } else { tr(shader.label()).to_string() };
    let label = if current { format!("{} {label}", egui_phosphor::regular::CHECK) } else { label };

    if shader == Shader::MeshInfo {
        ui.menu_button(label, |ui| {
            for &shown in MeshInfo::ALL {
                if ui.add(egui::Button::selectable(current && mesh_info == shown, tr(shown.label()))).clicked() {
                    app.run("view.3d", json!({"shader": shader.id(), "meshInfo": shown.id()}));
                    ui.close();
                }
            }
        });
    } else if shader.shows_height() {
        ui.menu_button(label, |ui| {
            for &choice in HeightTechnique::ALL {
                if ui.add(egui::Button::selectable(current && technique == choice, tr(choice.label()))).clicked() {
                    app.run("view.3d", json!({"shader": shader.id(), "technique": choice.id()}));
                    ui.close();
                }
            }
        });
    } else if ui.button(label).clicked() {
        app.run("view.3d", json!({"shader": shader.id()}));
        ui.close();
    }
}

fn camera(app: &mut MaterialCraftApp, ui: &mut Ui) {
    edit_properties(app, ui, Subject::Camera);
    ui.separator();
    if ui.add(egui::Button::new(tr("Frame")).shortcut_text("F")).clicked() {
        super::frame_view(app);
        ui.close();
    }
    ui.menu_button(tr("Viewpoint"), |ui| {
        for viewpoint in Viewpoint::ALL {
            if ui.button(tr(viewpoint.label)).clicked() {
                app.run("view.3d.viewpoint", json!({"name": viewpoint.id}));
                ui.close();
            }
        }
    });

    ui.separator();
    if ui.add(egui::Button::new(tr("Save Render...")).shortcut_text("Alt+S")).clicked() {
        super::output::save_render(app);
        ui.close();
    }
    if ui.add(egui::Button::new(tr("Copy Render to Clipboard")).shortcut_text("Alt+C")).clicked() {
        super::output::copy_render(app, ui.ctx());
        ui.close();
    }

    ui.separator();
    command(app, ui, "Reset Position", Some("R"), "view.3d.resetCamera");
    if ui.add(egui::Button::new(tr("Reset Selected")).shortcut_text("Shift+R")).clicked() {
        super::navigation::reset_selected(app);
        ui.close();
    }
}

fn display(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = app.session.view3d.clone();
    toggle(app, ui, "Wireframe", "wireframe", view.wireframe);

    ui.separator();
    toggle(app, ui, "Axis", "axis", view.show_axis);
    toggle(app, ui, "Backface culling", "backfaceCulling", view.backface_culling);
    toggle(app, ui, "Bounding box", "boundingBox", view.bounding_box);
    toggle(app, ui, "Grid", "grid", view.show_grid);
    toggle(app, ui, "Light", "lightHelpers", view.light_helpers);
    toggle(app, ui, "Vertex tangent space", "vertexTangentSpace", view.vertex_tangent_space);

    ui.separator();
    toggle(app, ui, "Resolution", "resolution", view.show_resolution);
    toggle(app, ui, "Scene stats", "sceneStats", view.scene_stats);
    toggle(app, ui, "FPS", "fps", view.fps);
}

#[cfg(test)]
mod hebrew_view3d_menu_layout_tests {
    use super::with_view3d_menu_direction;

    fn menu_centers(rtl: bool) -> (f32, f32) {
        let ctx = egui::Context::default();
        let mut centers = None;
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    with_view3d_menu_direction(ui, rtl, |ui| {
                        let first = ui.button("Material").rect.center().x;
                        let next = ui.button("Scene").rect.center().x;
                        centers = Some((first, next));
                    });
                });
            });
        });
        output.textures_delta.clear();
        centers.expect("two real egui menu hitboxes")
    }

    #[test]
    fn hebrew_menu_mirrors_hitboxes_while_english_stays_ltr() {
        let (en_first, en_second) = menu_centers(false);
        let (he_first, he_second) = menu_centers(true);
        assert!(en_first < en_second, "English menu order changed: {en_first} / {en_second}");
        assert!(he_first > he_second, "Hebrew menu not mirrored: {he_first} / {he_second}");
        assert!(he_first > en_first + 200.0, "Hebrew menu not anchored at right edge");
    }
}
