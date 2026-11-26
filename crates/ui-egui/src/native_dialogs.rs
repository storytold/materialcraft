//! The file pickers: asked through the host's file service and answered on a later frame. One pick is open at a time

use craft_host::{Files, PickId, PickRequest, Picked};
use materialcraft_engine::image::Image;
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

use crate::MaterialCraftApp;
use crate::i18n::tr;

/// What the app does with an answered dialog
pub(crate) enum Then {
    OpenPackage,
    /// `document` is the package the request was for; `before_action` finishes a Save in the unsaved-changes prompt
    SavePackage {
        document: u64,
        copy: bool,
        before_action: bool,
    },
    ImportBitmap(GraphRef),
    ImportResource {
        document: u64,
        vector: bool,
        folder: String,
    },
    /// The Export window's destination for this tab
    ExportDestination(crate::export::ExportTab),
    LoadMesh {
        document: u64,
    },
    SaveImage(Box<Image>),
    /// The 3D view's frame to write as a PNG file: width, height and RGBA pixels
    SaveRender(Box<(u32, u32, std::sync::Arc<Vec<u8>>)>),
    /// The mesh the 3D view shows, to write as an OBJ file
    ExportGeometry(std::sync::Arc<materialcraft_engine::geom::Mesh>),
    SaveViewState,
    /// Apply a saved 3D view state, also switching to its mesh when `with_mesh` is set
    LoadViewState {
        with_mesh: bool,
    },
    /// The image file the 2D view draws behind transparent pixels
    BackgroundImage,
    /// Help ▸ Export log: diagnostic events without message details
    ExportLog(String),
    /// The parameters `graph.input.bind` runs with, before the path
    BindInput(Value, GraphRef),
    /// A browser save of the package: success is `Downloaded` or `Saved`; `before_action` finishes a Save in the
    /// unsaved-changes prompt.
    DownloadPackage {
        /// The package the save was for.
        document: u64,
        /// Save a copy.
        copy: bool,
        /// Finish a Save in the unsaved-changes prompt.
        before_action: bool,
        /// The path the save was requested for.
        path: String,
    },
    /// A download with no follow-up beyond its toast.
    Download,
}

/// The active graph when a dialog was requested. Its index can pass to another graph when one is deleted, so its folder-qualified path is part of the identity
#[derive(Clone, PartialEq)]
pub(crate) struct GraphRef {
    document: u64,
    graph: usize,
    path: String,
}

impl GraphRef {
    pub(crate) fn active(app: &MaterialCraftApp) -> GraphRef {
        GraphRef { document: app.session.document_id, graph: app.session.graph, path: app.session.graph().map(|g| g.sbs_path()).unwrap_or_default() }
    }
}

/// Whether a file pick is open or waiting for its answer: the file-dialog source of `Modality::capture`.
pub(crate) fn pending(app: &MaterialCraftApp) -> bool {
    !app.picks.is_empty()
}

/// Asks the host for a file, then does `then` with the answer. While another pick waits, the request is dropped: the
/// host cannot bring an open picker forward.
pub(crate) fn ask(app: &mut MaterialCraftApp, request: PickRequest, then: Then) {
    start(app, then, |files| files.pick(request));
}

/// Asks the host to write `bytes` where the user picks (a download in the browser), then does `then` with the answer.
/// The same one-pick rule as [`ask`] applies.
pub(crate) fn save(app: &mut MaterialCraftApp, request: PickRequest, bytes: Vec<u8>, then: Then) {
    start(app, then, |files| files.save(request, bytes));
}

/// The one place a pick starts: the host's file service is read at each call, so a replaced host takes the next pick
fn start(app: &mut MaterialCraftApp, then: Then, pick: impl FnOnce(&dyn Files) -> PickId) {
    if pending(app) {
        return;
    }

    let id = pick(app.host.files());
    app.picks.insert(id, then);
}

#[cfg(not(target_arch = "wasm32"))]
fn save_dialog(app: &MaterialCraftApp) -> PickRequest {
    let archive = app.session.path.as_deref().is_some_and(materialcraft_engine::file_cmds::is_mcraft);
    let request = crate::windows::package_dialog(false, archive);
    let name = format!("{}.{}", materialcraft_engine::formats::safe_file_name(&app.session.pkg.name), request.suggested_extension.as_deref().unwrap_or_default());

    request.file_name(name)
}

/// File ▸ Save as and Save a copy as: choose where to write the active package
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn save_package(app: &mut MaterialCraftApp, copy: bool) {
    let then = Then::SavePackage { document: app.session.document_id, copy, before_action: false };
    let request = save_dialog(app);
    ask(app, request, then);
}

/// Help ▸ Export log: ask where to write a diagnostic event log
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn export_log(app: &mut MaterialCraftApp) {
    let text = log_for_export(&app.session);
    let request = PickRequest::save("document").file_name("materialcraft-log.txt").filter(tr("Text"), &["txt"]);
    ask(app, request, Then::ExportLog(text));
}

