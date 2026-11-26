use std::path::{Path, PathBuf};
use std::sync::Arc;

use craft_host::{FileFilter, PickKind, ScriptedFiles};
use craft_ui::shell::embed::CloseDecision;
use egui_kittest::kittest::Queryable as _;

use super::*;
use crate::UiState;
use crate::unsaved::Guarded;

fn app() -> MaterialCraftApp {
    MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default())
}

/// An app whose file service keeps each pick open until the test answers it
fn scripted() -> (MaterialCraftApp, Arc<ScriptedFiles>) {
    let files = Arc::new(ScriptedFiles::held());
    let mut app = app();
    app.host = Arc::new(app.host.with_files(files.clone()));
    (app, files)
}

#[test]
fn exported_log_keeps_events_and_strips_raw_document_details() {
    let mut app = app();
    app.session.log("Opened C:\\My projects\\Client sample.sbs");
    app.session.warn("Invalid resource at /path/to/local/private textures/input.png");

    let report = log_for_export(&app.session);

    assert!(report.contains("Opened package"));
    assert!(report.contains("Package warning"));
    assert!(report.contains("Node errors in the active graph: 0"));
    assert!(!report.contains("Client sample"));
    assert!(!report.contains("/home/person"));
}

/// Answer every open pick with `picked`, apply the answers as a frame does, and return what was asked
fn answer_all(app: &mut MaterialCraftApp, files: &ScriptedFiles, picked: Picked) -> Vec<PickRequest> {
    while files.answer(picked.clone()).is_some() {}
    poll(app);
    files.requests()
}

fn opened(path: &Path) -> Picked {
    Picked::Paths(vec![path.to_path_buf()])
}

fn saved(path: &Path) -> Picked {
    Picked::Saved { path: path.to_path_buf(), file_type: None }
}

/// The filters a pick offers for a filter list of the app
fn as_filters<N: Into<String>>(filters: Vec<(N, &'static [&'static str])>) -> Vec<FileFilter> {
    filters.into_iter().map(|(name, extensions)| FileFilter::new(name, extensions)).collect()
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("mc-dialogs-{}-{name}", std::process::id()))
}

#[test]
fn package_dialogs_keep_both_project_formats() {
    for (command, open) in [("file.open", true), ("file.saveAs", false), ("file.saveCopyAs", false)] {
        let (mut app, files) = scripted();

        crate::windows::file_action(&mut app, command);
        let asked = answer_all(&mut app, &files, Picked::Cancelled);

        let [request] = asked.as_slice() else { panic!("{asked:?}") };
        assert_eq!(request.filters, as_filters(crate::windows::package_filters(open, false)), "{command}");
        assert_eq!(request.kind, if open { PickKind::Open } else { PickKind::Save }, "{command}");
    }
}

#[test]
fn archive_save_dialog_keeps_the_mcraft_default() {
    for command in ["file.saveAs", "file.saveCopyAs"] {
        let (mut app, files) = scripted();
        app.session.path = Some("project.mcraft".into());

        crate::windows::file_action(&mut app, command);
        let asked = answer_all(&mut app, &files, Picked::Cancelled);

        let [request] = asked.as_slice() else { panic!("{asked:?}") };
        assert!(request.file_name.as_deref().is_some_and(|name| name.ends_with(".mcraft")), "{command}: {:?}", request.file_name);
        assert_eq!(request.filters.first().map(|filter| filter.extensions.clone()), Some(vec!["mcraft".to_string()]), "{command}");
    }
}

#[test]
fn bitmap_dialog_keeps_the_supported_format_filters() {
    let (mut app, files) = scripted();

    crate::windows::file_action(&mut app, "file.importBitmap");
    let asked = answer_all(&mut app, &files, Picked::Cancelled);

    let [request] = asked.as_slice() else { panic!("{asked:?}") };
    assert_eq!(request.filters, as_filters(crate::windows::resource_filters(false)));
}

