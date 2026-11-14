//! The toolbar above the image: what is shown, where it goes, and which panels are open

use egui::Ui;
use egui_phosphor::regular as icon;
use serde_json::json;

use super::output_choice::OutputChoice;
use super::shown::Shown;
use crate::MaterialCraftApp;
use crate::i18n::tr;
use crate::theme::Tokens;

const BUTTON_SIDE: f32 = 22.0;

pub(super) fn show(app: &mut MaterialCraftApp, ui: &mut Ui, shown: Option<&Shown>) {
    let tokens = Tokens::get(ui.ctx());
    egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            #[cfg(not(target_arch = "wasm32"))]
            background_image(app, ui);

            ui.add_enabled_ui(shown.is_some(), |ui| {
                #[cfg(not(target_arch = "wasm32"))]
                if crate::widgets::icon_button(ui, icon::FLOPPY_DISK, tr("Save image..."), false, BUTTON_SIDE).clicked() {
                    super::save_image(app);
                }
                if crate::widgets::icon_button(ui, icon::COPY, tr("Copy image to the clipboard"), false, BUTTON_SIDE).clicked() {
                    copy_image(app, ui);
                }
            });
            output_menu(app, ui, shown);
            ui.separator();

            let view = &app.session.view2d;
            let (uv, information, histogram) = (view.uv, view.information, view.histogram);
            if crate::widgets::chip(ui, "UV", uv).on_hover_text(tr("Draw the UV layout of the 3D view's mesh over the image")).clicked() {
                app.run("view.2d", json!({"uv": !uv}));
            }
            if crate::widgets::icon_button(ui, icon::INFO, tr("Image information"), information, BUTTON_SIDE).clicked() {
                app.run("view.2d", json!({"information": !information}));
            }
            if crate::widgets::icon_button(ui, icon::CHART_BAR, tr("Histogram"), histogram, BUTTON_SIDE).clicked() {
                app.run("view.2d", json!({"histogram": !histogram}));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| crate::preview_resolution::show(app, ui));
        });
    });
}

/// Choose an image file to draw behind transparent pixels, or remove the one in use
#[cfg(not(target_arch = "wasm32"))]
fn background_image(app: &mut MaterialCraftApp, ui: &mut Ui) {
    let in_use = app.session.view2d.background.is_some();
    let tooltip = if in_use { tr("Remove the background image") } else { tr("Background image...") };
    if !crate::widgets::icon_button(ui, icon::IMAGE, tooltip, in_use, BUTTON_SIDE).clicked() {
        return;
    }

    if in_use {
        app.run("view.2d", json!({"background": serde_json::Value::Null}));
    } else {
        let dialog = craft_dialogs::FileDialog::open().filter("Images", &["png", "jpg", "jpeg", "tga", "bmp", "tif", "tiff", "exr", "hdr", "dds", "svg"]);
        crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::BackgroundImage);
    }
}

/// Put the image on the clipboard as the view draws it
fn copy_image(app: &mut MaterialCraftApp, ui: &Ui) {
    let Some((width, height, pixels)) = &app.view2d.shown else { return };

    ui.ctx().copy_image(egui::ColorImage::from_rgba_unmultiplied([*width as usize, *height as usize], pixels));
    app.toast(tr("Image copied to the clipboard"), false);
}

/// Choose what the view shows: another output of the graph when an output node is shown, else another output of the shown node
fn output_menu(app: &mut MaterialCraftApp, ui: &mut Ui, shown: Option<&Shown>) {
    let choices = shown.map(|shown| OutputChoice::all(app, shown)).unwrap_or_default();
    let mut chosen = None;

    ui.add_enabled_ui(choices.len() > 1, |ui| {
        let response = crate::widgets::icon_button(ui, icon::SIGN_OUT, tr("Select output"), false, BUTTON_SIDE);
        egui::Popup::menu(&response).show(|ui| {
            for choice in &choices {
                if ui.add(egui::Button::selectable(choice.current, choice.label.as_str())).clicked() {
                    chosen = Some(choice.parameters.clone());
                    ui.close();
                }
            }
        });
    });

    if let Some(parameters) = chosen {
        app.run("view.2d", parameters);
    }
}