#[cfg(not(target_arch = "wasm32"))]
fn log_for_export(session: &materialcraft_engine::Session) -> String {
    use std::fmt::Write as _;

    let mut text = format!("MaterialCraft {}\nMessage details omitted to protect document names and paths.\n", materialcraft_engine::VERSION);

    // Raw Console messages can contain names and paths in platform-specific error formats
    for message in &session.log {
        let event = match message.as_str() {
            message if message.starts_with(materialcraft_engine::WARNING_PREFIX) => "Package warning",
            message if message.starts_with("Opened ") => "Opened package",
            message if message.starts_with("Saved ") => "Saved file",
            message if message.starts_with("Save all: ") => "Saved multiple packages",
            message if message.starts_with("Exported ") => "Exported file",
            _ => "Session message",
        };
        let _ = writeln!(text, "{event}");
    }

    let (failures, unsupported) = session.cook_failures();
    let _ = writeln!(text, "Node errors in the active graph: {}\nUnsupported nodes: {unsupported}", failures.len());
    text
}

/// Apply the answers that arrived since the last frame. The app's frame logic calls it once per frame, outside the
/// passes, so a second pass of a frame never applies an answer again
pub(crate) fn poll(app: &mut MaterialCraftApp) {
    let mut ids: Vec<PickId> = app.picks.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let Some(picked) = app.host.files().take_answer(id) else { continue };
        if let Some(then) = app.picks.remove(&id) {
            answered(app, then, picked);
        }
    }
}

fn answered(app: &mut MaterialCraftApp, then: Then, picked: Picked) {
    match picked {
        // A hosted browser page answers a package save with the file it made
        Picked::Saved { .. } if matches!(then, Then::DownloadPackage { .. }) => downloaded(app, then),
        #[cfg(not(target_arch = "wasm32"))]
        Picked::Paths(paths) => match paths.into_iter().next() {
            Some(path) => chosen(app, then, &path),
            None => cancelled(app, then),
        },
        #[cfg(not(target_arch = "wasm32"))]
        Picked::Saved { path, .. } => chosen(app, then, &path),
        // The browser reads and writes no paths: an open answers with the file's contents, a save with a download
        #[cfg(target_arch = "wasm32")]
        Picked::Paths(_) | Picked::Saved { .. } => cancelled(app, then),
        Picked::Contents(files) => match files.into_iter().next() {
            Some((name, bytes)) => chosen_bytes(app, then, &name, bytes),
            None => cancelled(app, then),
        },
        Picked::Downloaded(name) => {
            app.toast(format!("{} {name}", tr("Downloaded")), false);
            downloaded(app, then);
        }
        Picked::Cancelled => cancelled(app, then),
        Picked::Unavailable(why) => {
            app.toast(format!("{}: {why}", tr("The file dialog could not be shown")), true);
            cancelled(app, then);
        }
        Picked::Failed(message) => {
            app.toast(message, true);
            cancelled(app, then);
        }
    }
}

fn cancelled(app: &mut MaterialCraftApp, then: Then) {
    let held_save = match then {
        Then::SavePackage { before_action, .. } => before_action,
        Then::DownloadPackage { before_action, .. } => before_action,
        _ => false,
    };
    if held_save {
        crate::unsaved::finish_save_dialog(app, false);
    }
}

/// A save the host finished itself (the browser's download): the package records the path it stands for, and a held
/// Save in the unsaved-changes prompt goes on
fn downloaded(app: &mut MaterialCraftApp, then: Then) {
    let held_save = match then {
        Then::DownloadPackage { document, copy, before_action, path } => {
            crate::web_files::PackageSave::downloaded(app, document, copy, &path);
            before_action
        }
        Then::SavePackage { before_action, .. } => before_action,
        _ => false,
    };
    if held_save {
        crate::unsaved::finish_save_dialog(app, true);
    }
}