#[test]
fn a_picked_archive_saves_and_reopens_with_resources() {
    for copy in [false, true] {
        let (mut app, files) = scripted();
        let image = png(&format!("archive-{copy}.png"));
        app.run("file.importBitmap", json!({"path": image})).unwrap();
        app.session.path = Some("original.mcraft".into());
        let original = app.session.pkg.clone();
        let path = temp(&format!("archive-{copy}.mcraft"));

        crate::windows::file_action(&mut app, if copy { "file.saveCopyAs" } else { "file.saveAs" });
        answer_all(&mut app, &files, saved(&path));

        assert!(path.exists());
        assert_eq!(app.session.modified, copy);
        if copy {
            assert_eq!(app.session.path.as_deref(), Some("original.mcraft"));
            assert!(app.ui.recent.is_empty());
        }

        crate::windows::file_action(&mut app, "file.open");
        answer_all(&mut app, &files, opened(&path));

        assert_eq!(app.session.path.as_deref().map(PathBuf::from), std::fs::canonicalize(&path).ok());
        assert_eq!(app.session.pkg.resources.len(), original.resources.len());
        assert_eq!(app.session.pkg.resources.last().unwrap().bytes, original.resources.last().unwrap().bytes);
        assert!(app.toasts.iter().all(|toast| !toast.error));
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(image).unwrap();
    }
}

