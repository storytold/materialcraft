use craft_ui::shell::embed::CloseDecision;
use egui_kittest::kittest::Queryable as _;

use crate::MaterialCraftApp;
use crate::unsaved::{Guarded, close_requested, guard, held};

/// The prompt drawn on its own, answered by its buttons
fn prompt(app: MaterialCraftApp) -> egui_kittest::Harness<'static, MaterialCraftApp> {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(700.0, 350.0))
        .with_context_setup(|ctx| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(|ui, app: &mut MaterialCraftApp| crate::unsaved::show(app, ui), app);
    harness.run();
    harness
}

fn answer(harness: &mut egui_kittest::Harness<'static, MaterialCraftApp>, button: &str) {
    harness.get_by_label(button).click();
    harness.run();
}

#[test]
fn opening_packages_preserves_the_existing_unsaved_document() {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    app.session.modified = true;
    let bytes = |name: &str| materialcraft_engine::formats::sbs::write_sbs(&materialcraft_engine::core::Package::new(name)).unwrap();
    crate::web_files::load(&mut app, "first.sbs", bytes("First"), false);
    crate::web_files::load(&mut app, "second.sbs", bytes("Second"), false);
    assert!(app.unsaved.is_none());
    assert_eq!(app.session.open_packages().len(), 3);
    let result = crate::control::handle(&mut app, &egui::Context::default(), "ui.set", &serde_json::json!({"prefs": {"thumbnails": false}}));
    assert_eq!(result["ok"], true);
    app.run("package.activate", serde_json::json!({"document": 0})).unwrap();
    assert!(app.session.modified);
    assert_ne!(app.session.pkg.name, "First");
    assert_ne!(app.session.pkg.name, "Second");
}

#[test]
fn quitting_asks_about_each_modified_package() {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    app.session.modified = true;
    app.run("file.newPackage", serde_json::json!({"name": "Second"})).unwrap();
    app.session.modified = true;
    guard(&mut app, Guarded::Quit);
    assert_eq!(held(&app), Some(&Guarded::Quit));
    assert_eq!(app.session.document_id, 0, "the prompt names the first modified package and makes it active");

    let mut harness = prompt(app);
    answer(&mut harness, "Discard");
    let app = harness.state_mut();
    assert_eq!(app.session.pkg.name, "Second", "the prompt moves to the next modified package and makes it active");
    assert_eq!(held(app), Some(&Guarded::Quit));
    assert!(!app.quit_confirmed);

    answer(&mut harness, "Discard");
    assert_eq!(close_requested(harness.state_mut()), CloseDecision::Close);
}

#[test]
fn a_pending_action_is_kept_even_if_the_package_was_saved_elsewhere() {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    app.session.modified = true;
    guard(&mut app, Guarded::Quit);

    guard(&mut app, Guarded::Menu { id: "ui.closeAll".into() });
    assert_eq!(held(&app), Some(&Guarded::Quit));

    app.session.modified = false;
    guard(&mut app, Guarded::Menu { id: "ui.closeAll".into() });
    assert_eq!(held(&app), Some(&Guarded::Quit), "a package saved elsewhere does not let another action replace the held one");
    assert!(!app.quit_confirmed);
}

#[test]
fn closing_one_package_only_prompts_for_that_packages_edits() {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
    app.session.modified = true;
    app.run("file.newPackage", serde_json::json!({"name": "Second"})).unwrap();
    let second = app.session.document_id;
    guard(&mut app, Guarded::Package { document: second, reload: false });
    assert!(app.unsaved.is_none());
    assert_eq!(app.session.document_id, 0);
    assert!(app.session.modified);

    app.run("file.newPackage", serde_json::json!({"name": "Third"})).unwrap();
    let third = app.session.document_id;
    guard(&mut app, Guarded::Package { document: 0, reload: false });
    assert_eq!(held(&app), Some(&Guarded::Package { document: 0, reload: false }));

    let mut harness = prompt(app);
    answer(&mut harness, "Discard");
    let app = harness.state();
    assert!(app.unsaved.is_none());
    assert_eq!(app.session.document_id, third);
    assert_eq!(app.session.open_packages().len(), 1);
}
