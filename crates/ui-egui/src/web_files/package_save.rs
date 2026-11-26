use craft_ui::choice::{Radio, RadioGroup};
use craft_ui::contract::{Bind, Props as _, Show as _, ShowContainer as _};
use craft_ui::layout_components::Stack;
use craft_ui::patterns::form_dialogs::FormDialog;
use craft_ui::text::TextField;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// A browser save request retains its package while other documents are opened or closed
#[derive(Clone, Debug, PartialEq)]
pub struct PackageSave {
    document: u64,
    name: String,
    archive: bool,
    copy: bool,
    before_action: bool,
}

/// What the Save package dialog edits: the file's name and whether it is a MaterialCraft Project
#[derive(Clone, Debug, Default, PartialEq)]
struct SaveForm {
    name: String,
    archive: bool,
}

impl PackageSave {
    pub(crate) fn request(app: &mut MaterialCraftApp, copy: bool) {
        // Replacing this dialog would let an unrelated save release the held close action
        if crate::unsaved::saving(app) {
            return;
        }

        let archive = app.session.path.as_deref().is_some_and(materialcraft_engine::file_cmds::is_mcraft);
        app.ui.windows.package_save = Some(Self { document: app.session.document_id, name: app.session.pkg.name.clone(), archive, copy, before_action: false });
    }

    pub(crate) fn save(app: &mut MaterialCraftApp) {
        if let Some(path) = app.session.path.clone() {
            Self::download(app, &path, false, false);
        } else {
            Self::request(app, false);
        }
    }

    /// The unsaved-changes prompt's Save for the active package: the held action waits until the download answers
    pub(crate) fn save_then(app: &mut MaterialCraftApp) {
        if let Some(path) = app.session.path.clone() {
            crate::unsaved::wait_for_save(app);
            Self::download(app, &path, false, true);
        } else {
            Self::request(app, false);
            if let Some(request) = &mut app.ui.windows.package_save {
                request.before_action = true;
            }
            crate::unsaved::wait_for_save(app);
        }
    }

    pub(crate) fn show(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
        let Some(request) = &app.ui.windows.package_save else { return };

        let mut form = SaveForm { name: request.name.clone(), archive: request.archive };
        let shown = FormDialog::new(tr("Save package"), Bind::bound(&mut form)).ok_label(tr("Download")).show(ui, |ui, _, form| {
            let _ = TextField::new(tr("Name"), Bind::bound(&mut form.name)).show(ui);
            RadioGroup::new(tr("Format"), Bind::bound(&mut form.archive)).show(ui, |ui, _| {
                Stack::new().show(ui, |ui, _| {
                    Radio::new(false).label(tr("SBS XML Package (*.sbs)")).detail(tr("Downloads the SBS and resources together in a ZIP.")).show(ui);
                    Radio::new(true).label(tr("MaterialCraft Project (*.mcraft)")).show(ui);
                });
            });
            true
        });
        if let Some(request) = &mut app.ui.windows.package_save {
            request.name = form.name;
            request.archive = form.archive;
        }
        match (shown.accepted, shown.closed) {
            (Some(_), _) => Self::finish(app, true),
            (None, Some(_)) => Self::finish(app, false),
            (None, None) => {}
        }
    }

    fn finish(app: &mut MaterialCraftApp, accept: bool) {
        let Some(request) = app.ui.windows.package_save.take() else { return };

        if accept && app.run("package.activate", serde_json::json!({"document": request.document})).is_some() {
            let extension = if request.archive { "mcraft" } else { "sbs" };
            let path = format!("{}.{extension}", materialcraft_engine::formats::safe_file_name(&request.name));
            Self::download(app, &path, request.copy, request.before_action);
        } else if request.before_action {
            crate::unsaved::finish_save_dialog(app, false);
        }
    }

    /// Hand the package to the browser as `path` (an SBS package as a ZIP with its resources); the answer arrives in
    /// `native_dialogs::poll`, which calls [`PackageSave::downloaded`] on success
    fn download(app: &mut MaterialCraftApp, path: &str, copy: bool, before_action: bool) {
        let archive = materialcraft_engine::file_cmds::is_mcraft(path);
        let name = if archive { path.to_string() } else { format!("{}_sbs.zip", path.strip_suffix(".sbs").unwrap_or(path)) };
        let bytes = materialcraft_engine::file_cmds::save_files_at(&app.session, path)
            .map_err(|error| error.to_string())
            .and_then(|files| files.zip().map_err(|error| error.to_string()));

        match bytes {
            Err(error) => {
                app.toast(format!("{} {name}: {error}", tr("Couldn't save")), true);
                if before_action {
                    crate::unsaved::finish_save_dialog(app, false);
                }
            }
            // A save that cannot be asked while another file dialog waits must not leave the held action waiting
            Ok(_) if before_action && crate::native_dialogs::pending(app) => crate::unsaved::finish_save_dialog(app, false),
            Ok(bytes) => {
                let then = crate::native_dialogs::Then::DownloadPackage { document: app.session.document_id, copy, before_action, path: path.to_string() };
                crate::native_dialogs::save(app, craft_host::PickRequest::save("document").file_name(name), bytes, then);
            }
        }
    }

    /// The download of `document` as `path` finished: unless it was a copy, the package now has that path and no
    /// unsaved changes
    // Package saves use the same host file service on every target.
    pub(crate) fn downloaded(app: &mut MaterialCraftApp, document: u64, copy: bool, path: &str) {
        if copy || app.run("package.activate", serde_json::json!({"document": document})).is_none() {
            return;
        }
        app.session.path = Some(path.to_string());
        app.session.modified = false;
    }
}

#[cfg(test)]
#[path = "package_save.test.rs"]
mod tests;