#[test]
fn resource_dialogs_keep_the_originating_package_and_folder() {
    for vector in [false, true] {
        for closed in [false, true] {
            let (mut app, files) = scripted();
            let document = app.session.document_id;
            app.run("folder.create", json!({"parent": "Resources", "name": "Imported"})).unwrap();
            let nodes = app.session.graph().unwrap().nodes.len();
            let path = if vector {
                let path = temp(&format!("resource-{closed}.svg"));
                std::fs::write(&path, r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><path d="M0 0H4V4H0Z"/></svg>"#).unwrap();
                path
            } else {
                png(&format!("resource-{closed}.png"))
            };

            crate::windows::import_resource_into(&mut app, vector, "Resources/Imported");
            assert_eq!(files.open(), 1);
            app.run("file.newPackage", json!({})).unwrap();
            let other = app.session.document_id;
            if closed {
                app.run("package.close", json!({"document": document, "discard": true})).unwrap();
            }
            let asked = answer_all(&mut app, &files, opened(&path));

            let [request] = asked.as_slice() else { panic!("{asked:?}") };
            assert_eq!(request.filters, as_filters(crate::windows::resource_filters(vector)));
            assert!(app.session.package(other).unwrap().resources.is_empty());
            if closed {
                assert_eq!(app.session.document_id, other);
                assert!(app.toasts.iter().any(|toast| toast.error));
            } else {
                assert_eq!(app.session.document_id, document);
                assert_eq!(app.session.graph().unwrap().nodes.len(), nodes);
                let resource = app.session.pkg.resources.last().unwrap();
                assert_eq!(resource.folder, "Resources/Imported");
                assert_eq!(resource.bytes.as_slice(), std::fs::read(&path).unwrap());
                app.run("edit.undo", json!({})).unwrap();
                assert!(app.session.pkg.resources.is_empty());
            }
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[test]
fn save_as_writes_the_package_and_adds_it_to_recent() {
    let (mut app, files) = scripted();
    let path = temp("saved.sbs");
    assert!(crate::windows::file_action(&mut app, "file.saveAs"));
    let asked = answer_all(&mut app, &files, saved(&path));
    let [request] = asked.as_slice() else { panic!("{asked:?}") };
    assert_eq!(request.kind, PickKind::Save);
    assert_eq!(request.file_name.as_deref(), Some(format!("{}.sbs", app.session.pkg.name).as_str()));
    assert!(path.exists());
    assert!(!app.ui.recent.is_empty());
    assert_eq!(app.session.path.as_deref().map(PathBuf::from), std::fs::canonicalize(&path).ok());
    std::fs::remove_file(path).ok();
}

#[test]
fn a_canceled_dialog_changes_nothing() {
    let (mut app, files) = scripted();
    let before = app.session.pkg.clone();
    crate::windows::file_action(&mut app, "file.saveAs");
    let asked = answer_all(&mut app, &files, Picked::Cancelled);
    assert_eq!(asked.len(), 1);
    assert_eq!(app.session.pkg, before);
    assert!(app.toasts.is_empty());
}

#[test]
fn an_unavailable_dialog_reports_why() {
    let (mut app, files) = scripted();
    crate::windows::file_action(&mut app, "file.saveAs");
    answer_all(&mut app, &files, Picked::Unavailable("no display".into()));
    assert!(app.toasts.iter().any(|toast| toast.error && toast.text.contains("no display")), "{:?}", app.toasts.len());
}

#[test]
fn a_second_request_while_a_dialog_is_waiting_is_not_queued() {
    let (mut app, files) = scripted();
    crate::windows::file_action(&mut app, "file.open");
    crate::windows::file_action(&mut app, "file.open");
    assert_eq!(files.requests().len(), 1);
}

#[test]
fn save_goes_straight_to_a_path_the_package_has() {
    let (mut app, files) = scripted();
    let path = temp("direct.sbs");
    app.session.path = Some(path.display().to_string());
    crate::windows::file_action(&mut app, "file.save");
    assert!(files.requests().is_empty(), "no dialog for a package with a path");
    assert!(path.exists());
    std::fs::remove_file(path).ok();
}

type Shell = egui_kittest::Harness<'static, MaterialCraftApp>;

/// A modified package without a path, asking to quit, in an app whose file service keeps each pick open for the test
fn modified_app_asking_to_quit() -> (MaterialCraftApp, Arc<ScriptedFiles>) {
    let (mut app, files) = scripted();
    app.session.modified = true;
    app.session.path = None;
    crate::unsaved::guard(&mut app, Guarded::Quit);
    assert_eq!(crate::unsaved::held(&app), Some(&Guarded::Quit));
    (app, files)
}

/// Draw the unsaved-changes prompt and answer it with its Save button
// The prompt is drawn alone so its Save button is unambiguous; the whole window also has toolbar and File menu Save buttons.
fn answer_prompt_with_save(app: MaterialCraftApp) -> Shell {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(700.0, 350.0))
        .with_context_setup(|ctx| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(|ui, app: &mut MaterialCraftApp| crate::unsaved::show(app, ui), app);
    harness.run();
    harness.get_by_label("Save").click();
    harness.run();
    harness
}

#[test]
fn the_prompts_save_with_a_path_saves_and_runs_the_action_at_once() {
    let (mut app, files) = modified_app_asking_to_quit();
    let path = temp("prompt-direct.sbs");
    app.session.path = Some(path.display().to_string());
    let mut harness = answer_prompt_with_save(app);
    let app = harness.state_mut();
    assert!(files.requests().is_empty(), "no dialog for a package with a path");
    assert!(path.exists());
    assert!(crate::unsaved::held(app).is_none());
    assert_eq!(crate::unsaved::close_requested(app), CloseDecision::Close);
    std::fs::remove_file(path).ok();
}

#[test]
fn the_app_shows_a_requested_dialog_and_applies_its_answer() {
    let (app, files) = scripted();
    let mut harness = egui_kittest::Harness::builder().build_eframe(move |cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app
    });
    harness.step();
    harness.step();
    let path = temp("frames.sbs");
    crate::windows::file_action(harness.state_mut(), "file.saveAs");
    assert_eq!(files.open(), 1);
    harness.step();
    assert!(!path.exists(), "nothing is written while the pick waits");
    assert!(files.answer(saved(&path)).is_some());
    assert!(!path.exists(), "the answer is applied by a frame");
    harness.step();
    assert!(path.exists(), "the frame applies the answer");
    std::fs::remove_file(path).ok();
}

#[test]
fn a_picked_package_opens() {
    let mut first = app();
    let path = temp("opened.sbs");
    let document = first.session.document_id;
    assert!(save_to(&mut first, document, false, &path.display().to_string()));

    let (mut app, files) = scripted();
    crate::windows::file_action(&mut app, "file.open");
    answer_all(&mut app, &files, opened(&path));
    assert_eq!(app.session.path.as_deref().map(PathBuf::from), std::fs::canonicalize(&path).ok());
    assert!(app.toasts.iter().any(|toast| !toast.error && toast.text.starts_with("Opened")), "{}", app.toasts.len());
    std::fs::remove_file(path).ok();
}

#[test]
fn export_material_opens_the_export_window_on_the_mcmat_tab() {
    let (mut app, files) = scripted();

    crate::windows::file_action(&mut app, "file.exportMaterial");

    assert!(app.ui.windows.export);
    assert_eq!(app.ui.export_tab, crate::export::ExportTab::Mcmat);
    assert!(files.requests().is_empty() && !pending(&app), "the window picks the destination, not a dialog of its own");
}

fn png(name: &str) -> PathBuf {
    use materialcraft_engine::formats::{Depth, encode};
    let image = Image::filled(4, 4, materialcraft_engine::image::Kind::Gray, [0.7; 4]);
    let path = temp(name);
    std::fs::write(&path, encode(&image, "png", Depth::U16, false).unwrap()).unwrap();
    path
}

#[test]
fn an_imported_bitmap_joins_the_active_package() {
    let (mut app, files) = scripted();
    let before = app.session.pkg.resources.len();
    crate::windows::file_action(&mut app, "file.importBitmap");
    let path = png("imported.png");
    answer_all(&mut app, &files, opened(&path));
    assert_eq!(app.session.pkg.resources.len(), before + 1);
    std::fs::remove_file(path).ok();
}

#[test]
fn a_picked_mesh_shows_in_the_3d_view() {
    let (mut app, files) = scripted();
    crate::web_files::load_mesh(&mut app);
    let path = temp("mesh.obj");
    std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    answer_all(&mut app, &files, opened(&path));
    assert!(app.session.custom_mesh.is_some());
    assert_eq!(app.session.view3d.scene, "custom");
    std::fs::remove_file(path).ok();
}

#[test]
fn render_dialog_exports_the_frame_captured_before_a_scene_change() {
    let (mut app, files) = scripted();
    let pixels = std::sync::Arc::new(vec![5, 19, 77, 255, 200, 40, 9, 128]);
    let path = temp("captured-render.png");
    ask(&mut app, PickRequest::save("export"), Then::SaveRender(Box::new((2, 1, pixels.clone()))));
    app.session.view3d.scene = "sphere".into();

    answer_all(&mut app, &files, saved(&path));

    let written = image::load_from_memory(&std::fs::read(&path).unwrap()).unwrap().to_rgba8();
    assert_eq!((written.width(), written.height(), written.as_raw()), (2, 1, pixels.as_ref()));
    assert_eq!(std::sync::Arc::strong_count(&pixels), 1, "the answered dialog retained the frame");
    assert!(app.session.finish_display_handoff().is_none());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn geometry_dialog_exports_the_mesh_captured_before_a_scene_change() {
    let (mut app, files) = scripted();
    let mesh = std::sync::Arc::new(materialcraft_engine::geom::primitives::cube(1.0));
    let path = temp("captured-mesh.obj");
    ask(&mut app, PickRequest::save("export"), Then::ExportGeometry(mesh.clone()));
    app.session.custom_mesh = Some(("newer".into(), std::sync::Arc::new(materialcraft_engine::geom::primitives::sphere(1.0, 8, 4))));

    answer_all(&mut app, &files, saved(&path));

    assert_eq!(std::fs::read_to_string(&path).unwrap(), materialcraft_engine::formats::mesh::obj_text(&mesh).unwrap());
    assert_eq!(std::sync::Arc::strong_count(&mesh), 1, "the answered dialog retained the mesh");
    assert!(app.session.finish_display_handoff().is_none());
    std::fs::remove_file(path).unwrap();
}

#[cfg(unix)]
#[test]
fn export_dialogs_keep_native_non_utf8_destinations() {
    use std::os::unix::ffi::OsStringExt;

    for geometry in [false, true] {
        let (mut app, files) = scripted();
        let filename: Vec<u8> = format!("mc-export-{}-{geometry}-", std::process::id()).into_bytes().into_iter().chain([0xff]).collect();
        let path = std::env::temp_dir().join(std::ffi::OsString::from_vec(filename));
        if geometry {
            let mesh = std::sync::Arc::new(materialcraft_engine::geom::primitives::cube(1.0));
            ask(&mut app, PickRequest::save("export"), Then::ExportGeometry(mesh.clone()));
            answer_all(&mut app, &files, saved(&path));
            assert_eq!(std::fs::read_to_string(&path).unwrap(), materialcraft_engine::formats::mesh::obj_text(&mesh).unwrap());
        } else {
            ask(&mut app, PickRequest::save("export"), Then::SaveRender(Box::new((1, 1, std::sync::Arc::new(vec![5, 19, 77, 255])))));
            answer_all(&mut app, &files, saved(&path));
            assert_eq!(image::load_from_memory(&std::fs::read(&path).unwrap()).unwrap().to_rgba8().as_raw(), &[5, 19, 77, 255]);
        }
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn the_2d_views_image_is_written_to_the_picked_file() {
    for ext in ["png", "exr"] {
        let (mut app, files) = scripted();
        app.session.cook();
        crate::view2d::save_image(&mut app);
        let path = temp(&format!("view.{ext}"));
        let asked = answer_all(&mut app, &files, saved(&path));
        let [request] = asked.as_slice() else { panic!("{asked:?}") };
        assert!(request.file_name.as_deref().is_some_and(|name| name.ends_with(".png")));
        assert!(materialcraft_engine::formats::decode(&std::fs::read(&path).unwrap(), false).is_ok(), "{ext}");
        std::fs::remove_file(path).ok();
    }
}

#[test]
fn a_picked_input_image_is_bound_on_the_active_graph() {
    let (mut app, files) = scripted();
    app.run("graph.node.add", json!({"def": "input_grayscale", "params": {"identifier": "curvature"}}));
    let path = png("input.png");
    let target = GraphRef::active(&app);
    ask(&mut app, PickRequest::open("import"), Then::BindInput(json!({"identifier": "curvature"}), target));
    answer_all(&mut app, &files, opened(&path));
    assert!(app.session.graph().unwrap().input_images.contains_key("curvature"));
    std::fs::remove_file(path).ok();
}

#[test]
fn the_prompts_save_waits_for_its_dialog_and_then_quits() {
    let (app, files) = modified_app_asking_to_quit();
    let mut harness = answer_prompt_with_save(app);
    let app = harness.state_mut();

    // While the dialog is up, another close asks nothing and the app is not quitting
    assert_eq!(files.open(), 1);
    crate::unsaved::guard(app, Guarded::Quit);
    assert_eq!(files.requests().len(), 1, "no second prompt or pick while the Save dialog is open");
    assert_eq!(crate::unsaved::close_requested(app), CloseDecision::Hold);

    let path = temp("quit.sbs");
    answer_all(app, &files, saved(&path));
    assert!(path.exists());
    assert!(!app.session.modified);
    assert!(crate::unsaved::held(app).is_none());
    assert_eq!(crate::unsaved::close_requested(app), CloseDecision::Close, "the held quit runs once the package is saved");
    std::fs::remove_file(path).ok();
}

#[test]
fn canceling_the_prompts_save_dialog_keeps_the_app_open() {
    let (mut app, files) = modified_app_asking_to_quit();
    app.pending_load = Some(("waiting.sbs".into(), vec![1, 2, 3]));
    let mut harness = answer_prompt_with_save(app);
    let app = harness.state_mut();
    answer_all(app, &files, Picked::Cancelled);
    assert!(app.pending_load.is_none(), "the held load is dropped");
    assert!(app.session.modified);
    assert_ne!(crate::unsaved::close_requested(app), CloseDecision::Close);
}

#[test]
fn a_held_close_waits_for_the_save_dialog() {
    let (app, files) = modified_app_asking_to_quit();
    let mut harness = answer_prompt_with_save(app);
    let app = harness.state_mut();
    assert_eq!(files.open(), 1);
    app.session.modified = false;
    assert_eq!(crate::unsaved::close_requested(app), CloseDecision::Hold);
}

#[test]
fn a_saved_package_goes_to_the_document_that_asked() {
    let mut app = app();
    let first = app.session.document_id;
    app.run("file.newPackage", serde_json::json!({}));
    let second = app.session.document_id;
    assert_ne!(first, second, "a second package is open");

    // Keep adjacent SBS resources in an exclusive directory
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("first.sbs");

    let saved = save_to(&mut app, first, false, &path.display().to_string());
    assert!(saved, "{}", app.toasts.last().map_or("no error toast", |toast| toast.text.as_str()));
    assert_eq!(app.session.document_id, first, "the asking package is active again");
    assert_eq!(app.session.path.as_deref().map(PathBuf::from), std::fs::canonicalize(&path).ok());
}

#[test]
fn saving_a_package_that_closed_meanwhile_does_not_write() {
    let mut app = app();
    let path = temp("gone.sbs");
    assert!(!save_to(&mut app, u64::MAX, false, &path.display().to_string()));
    assert!(!path.exists());
    assert!(!app.toasts.is_empty(), "the error is reported");
}

#[test]
fn export_destination_dialogs_fill_their_tab_field() {
    use crate::export::ExportTab;

    let (mut app, files) = scripted();
    for (tab, name) in [(ExportTab::FromGraph, "images"), (ExportTab::MaterialX, "materialx"), (ExportTab::Mcmat, "material.mcmat")] {
        ask(&mut app, PickRequest::folder("export"), Then::ExportDestination(tab));
        answer_all(&mut app, &files, opened(&std::env::temp_dir().join(name)));
    }

    let draft = app.ui.export_draft.as_ref().unwrap();
    let picked = |name: &str| std::env::temp_dir().join(name).display().to_string();
    assert_eq!(draft.destination, picked("images"));
    assert_eq!(draft.material.folder, picked("materialx"));
    assert_eq!(draft.material.archive, picked("material.mcmat"));
    assert_eq!(app.ui.export, crate::export::ExportSettings::default(), "picking edits the window's copy until it is saved");
}

#[test]
fn a_picked_input_image_is_not_applied_to_another_graph() {
    let (mut app, files) = scripted();
    let target = GraphRef { path: "another graph".into(), ..GraphRef::active(&app) };
    ask(&mut app, PickRequest::open("import"), Then::BindInput(json!({"identifier": "curvature"}), target));
    let before = app.session.history().len();
    answer_all(&mut app, &files, opened(Path::new("image.png")));
    assert_eq!(app.session.history().len(), before);
    assert!(app.toasts.iter().any(|toast| toast.error && toast.text.starts_with("Not applied")), "{}", app.toasts.len());
}

#[test]
fn an_imported_bitmap_is_not_added_to_another_graph() {
    let (mut app, files) = scripted();
    let target = GraphRef { path: "another graph".into(), ..GraphRef::active(&app) };
    ask(&mut app, PickRequest::open("import"), Then::ImportBitmap(target));
    let before = app.session.history().len();
    answer_all(&mut app, &files, opened(Path::new("does-not-exist.png")));
    assert_eq!(app.session.history().len(), before);
    assert!(app.toasts.iter().any(|toast| toast.text.starts_with("Not applied")), "{}", app.toasts.len());
}

#[test]
fn a_graph_that_took_over_the_index_is_a_different_graph() {
    let mut app = app();
    app.run("graph.duplicate", json!({}));
    app.run("graph.open", json!({"graph": 0}));
    let target = GraphRef::active(&app);
    app.run("graph.delete", json!({"graph": 0}));
    assert_eq!(app.session.graph, 0, "another graph holds the index now");
    assert!(GraphRef::active(&app) != target);
}

#[test]
fn a_pick_for_a_renamed_graph_says_the_graph_changed() {
    let (mut app, files) = scripted();
    let target = GraphRef::active(&app);
    app.session.graph_mut().unwrap().name = "renamed".into();
    ask(&mut app, PickRequest::open("import"), Then::ImportBitmap(target));
    answer_all(&mut app, &files, opened(Path::new("does-not-exist.png")));
    assert!(app.toasts.iter().any(|toast| toast.text.contains("graph or package changed")), "{}", app.toasts.len());
}

#[test]
fn a_graph_of_the_same_name_in_another_folder_is_a_different_graph() {
    let mut app = app();
    app.session.graph_mut().unwrap().folder = "first".into();
    let target = GraphRef::active(&app);
    app.session.graph_mut().unwrap().folder = "second".into();
    assert!(GraphRef::active(&app) != target);
}

#[test]
fn a_mesh_picked_for_another_package_is_not_loaded() {
    let (mut app, files) = scripted();
    let document = app.session.document_id + 1;
    ask(&mut app, PickRequest::open("import"), Then::LoadMesh { document });
    answer_all(&mut app, &files, opened(Path::new("does-not-exist.obj")));
    assert!(app.toasts.iter().any(|toast| toast.text.starts_with("Not applied")), "{}", app.toasts.len());
}
