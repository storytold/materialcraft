use crate::MaterialCraftApp;
use crate::i18n::tr;

mod package_save;
pub use package_save::PackageSave;

/// What a picked or dropped file is for
#[derive(Clone, PartialEq)]
pub(crate) enum Pick {
    Package,
    Bitmap { target: materialcraft_engine::GraphTarget, path: Option<String> },
    Mesh { document: u64 },
}

impl Pick {
    /// A Bitmap node in the active graph, as it is now
    pub(crate) fn bitmap(app: &MaterialCraftApp) -> Self {
        Self::Bitmap { target: materialcraft_engine::GraphTarget::active(&app.session), path: app.session.graph().ok().map(|graph| graph.sbs_path()) }
    }

    /// The 3D view's mesh, for the active package
    pub(crate) fn mesh(app: &MaterialCraftApp) -> Self {
        Self::Mesh { document: app.session.document_id }
    }

    fn dropped(app: &MaterialCraftApp, name: &str) -> Self {
        let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
        if materialcraft_engine::formats::mesh::MESH_EXTENSIONS.contains(&extension.as_str()) {
            Self::mesh(app)
        } else if materialcraft_engine::formats::IMAGE_EXTENSIONS.contains(&extension.as_str()) {
            Self::bitmap(app)
        } else {
            Self::Package
        }
    }
}

/// Use a file's bytes for `pick`, unless its target changed while the file was on its way
pub(crate) fn apply_pick(app: &mut MaterialCraftApp, name: &str, bytes: Vec<u8>, pick: Pick) {
    match pick {
        Pick::Bitmap { target, path } => {
            // An index can belong to a different graph after deletion, so compare its qualified path too
            if target == materialcraft_engine::GraphTarget::active(&app.session) && path == app.session.graph().ok().map(|graph| graph.sbs_path()) {
                load(app, name, bytes, true);
            } else {
                not_applied(app);
            }
        }
        Pick::Mesh { document } => {
            if app.session.document_id == document {
                mesh(app, name, &bytes);
            } else {
                not_applied(app);
            }
        }
        Pick::Package => load(app, name, bytes, false),
    }
}

fn not_applied(app: &mut MaterialCraftApp) {
    app.toast(tr("Not applied: the active graph or package changed while the file dialog was open"), true);
}

/// Add a picked resource file to `document`, the package its picker was opened for
pub(crate) fn import_resource_bytes(app: &mut MaterialCraftApp, document: u64, name: &str, bytes: Vec<u8>, folder: &str) {
    // A browser picker can finish after the user activates or closes its originating package
    if app.run("package.activate", serde_json::json!({"document": document})).is_none() {
        return;
    }

    match materialcraft_engine::file_cmds::add_resource_into(&mut app.session, name, bytes, None, folder) {
        Ok(name) => app.toast(format!("{} {name}", tr("Imported")), false),
        Err(error) => app.toast(error.to_string(), true),
    }
}

/// Show a mesh file's bytes in the 3D view
pub(crate) fn mesh(app: &mut MaterialCraftApp, name: &str, bytes: &[u8]) {
    match app.session.load_mesh_bytes(name, bytes) {
        Ok(()) => app.toast(format!("{} {name}", tr("Showing")), false),
        Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't load")), true),
    }
}

/// 3D view ▸ Scene ▸ Load mesh: the host's file picker, for the active package
pub fn load_mesh(app: &mut MaterialCraftApp) {
    let request = craft_host::PickRequest::open("import").filter("Meshes", materialcraft_engine::formats::mesh::MESH_EXTENSIONS);
    let then = crate::native_dialogs::Then::LoadMesh { document: app.session.document_id };
    crate::native_dialogs::ask(app, request, then);
}

/// Open a package or import an image (by extension unless `bitmap` forces an image)
pub fn load(app: &mut MaterialCraftApp, name: &str, bytes: Vec<u8>, bitmap: bool) {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if materialcraft_engine::formats::mesh::MESH_EXTENSIONS.contains(&ext.as_str()) {
        mesh(app, name, &bytes);
        return;
    }
    if bitmap || materialcraft_engine::formats::IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        crate::graph::import_bitmap_bytes(app, name, bytes);
        return;
    }
    if app.unsaved.is_some() {
        return;
    }
    load_now(app, name, &bytes);
}

pub(crate) fn load_now(app: &mut MaterialCraftApp, name: &str, bytes: &[u8]) {
    // Browser SBS downloads contain the same directory layout in a transport ZIP
    let path = name.strip_suffix("_sbs.zip").map_or_else(|| name.to_string(), |stem| format!("{stem}.sbs"));
    match materialcraft_engine::file_cmds::open_bytes(&mut app.session, bytes, Some(path)) {
        Ok(()) => {
            app.close_subgraph_after("file.open");
            app.ui.frame_all = true;
            app.toast(format!("{} {name}", tr("Opened")), false);
            crate::windows::after_open(app);
        }
        Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't open")), true),
    }
}