/// A browser open: the picked file's name and bytes. Only opens of a package, a bitmap, a resource or a mesh are
/// answered with contents in the browser
fn chosen_bytes(app: &mut MaterialCraftApp, then: Then, name: &str, bytes: Vec<u8>) {
    match then {
        Then::OpenPackage => crate::web_files::load(app, name, bytes, false),
        Then::ImportBitmap(target) => {
            if GraphRef::active(app) == target {
                crate::web_files::load(app, name, bytes, true);
            } else {
                not_applied(app);
            }
        }
        Then::LoadMesh { document } => {
            if app.session.document_id == document {
                crate::web_files::mesh(app, name, &bytes);
            } else {
                not_applied(app);
            }
        }
        Then::ImportResource { document, vector, folder } => match materialcraft_engine::file_cmds::validate_resource_extension(name, vector) {
            Ok(()) => crate::web_files::import_resource_bytes(app, document, name, bytes, &folder),
            Err(error) => app.toast(error.to_string(), true),
        },
        then => cancelled(app, then),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn chosen(app: &mut MaterialCraftApp, then: Then, path: &std::path::Path) {
    let picked = path.display().to_string();
    match then {
        Then::OpenPackage => crate::windows::open_path(app, &picked),
        Then::SavePackage { document, copy, before_action } => {
            let saved = save_to(app, document, copy, &picked);
            if before_action {
                crate::unsaved::finish_save_dialog(app, saved);
            }
        }
        Then::ImportBitmap(target) => {
            if GraphRef::active(app) == target {
                crate::graph::import_bitmap_file(app, &picked);
            } else {
                not_applied(app);
            }
        }
        Then::ImportResource { document, vector, folder } => {
            // A resource picker belongs to the package that opened it, even after another package becomes active
            if app.run("package.activate", json!({"document": document})).is_none() {
                return;
            }

            let command = if vector { "file.importVector" } else { "file.importBitmap" };
            app.run(command, json!({"path": picked, "folder": folder}));
        }
        Then::ExportDestination(tab) => {
            let settings = app.ui.export_draft.get_or_insert_with(|| app.ui.export.clone());
            match tab {
                crate::export::ExportTab::MaterialX => settings.material.folder = picked,
                crate::export::ExportTab::Mcmat => settings.material.archive = picked,
                crate::export::ExportTab::FromGraph | crate::export::ExportTab::Batch => settings.destination = picked,
            }
        }
        Then::LoadMesh { document } => {
            if app.session.document_id == document {
                app.run("view.3d.loadMesh", json!({"path": picked}));
            } else {
                not_applied(app);
            }
        }
        Then::SaveImage(image) => save_image(app, &image, path),
        Then::SaveRender(frame) => {
            let (width, height, pixels) = *frame;
            let snapshot = materialcraft_engine::DisplaySnapshot::render_to(width, height, pixels, path.to_path_buf());
            if app.run_with_display_snapshot("view.3d.saveRender", json!({}), snapshot).0.is_some() {
                app.toast(format!("{} {picked}", tr("Saved")), false);
            }
        }
        Then::ExportGeometry(mesh) => {
            let snapshot = materialcraft_engine::DisplaySnapshot::geometry_to(mesh, path.to_path_buf());
            if app.run_with_display_snapshot("view.3d.exportGeometry", json!({}), snapshot).0.is_some() {
                app.toast(format!("{} {picked}", tr("Saved")), false);
            }
        }
        Then::ExportLog(text) => match materialcraft_engine::formats::write_atomic(path, text.as_bytes()) {
            Ok(()) => app.toast_only(format!("{} {picked}", tr("Saved")), false),
            Err(error) => app.toast_only(format!("{}: {error}", tr("The log could not be saved")), true),
        },
        Then::SaveViewState => {
            if app.run("view.3d.saveState", json!({"path": picked})).is_some() {
                app.toast(format!("{} {picked}", tr("Saved")), false);
            }
        }
        Then::LoadViewState { with_mesh } => {
            app.run("view.3d.loadState", json!({"path": picked, "mesh": with_mesh}));
        }
        Then::BackgroundImage => {
            app.run("view.2d", json!({"background": picked}));
        }
        Then::BindInput(mut parameters, target) => {
            if GraphRef::active(app) != target {
                not_applied(app);
                return;
            }

            parameters["path"] = json!(picked);
            app.run("graph.input.bind", parameters);
        }
    }
}

/// The commands behind these dialogs act on the active graph, so an answer for a graph that is no longer active, or was renamed or moved, is reported instead of applied
fn not_applied(app: &mut MaterialCraftApp) {
    app.toast(tr("Not applied: the active graph or package changed while the file dialog was open"), true);
}

/// Save `document` to the chosen path, activating it first when another package became active meanwhile
#[cfg(not(target_arch = "wasm32"))]
fn save_to(app: &mut MaterialCraftApp, document: u64, copy: bool, picked: &str) -> bool {
    if app.session.document_id != document && app.run("package.activate", json!({"document": document})).is_none() {
        return false;
    }

    let path = materialcraft_engine::file_cmds::package_path(picked);
    if app.run(if copy { "file.saveCopyAs" } else { "file.save" }, json!({"path": path})).is_none() {
        return false;
    }

    if !copy {
        app.ui.add_recent(&path);
    }
    app.toast(format!("{} {path}", tr("Saved")), false);
    true
}

#[cfg(not(target_arch = "wasm32"))]
fn save_image(app: &mut MaterialCraftApp, image: &Image, path: &std::path::Path) {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    let depth = if ext == "exr" { materialcraft_engine::formats::Depth::F32 } else { materialcraft_engine::formats::Depth::U16 };
    let result =
        materialcraft_engine::formats::encode(image, &ext, depth, !image.is_gray()).and_then(|b| materialcraft_engine::formats::write_atomic(path, &b));
    match result {
        Ok(()) => app.toast(format!("{} {}", tr("Saved"), path.display()), false),
        Err(e) => app.toast(e.to_string(), true),
    }
}

#[cfg(test)]
#[path = "native_dialogs.test.rs"]
mod tests;
