//! The bar under the 3D view: the channels and the color space of the picture on the left, the
//! scene on the right

use egui::Ui;
use egui_phosphor::regular as icon;
use materialcraft_engine::geom::view::{ColorSpace, ViewChannels};
use materialcraft_engine::view_cmds::SCENES;
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const SCENE_WIDTH: f32 = 120.0;
const COLOR_SPACE_WIDTH: f32 = 150.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.horizontal(|ui| {
            channel_menu(app, ui);
            color_space_menu(app, ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                scene_menu(app, ui);
                ui.label(egui::RichText::new(icon::SPHERE).color(tokens.text_dim));
            });
        });
    });
}

/// Choose the color channels shown, or the material's alpha in their place
fn channel_menu(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let view = &app.session.view3d;
    let (channels, alpha) = (view.view_channels, view.alpha_view);
    let label = if alpha { "Alpha" } else { channels.label() };
    let response =
        crate::widgets::chip(ui, &format!("{} {}", tr(label), icon::CARET_DOWN), alpha || channels != ViewChannels::Rgb).on_hover_text(tr("Channels shown"));
    crate::widgets::name(&response, egui::Role::Button, "Channels shown");

    egui::Popup::menu(&response).show(|ui| {
        for &choice in ViewChannels::ALL {
            if ui.add(egui::Button::selectable(!alpha && channels == choice, tr(choice.label()))).clicked() {
                app.run("view.3d", json!({"viewChannels": choice.id(), "alphaView": false}));
                ui.close();
            }
        }
        if ui.add(egui::Button::selectable(alpha, tr("Alpha"))).clicked() {
            app.run("view.3d", json!({"alphaView": true}));
            ui.close();
        }
    });
}

/// Choose how rendered light becomes display colors
fn color_space_menu(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let current = app.session.view3d.color_space;
    let response = egui::ComboBox::from_id_salt("view3d-color-space").width(COLOR_SPACE_WIDTH).selected_text(tr(current.label())).show_ui(ui, |ui| {
        for &space in ColorSpace::ALL {
            if ui.selectable_label(current == space, tr(space.label())).clicked() {
                app.run("view.3d", json!({"colorSpace": space.id()}));
            }
        }
    });
    crate::widgets::name(&response.response, egui::Role::ComboBox, "Color space");
}

fn scene_menu(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let response = egui::ComboBox::from_id_salt("scene3d").width(SCENE_WIDTH).selected_text(super::scene_label(app)).show_ui(ui, |ui| {
        for (id, label) in SCENES {
            if ui.selectable_label(app.session.view3d.scene == *id, tr(label)).clicked() {
                app.run("view.3d", json!({"scene": id}));
            }
        }
    });
    crate::widgets::name(&response.response, egui::Role::ComboBox, "Scene");
}