/// Files dropped on the window, as the drop veil reports them: a package opens, an image becomes a Bitmap node, a
/// mesh shows in the 3D view. The desktop gives a path; the browser gives the bytes, read before the drop is reported
pub fn dropped(app: &mut MaterialCraftApp, files: &craft_ui::contract::OsFiles) {
    for file in &files.files {
        let name = if file.name.is_empty() { "dropped" } else { file.name.as_str() };
        if let Some(bytes) = &file.bytes {
            let pick = Pick::dropped(app, name);
            apply_pick(app, name, bytes.to_vec(), pick);
            continue;
        }

        let Some(path) = &file.path else {
            app.toast(format!("{} {name}", tr("Couldn't open")), true);
            continue;
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let lower = name.to_ascii_lowercase();
            if (lower.ends_with(".sbs") || lower.ends_with(".mcraft")) && path.is_file() {
                // Open from the path so an SBS package finds the bitmaps kept next to it
                crate::windows::open_path(app, &path.display().to_string());
                continue;
            }
        }

        match app.host.files().read(path) {
            Ok(bytes) => {
                let pick = Pick::dropped(app, name);
                apply_pick(app, name, bytes, pick);
            }
            Err(e) => app.toast(format!("{} {name}: {e}", tr("Couldn't open")), true),
        }
    }
}

