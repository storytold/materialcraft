//! The toolbar below the image: how the pixels are shown on the left, tiling and zoom on the right

use egui::Ui;
use egui_phosphor::regular as icon;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::state::ImageCamera;
use crate::theme::Tokens;

const BUTTON_SIDE: f32 = 20.0;
const ZOOM_FIELD_WIDTH: f32 = 58.0;

/// The channels the view can show: the `view.2d` id, the label and the key that picks it over the view
pub(super) const CHANNELS: [(&str, &str, egui::Key); 5] =
    [("rgba", "RGB", egui::Key::C), ("r", "Red", egui::Key::R), ("g", "Green", egui::Key::G), ("b", "Blue", egui::Key::B), ("a", "Alpha", egui::Key::A)];

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            channel_menu(app, ui);

            // The backdrop and alpha handling only show with all channels
            let all_channels = app.session.view2d.channel == "rgba";
            ui.add_enabled_ui(all_channels, |ui| {
                setting(app, ui, icon::CHECKERBOARD, "Show a checkerboard behind transparent pixels", "checkerboard", app.session.view2d.checkerboard);
                setting(app, ui, icon::EXCLUDE, "The colors are already multiplied by their alpha", "premultiplied", app.session.view2d.premultiplied);
            });
            setting(app, ui, icon::MONITOR, "The values are sRGB encoded; off shows them as linear light", "srgb", app.session.view2d.srgb);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| zoom_controls(app, ui));
        });
    });
}

/// A toggle button for one boolean `view.2d` setting
fn setting(app: &mut MaterialCraftApp, ui: &mut Ui, icon: &str, tooltip: &str, parameter: &str, on: bool) {
    if crate::widgets::icon_button(ui, icon, tr(tooltip), on, BUTTON_SIDE).clicked() {
        app.run("view.2d", json!({parameter: !on}));
    }
}

fn channel_menu(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let current = app.session.view2d.channel.clone();
    let label = CHANNELS.iter().find(|(id, ..)| *id == current).map_or("RGB", |(_, label, _)| label);
    let response = crate::widgets::chip(ui, &format!("{} {}", tr(label), icon::CARET_DOWN), current != "rgba").on_hover_text(tr("Channels shown"));
    crate::widgets::name(&response, egui::Role::Button, "Channels shown");

    egui::Popup::menu(&response).show(|ui| {
        for (id, label, key) in CHANNELS {
            let entry = egui::Button::selectable(current == id, tr(label)).shortcut_text(key.name());
            if ui.add(entry).clicked() {
                app.run("view.2d", json!({"channel": id}));
                ui.close();
            }
        }
    });
}

/// Right to left: the zoom lock, the zoom buttons around the zoom field, then the sizing toggles
fn zoom_controls(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = &app.session.view2d;
    let (maintain_zoom, physical_size, tiled) = (view.maintain_zoom, view.physical_size, view.tiled);

    let lock = if maintain_zoom { icon::LOCK } else { icon::LOCK_OPEN };
    setting(app, ui, lock, "Keep the zoom when the image size changes; off keeps the image's size on screen", "maintainZoom", maintain_zoom);

    if crate::widgets::icon_button(ui, icon::PLUS, tr("Zoom in"), false, BUTTON_SIDE).clicked() {
        app.ui.image_camera = app.ui.image_camera.zoomed(1.0, [0.0, 0.0]);
    }
    let mut percent = app.ui.image_camera.zoom * 100.0;
    let (lowest, highest) = (ImageCamera::ZOOM.0 * 100.0, ImageCamera::ZOOM.1 * 100.0);
    let field = ui.add_sized([ZOOM_FIELD_WIDTH, BUTTON_SIDE], egui::DragValue::new(&mut percent).range(lowest..=highest).max_decimals(2).suffix("%"));
    crate::widgets::name(&field, egui::Role::SpinButton, "Zoom");
    if field.changed() {
        app.ui.image_camera = ImageCamera { zoom: percent / 100.0, fit: false, ..app.ui.image_camera }.sanitized();
    }
    if crate::widgets::icon_button(ui, icon::MINUS, tr("Zoom out"), false, BUTTON_SIDE).clicked() {
        app.ui.image_camera = app.ui.image_camera.zoomed(-1.0, [0.0, 0.0]);
    }

    if ui.small_button("1:1").on_hover_text(tr("Actual pixels")).clicked() {
        app.ui.image_camera = ImageCamera { zoom: 1.0, offset: [0.0, 0.0], fit: false };
    }
    if crate::widgets::icon_button(ui, icon::CORNERS_OUT, tr("Fit"), app.ui.image_camera.fit, BUTTON_SIDE).clicked() {
        app.ui.image_camera = ImageCamera::default();
    }
    setting(app, ui, icon::RULER, "Show the image in the proportions of the graph's physical size", "physicalSize", physical_size);
    setting(app, ui, icon::GRID_FOUR, "Repeat the image to check its tiling", "tiled", tiled);
}
