use std::path::Path;
use std::sync::Arc;

use craft_host::{Picked, ScriptedFiles};
use craft_ui::shell::embed::CloseDecision;
use egui_kittest::kittest::Queryable as _;
use serde_json::json;

use super::*;
use crate::unsaved::{self, Guarded};

type Shell = egui_kittest::Harness<'static, MaterialCraftApp>;

/// A modified package in an app whose file service saves as a browser does; each save waits for the test's answer
fn app() -> (MaterialCraftApp, Arc<ScriptedFiles>) {
    let files = Arc::new(ScriptedFiles::held().in_browser());
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
    app.host = Arc::new(app.host.with_files(files.clone()));
    app.session.modified = true;
    (app, files)
}

/// The browser answers the open save, and the app takes the answer as it does each frame
fn answer(app: &mut MaterialCraftApp, files: &ScriptedFiles, picked: Picked) {
    assert!(files.answer(picked).is_some(), "a download was asked");
    crate::native_dialogs::poll(app);
}

/// The file name the last save asked for
fn asked_name(files: &ScriptedFiles) -> String {
    files.requests().last().and_then(|request| request.file_name.clone()).unwrap_or_default()
}

/// The unsaved-changes prompt and the Save package dialog, as the window draws them
fn shell(app: MaterialCraftApp) -> Shell {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(700.0, 350.0))
        .with_context_setup(|ctx| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(
            |ui, app: &mut MaterialCraftApp| {
                unsaved::show(app, ui);
                PackageSave::show(app, ui);
            },
            app,
        );
    harness.run();
    harness
}

fn press(harness: &mut Shell, label: &str) {
    harness.get_by_label(label).click();
    harness.run();
}

/// Ask to run `action` over the modified package and answer the prompt with Save
fn choose_save(mut app: MaterialCraftApp, action: Guarded) -> Shell {
    unsaved::guard(&mut app, action);
    let mut harness = shell(app);
    press(&mut harness, "Save");
    harness
}

#[test]
fn browser_save_before_close_waits_for_download() {
    for close_all in [false, true] {
        let (app, files) = app();
        let document = app.session.document_id;
        let action = if close_all { Guarded::Menu { id: "ui.closeAll".into() } } else { Guarded::Package { document, reload: false } };

        let mut harness = choose_save(app, action.clone());
        let app = harness.state_mut();

        assert!(app.session.has_package());
        assert!(app.session.modified);
        assert_eq!(app.ui.windows.package_save.as_ref().unwrap().document, document);
        assert_eq!(unsaved::held(app), Some(&action));
        assert!(unsaved::saving(app), "the held action waits for the Save package dialog");

        PackageSave::finish(app, true);
        let name = asked_name(&files);
        assert!(name.ends_with("_sbs.zip"));
        assert!(app.session.has_package(), "{action:?} must wait while the download is open");
        assert!(unsaved::saving(app));

        answer(app, &files, Picked::Saved { path: name.clone().into(), file_type: None });

        assert!(zip::ZipArchive::new(std::io::Cursor::new(files.file(Path::new(&name)).unwrap())).is_ok());
        assert!(!app.session.has_package(), "{action:?} must resume after Download");
        assert!(app.unsaved.is_none());
        assert!(app.ui.windows.package_save.is_none());
    }
}

#[test]
fn browser_save_before_close_keeps_its_document_and_cannot_be_replaced_by_save_copy() {
    let (app, files) = app();
    let document = app.session.document_id;
    let mut harness = choose_save(app, Guarded::Package { document, reload: false });
    let app = harness.state_mut();
    let request = app.ui.windows.package_save.clone();
    app.run("file.newPackage", json!({"name": "Other"})).unwrap();
    let other = app.session.document_id;
    app.session.modified = true;

    PackageSave::request(app, true);

    assert_eq!(app.ui.windows.package_save, request);

    PackageSave::finish(app, true);
    answer(app, &files, Picked::Downloaded(asked_name(&files)));

    assert!(app.session.package(document).is_err());
    assert_eq!(app.session.document_id, other);
    assert!(app.session.modified);
}

#[test]
fn browser_cancel_or_download_error_releases_the_action_and_pending_load() {
    for cancel in [false, true] {
        let (app, files) = app();
        let mut harness = choose_save(app, Guarded::Quit);
        let app = harness.state_mut();
        app.pending_load = Some(("pending.sbs".into(), vec![]));

        if cancel {
            PackageSave::finish(app, false);
        } else {
            PackageSave::finish(app, true);
            answer(app, &files, Picked::Failed("download failed".into()));
        }

        assert!(app.session.has_package());
        assert!(app.session.modified);
        assert!(app.session.path.is_none());
        assert!(app.unsaved.is_none());
        assert!(app.pending_load.is_none());
        assert!(app.ui.windows.package_save.is_none());
        if !cancel {
            assert!(app.toasts.iter().any(|toast| toast.error));
        }
        assert_eq!(unsaved::close_requested(app), CloseDecision::Hold, "the abandoned quit must not close the window");
        app.unsaved = None;

        PackageSave::save(app);
        assert!(app.ui.windows.package_save.is_some(), "the package still needs a name");
        PackageSave::finish(app, true);
        answer(app, &files, Picked::Downloaded(asked_name(&files)));

        assert!(!app.session.modified);
        assert!(app.unsaved.is_none());
        assert!(!app.quit_confirmed, "a later save must not resume the abandoned quit");
    }
}

