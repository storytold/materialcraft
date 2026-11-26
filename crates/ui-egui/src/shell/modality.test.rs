use std::path::{Path, PathBuf};
use std::sync::Arc;

use craft_ui::dialog::{Alert, HostedAlert};
use craft_ui::runtime::env::WindowKey;
use craft_ui::shell::WindowDialogs;

use super::{Blocker, Modality};
use crate::{MaterialCraftApp, UiState};

fn app() -> MaterialCraftApp {
    MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default())
}

fn capture(app: &MaterialCraftApp) -> Modality {
    Modality::capture(app, &egui::Context::default())
}

/// The unsaved-changes prompt as a user opens it: an edit, then Quit.
fn prompt(app: &mut MaterialCraftApp) {
    app.run("edit.selectAll", serde_json::json!({}));
    app.run("edit.delete", serde_json::json!({}));
    crate::unsaved::guard(app, crate::unsaved::Guarded::Quit);
}

/// A native file dialog asked for and waiting.
fn file_dialog(app: &mut MaterialCraftApp) {
    crate::native_dialogs::ask(app, crate::windows::package_dialog(true, false), crate::native_dialogs::Then::OpenPackage);
}

/// A modal alert open on the main window's dialog host of `ctx`.
fn host_modal(ctx: &egui::Context) {
    WindowDialogs::of(ctx, &WindowKey::main()).with(|host| {
        host.open(HostedAlert::new(Alert::new("Could not open", "The file is damaged.")));
    });
}

#[test]
fn home_and_the_shortcuts_list_leave_input_free() {
    let mut app = app();
    assert_eq!(capture(&app), Modality::Free);
    app.ui.windows.home = true;
    assert_eq!(capture(&app), Modality::Free, "Home is modeless");
}

#[test]
fn every_blocking_source_blocks() {
    type Open = fn(&mut MaterialCraftApp);
    // PLAN: `ExposeParameterDialog::open` is `pub(super)` in `property` (panels-w12), so this module cannot open the
    // `expose_parameter_dialog` source; its case waits for an opener the tests can reach.
    let cases: [(&str, Open, Blocker); 9] = [
        ("new graph", |app| app.ui.windows.new_graph = true, Blocker::Dialog),
        ("export", |app| app.ui.windows.export = true, Blocker::Dialog),
        ("preferences", |app| app.ui.windows.preferences = true, Blocker::Dialog),
        ("about", |app| app.ui.windows.about = true, Blocker::Dialog),
        ("web save", |app| crate::web_files::PackageSave::request(app, false), Blocker::Dialog),
        ("the shortcuts list", |app| app.shortcuts = Some(craft_menu::shortcuts::DialogState::new()), Blocker::Dialog),
        ("unsaved prompt", prompt, Blocker::UnsavedPrompt),
        ("queued file dialog", file_dialog, Blocker::FileDialog),
        ("update prompt", |app| crate::updates_ui::open_prompt_for_test(app), Blocker::Update),
    ];

    let mut wrong: Vec<_> = cases
        .into_iter()
        .filter_map(|(name, open, blocker)| {
            let mut app = app();
            open(&mut app);
            (capture(&app) != Modality::Blocked(blocker)).then_some(name)
        })
        .collect();

    let free = app();
    let ctx = egui::Context::default();
    host_modal(&ctx);
    if Modality::capture(&free, &ctx) != Modality::Blocked(Blocker::Dialog) {
        wrong.push("a modal on the window's dialog host");
    }
    assert!(wrong.is_empty(), "not reported as blocking: {wrong:?}");
}

#[test]
fn a_blocker_lets_only_its_own_commands_through() {
    assert!(Modality::Free.allows("edit.delete"));
    for id in ["edit.delete", "file.open", "file.save"] {
        assert!(!Modality::Blocked(Blocker::UnsavedPrompt).allows(id), "{id}");
        assert!(!Modality::Blocked(Blocker::Dialog).allows(id), "{id}");
        assert!(!Modality::Blocked(Blocker::Update).allows(id), "{id}");
    }

    let file_dialog = Modality::Blocked(Blocker::FileDialog);
    for id in ["file.open", "file.saveAs", "file.saveCopyAs", "file.importBitmap", "file.importVector"] {
        assert!(file_dialog.allows(id), "{id}");
    }
    assert!(!file_dialog.allows("file.exportMaterial"), "Export material opens the Export window, which waits for the file dialog");
    assert!(!file_dialog.allows("file.save"), "File > Save writes without showing a dialog when the package has a path");
    assert!(!file_dialog.allows("edit.delete"));
}

#[test]
fn a_file_dialog_frees_its_commands_only_when_nothing_else_blocks() {
    let mut alone = app();
    file_dialog(&mut alone);
    assert!(capture(&alone).allows("file.open"), "the dialog alone lets Open bring it forward");

    let mut preferences = app();
    file_dialog(&mut preferences);
    preferences.ui.windows.preferences = true;
    assert_eq!(capture(&preferences), Modality::Blocked(Blocker::Dialog));
    assert!(!capture(&preferences).allows("file.open"));

    let mut update = app();
    file_dialog(&mut update);
    crate::updates_ui::open_prompt_for_test(&mut update);
    assert_eq!(capture(&update), Modality::Blocked(Blocker::Update));
    assert!(!capture(&update).allows("file.open"));

    let mut modal = app();
    file_dialog(&mut modal);
    let ctx = egui::Context::default();
    host_modal(&ctx);
    assert_eq!(Modality::capture(&modal, &ctx), Modality::Blocked(Blocker::Dialog));
    assert!(!Modality::capture(&modal, &ctx).allows("file.open"));
}

#[derive(Debug)]
struct Dropped(PathBuf, Vec<u8>);

impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.1.clone())
    }
}

#[test]
fn a_file_dropped_behind_a_prompt_does_not_reach_the_package() {
    use materialcraft_engine::formats::{Depth, encode};
    use materialcraft_engine::image::{Image, Kind};

    let mut harness = egui_kittest::Harness::builder().build_eframe(|cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app()
    });
    harness.step();
    harness.step();
    let png = encode(&Image::filled(2, 2, Kind::Gray, [0.5; 4]), "png", Depth::U8, false).unwrap();
    let dropped = || -> egui::DroppedFileHandle { Arc::new(Dropped("dropped.png".into(), png.clone())) };

    prompt(harness.state_mut());
    let nodes = harness.state().session.graph().unwrap().nodes.len();
    harness.input_mut().dropped_files.push(dropped());
    harness.step();
    assert_eq!(harness.state().session.graph().unwrap().nodes.len(), nodes, "the drop waits behind the prompt");

    harness.state_mut().unsaved = None;
    harness.input_mut().dropped_files.push(dropped());
    harness.step();
    assert_eq!(harness.state().session.graph().unwrap().nodes.len(), nodes + 1, "the drop works once the prompt is answered");
}
