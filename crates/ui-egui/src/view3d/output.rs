//! What leaves the 3D view: the rendered frame as a file or on the clipboard, the shown mesh as
//! an OBJ file, and the view's state as a file to load again later

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// Put the shown frame on the clipboard
pub(super) fn copy_render(app: &mut MaterialCraftApp, context: &egui::Context) {
    let Some((width, height, pixels)) = &app.view3d.shown else { return };

    context.copy_image(egui::ColorImage::from_rgba_unmultiplied([*width as usize, *height as usize], pixels));
    app.toast(tr("Render copied to the clipboard"), false);
}

/// Ask where to save the shown frame as a PNG file
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn save_render(app: &mut MaterialCraftApp) {
    let Some(frame) = app.view3d.shown.clone() else { return };

    let name = format!("{}.png", materialcraft_engine::formats::safe_file_name(&super::frame::shown_label(app)));
    let dialog = craft_dialogs::FileDialog::save().file_name(name).filter("PNG", &["png"]);
    crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::SaveRender(Box::new(frame)));
}

/// The browser has no file dialog for a render: the frame goes to the clipboard instead
#[cfg(target_arch = "wasm32")]
pub(super) fn save_render(app: &mut MaterialCraftApp) {
    app.toast(tr("Saving a render needs the desktop app; copy it to the clipboard instead"), true);
}

/// Ask where to save the mesh the view shows, raised by the height output when it is, as an OBJ file
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn export_geometry(app: &mut MaterialCraftApp) {
    let Some(mesh) = displayed_mesh(app) else {
        app.toast(tr("The 3D view has not drawn a mesh yet"), true);
        return;
    };
    let name = format!("{}.obj", materialcraft_engine::formats::safe_file_name(&super::frame::shown_label(app)));
    let dialog = craft_dialogs::FileDialog::save().file_name(name).filter("Wavefront OBJ", &["obj"]);
    crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::ExportGeometry(mesh));
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn displayed_mesh(app: &MaterialCraftApp) -> Option<std::sync::Arc<materialcraft_engine::geom::Mesh>> {
    app.view3d.shown_scene.as_ref().map(|frame| frame.scene.mesh.clone())
}

/// Ask where to save the view's state
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn save_state(app: &mut MaterialCraftApp) {
    let extension = materialcraft_engine::VIEW_STATE_EXTENSION;
    let dialog = craft_dialogs::FileDialog::save().file_name(format!("3d-view.{extension}")).filter(tr("3D view state"), &[extension]);
    crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::SaveViewState);
}

/// Ask for a saved state to apply; `with_mesh` also switches to the mesh it was saved with
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load_state(app: &mut MaterialCraftApp, with_mesh: bool) {
    let dialog = craft_dialogs::FileDialog::open().filter(tr("3D view state"), &[materialcraft_engine::VIEW_STATE_EXTENSION]);
    crate::native_dialogs::ask(app, dialog, crate::native_dialogs::Then::LoadViewState { with_mesh });
}