#[test]
fn browser_download_for_a_closed_document_cancels_the_held_action() {
    let (app, files) = app();
    let mut harness = choose_save(app, Guarded::Menu { id: "ui.closeAll".into() });
    let app = harness.state_mut();
    app.run("package.close", json!({"discard": true})).unwrap();
    app.run("file.newPackage", json!({"name": "Other"})).unwrap();
    app.session.modified = true;

    PackageSave::finish(app, true);

    assert!(files.requests().is_empty(), "a closed package must not download");
    assert_eq!(app.session.pkg.name, "Other");
    assert!(app.session.modified);
    assert!(app.session.path.is_none());
    assert!(app.unsaved.is_none());
    assert!(app.ui.windows.package_save.is_none());
}

#[test]
fn browser_close_all_and_quit_wait_for_each_modified_package() {
    for action in [Guarded::Menu { id: "ui.closeAll".into() }, Guarded::Quit] {
        let (mut app, files) = app();
        let first = app.session.document_id;
        app.run("file.newPackage", json!({"name": "Second"})).unwrap();
        let second = app.session.document_id;
        app.session.modified = true;

        let mut harness = choose_save(app, action.clone());
        let app = harness.state_mut();
        assert_eq!(app.ui.windows.package_save.as_ref().unwrap().document, first);
        PackageSave::finish(app, true);
        answer(app, &files, Picked::Downloaded(asked_name(&files)));

        assert!(!app.quit_confirmed);
        assert_eq!(unsaved::held(app), Some(&action), "the prompt moves on to the second package");
        assert_eq!(app.session.document_id, second);
        assert!(app.session.modified);

        harness.run();
        press(&mut harness, "Save");
        let app = harness.state_mut();
        assert_eq!(app.ui.windows.package_save.as_ref().unwrap().document, second);
        PackageSave::finish(app, true);
        answer(app, &files, Picked::Downloaded(asked_name(&files)));

        assert!(!unsaved::needs_confirmation(&app.session));
        assert!(app.unsaved.is_none());
        if action == Guarded::Quit {
            assert_eq!(unsaved::close_requested(app), CloseDecision::Close);
        } else {
            assert!(!app.session.has_package());
        }
    }
}

#[test]
fn browser_save_with_an_existing_path_resumes_only_on_success() {
    for success in [false, true] {
        let (mut app, files) = app();
        app.session.path = Some("existing.sbs".into());

        let mut harness = choose_save(app, Guarded::Quit);
        let app = harness.state_mut();
        assert!(app.ui.windows.package_save.is_none(), "a package with a path downloads without asking for a name");
        assert_eq!(asked_name(&files), "existing_sbs.zip");

        let picked = if success { Picked::Downloaded("existing_sbs.zip".into()) } else { Picked::Failed("download failed".into()) };
        answer(app, &files, picked);

        assert_eq!(app.session.modified, !success);
        assert!(app.unsaved.is_none());
        let expected = if success { CloseDecision::Close } else { CloseDecision::Hold };
        assert_eq!(unsaved::close_requested(app), expected);
    }
}

#[test]
fn browser_save_copy_preserves_the_original_path_and_unsaved_changes() {
    for archive in [false, true] {
        let (mut app, files) = app();
        app.session.path = Some("original.sbs".into());
        PackageSave::request(&mut app, true);
        let request = app.ui.windows.package_save.as_mut().unwrap();
        request.archive = archive;
        request.name = "Copy".into();

        PackageSave::finish(&mut app, true);
        let name = asked_name(&files);
        assert_eq!(name, if archive { "Copy.mcraft" } else { "Copy_sbs.zip" });
        answer(&mut app, &files, Picked::Saved { path: name.clone().into(), file_type: None });

        assert!(zip::ZipArchive::new(std::io::Cursor::new(files.file(Path::new(&name)).unwrap())).is_ok());
        assert!(app.session.modified);
        assert_eq!(app.session.path.as_deref(), Some("original.sbs"));
        assert!(app.unsaved.is_none());
    }
}

#[test]
fn browser_save_dialog_buttons_and_escape_complete_or_cancel_the_close() {
    // PLAN: the sheet names the title row's close "Close window"; the framework's title-row close is labelled "Close" (`sections.titleRowClose`).
    for button in ["Download", "Cancel", "Close", "Escape"] {
        let (app, files) = app();
        let document = app.session.document_id;
        let mut harness = choose_save(app, Guarded::Package { document, reload: false });

        if button == "Escape" {
            harness.key_press(egui::Key::Escape);
        } else {
            harness.get_by_label(button).click();
        }
        harness.run();

        let app = harness.state_mut();
        if button == "Download" {
            answer(app, &files, Picked::Downloaded(asked_name(&files)));
        } else {
            assert!(files.requests().is_empty(), "dismissing the dialog must not download ({button})");
        }
        assert!(app.ui.windows.package_save.is_none(), "{button}");
        assert!(app.unsaved.is_none(), "{button}");
        assert_eq!(app.session.has_package(), button != "Download", "{button}");
        if button != "Download" {
            assert!(app.session.modified);
        }
    }
}