/// A zip of `(name, bytes)` files (export downloads several maps at once)
pub fn zip_files(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    use std::io::Write as _;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            z.start_file(name.as_str(), opts).map_err(|e| e.to_string())?;
            z.write_all(bytes).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::zip_files;
    use crate::MaterialCraftApp;
    use crate::i18n::tr;

    /// File ▸ Save: download either the native archive or SBS with its resources.
    pub fn save(app: &mut MaterialCraftApp) {
        super::PackageSave::save(app);
    }

    /// File ▸ Save a copy as: the Save package dialog, leaving the package's name and unsaved state as they are.
    pub fn save_copy(app: &mut MaterialCraftApp) {
        save_as(app, true);
    }

    /// File ▸ Save as (`copy` for Save a copy as): the Save package dialog.
    pub fn save_as(app: &mut MaterialCraftApp, copy: bool) {
        super::PackageSave::request(app, copy);
    }

    /// The browser's Save package dialog, while a save asks for its name and format.
    pub fn save_dialog(app: &mut MaterialCraftApp, ui: &mut egui::Ui) {
        super::PackageSave::show(app, ui);
    }

    /// Export outputs: one file downloads as is, several as `<graph>_outputs.zip`
    pub fn export(app: &mut MaterialCraftApp, params: &serde_json::Value) {
        let result = materialcraft_engine::ExportRequest::from_params(&app.session, params)
            .and_then(|request| {
                let files = materialcraft_engine::file_cmds::export_outputs(&app.session, &request)?;
                let graph = app.session.pkg.graph(request.graph)?.name.clone();
                Ok((graph, files))
            })
            .map_err(|error| error.to_string())
            .and_then(|(graph, files)| {
                let mut files: Vec<(String, Vec<u8>)> = files.into_iter().map(|exported| (exported.file, exported.bytes)).collect();
                match files.len() {
                    1 => Ok(files.remove(0)),
                    _ => Ok((format!("{graph}_outputs.zip"), zip_files(&files)?)),
                }
            });

        // The download's answer reports the result
        match result {
            Ok((name, bytes)) => download(app, &name, bytes),
            Err(error) => app.toast(format!("{}: {error}", tr("Couldn't export")), true),
        }
    }

    /// Hand `bytes` to the browser as a file named `name`; the answer's toast reports the result.
    // Export saves use the host file service so its asynchronous answer can report success or failure.
    pub fn download(app: &mut MaterialCraftApp, name: &str, bytes: Vec<u8>) {
        let request = craft_host::PickRequest::save("export").file_name(name);
        crate::native_dialogs::save(app, request, bytes, crate::native_dialogs::Then::Download);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(modified: bool) -> MaterialCraftApp {
        let mut s = materialcraft_engine::sample_session();
        s.modified = modified;
        MaterialCraftApp::headless(s, crate::state::UiState::default())
    }

    #[test]
    fn an_invalid_package_reports_an_error_without_discarding_unsaved_work() {
        let mut app = app(true);
        let before = app.session.pkg.name.clone();
        load(&mut app, "other.sbs", b"not a package".to_vec(), false);
        assert!(app.unsaved.is_none());
        assert!(app.pending_load.is_none());
        assert_eq!(app.session.pkg.name, before);
        assert!(app.session.modified);
        assert!(!app.toasts.is_empty());
    }

    #[test]
    fn an_image_import_is_not_a_replacement_and_never_asks() {
        let mut app = app(true);
        load(&mut app, "x.png", b"junk".to_vec(), true);
        assert!(app.unsaved.is_none());
    }

    #[test]
    fn zip_round_trips() {
        let files = vec![("a.png".to_string(), b"one".to_vec()), ("b.png".to_string(), vec![7u8; 4096])];
        let bytes = zip_files(&files).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 2);
        for (name, data) in &files {
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut zip.by_name(name).unwrap(), &mut out).unwrap();
            assert_eq!(&out, data);
        }
    }

    #[test]
    fn delayed_bitmap_picks_and_drops_do_not_follow_a_changed_graph() {
        use materialcraft_engine::formats::{Depth, encode};
        use materialcraft_engine::image::{Image, Kind};

        let bytes = encode(&Image::filled(2, 2, Kind::Gray, [0.5; 4]), "png", Depth::U8, false).unwrap();
        for dropped in [false, true] {
            for change in ["package", "graph", "rename", "folder"] {
                let mut app = app(false);
                let pick = if dropped { Pick::dropped(&app, "picked.png") } else { Pick::bitmap(&app) };
                match change {
                    "package" => {
                        app.run("file.newPackage", serde_json::json!({})).unwrap();
                    }
                    "graph" => {
                        app.run("file.new", serde_json::json!({"template": "empty", "name": "Other"})).unwrap();
                    }
                    "rename" => app.session.graph_mut().unwrap().name = "Renamed".into(),
                    "folder" => app.session.graph_mut().unwrap().folder = "Moved".into(),
                    _ => panic!("unknown graph change: {change}"),
                }
                let before = app.session.pkg.clone();

                apply_pick(&mut app, "picked.png", bytes.clone(), pick);

                assert!(app.session.pkg == before, "{change}, dropped={dropped}");
                assert!(app.toasts.iter().any(|toast| toast.error && toast.text.starts_with("Not applied")));
            }
        }
    }

    #[test]
    fn delayed_mesh_picks_and_drops_do_not_follow_another_package() {
        for dropped in [false, true] {
            let mut app = app(false);
            let pick = if dropped { Pick::dropped(&app, "picked.obj") } else { Pick::mesh(&app) };
            app.run("file.newPackage", serde_json::json!({})).unwrap();

            apply_pick(&mut app, "picked.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec(), pick);

            assert!(app.session.custom_mesh.is_none());
            assert!(app.toasts.iter().any(|toast| toast.error && toast.text.starts_with("Not applied")));
        }
    }

    #[test]
    fn bitmap_and_mesh_picks_and_drops_apply_when_the_target_is_unchanged() {
        use materialcraft_engine::formats::{Depth, encode};
        use materialcraft_engine::image::{Image, Kind};

        let bytes = encode(&Image::filled(2, 2, Kind::Gray, [0.5; 4]), "png", Depth::U8, false).unwrap();
        for dropped in [false, true] {
            let mut app = app(false);
            let nodes = app.session.graph().unwrap().nodes.len();
            let pick = if dropped { Pick::dropped(&app, "picked.png") } else { Pick::bitmap(&app) };

            apply_pick(&mut app, "picked.png", bytes.clone(), pick);

            assert_eq!(app.session.graph().unwrap().nodes.len(), nodes + 1);
            assert!(app.session.modified);
            assert!(!app.toasts.iter().any(|toast| toast.error));

            let pick = if dropped { Pick::dropped(&app, "picked.obj") } else { Pick::mesh(&app) };
            apply_pick(&mut app, "picked.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n".to_vec(), pick);

            assert!(app.session.custom_mesh.is_some());
            assert!(!app.toasts.iter().any(|toast| toast.error));
        }
    }

    #[test]
    fn delayed_resource_import_targets_its_original_package_without_a_node() {
        let mut app = app(false);
        let original = app.session.document_id;
        app.run("file.newPackage", serde_json::json!({"name": "Other"})).unwrap();
        let other = app.session.document_id;
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2"/></svg>"#;
        import_resource_bytes(&mut app, original, "shape.svg", svg.to_vec(), "Resources");

        assert_eq!(app.session.document_id, original);
        assert_eq!(app.session.pkg.resources.len(), 1);
        assert!(app.session.pkg.graphs.iter().all(|graph| graph.nodes.iter().all(|node| node.def != "svg")));

        app.run("package.close", serde_json::json!({"discard": true})).unwrap();

        assert_eq!(app.session.document_id, other);

        import_resource_bytes(&mut app, original, "shape.svg", svg.to_vec(), "Resources");

        assert!(app.session.pkg.resources.is_empty(), "a closed picker target must not import into a different package");
    }
}
