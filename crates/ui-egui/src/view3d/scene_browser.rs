//! The scene browser beside the 3D view: what the scene holds. A click on the mesh selects it, a
//! click on anything else shows it in the Properties panel

use egui::Ui;
use egui_phosphor::regular as icon;

use super::Subject;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

/// Width of the browser, when the view is wide enough to keep showing the mesh beside it
pub(super) const WIDTH: f32 = 176.0;
const ROW_HEIGHT: f32 = 20.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    ui.painter().rect_filled(ui.max_rect(), 0.0, tokens.panel);
    ui.add_space(4.0);
    ui.label(crate::theme::caption(tr("Scene")));

    let inspected = super::inspected(app);
    let shader = tr(app.session.view3d.shader.label());
    if row(ui, icon::CUBE, &super::scene_label(app), app.view3d.selected) {
        app.view3d.selected = !app.view3d.selected;
    }
    if row(ui, icon::PALETTE, &format!("{} ({shader})", tr("Default")), inspected == Some(Subject::Material)) {
        super::inspect(app, Subject::Material);
    }
    for light in ["Ambient Light", "Point Light 1", "Point Light 2"] {
        if row(ui, icon::LIGHTBULB, tr(light), inspected == Some(Subject::Lights)) {
            super::inspect(app, Subject::Lights);
        }
    }
    if row(ui, icon::VIDEO_CAMERA, tr("Camera"), inspected == Some(Subject::Camera)) {
        super::inspect(app, Subject::Camera);
    }
    if row(ui, icon::GLOBE_HEMISPHERE_WEST, tr("Environment"), inspected == Some(Subject::Environment)) {
        super::inspect(app, Subject::Environment);
    }
}

/// One entry of the scene: its icon and name. Returns true when it was clicked
fn row(ui: &mut Ui, icon: &str, name: &str, selected: bool) -> bool {
    let button = egui::Button::selectable(selected, format!("{icon}  {name}")).truncate();
    let size = egui::vec2(ui.available_width(), ROW_HEIGHT);
    ui.allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true).with_main_align(egui::Align::LEFT), |ui| {
        ui.add(button).clicked()
    })
    .inner
}
