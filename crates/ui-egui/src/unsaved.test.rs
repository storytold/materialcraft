#[cfg(test)]
mod unsaved_tests {
    use crate::unsaved::{Choice, Guarded, Step, answer};

    fn open() -> Guarded {
        Guarded::OpenPath { path: "a.sbs".into() }
    }

    #[test]
    fn cancel_drops_discard_runs_save_saves_first() {
        let mut pending = Some(open());
        assert_eq!(answer(&mut pending, Choice::Cancel), Step::Stop);
        assert_eq!(pending, None);

        let mut pending = Some(open());
        assert_eq!(answer(&mut pending, Choice::Discard), Step::Run(open()));
        assert_eq!(pending, None);

        let mut pending = Some(open());
        assert_eq!(answer(&mut pending, Choice::Save), Step::Save(open()));
        assert_eq!(pending, None);

        assert_eq!(answer(&mut None, Choice::Discard), Step::Stop, "nothing pending, nothing to run");
    }
}

#[test]
fn opening_packages_preserves_the_existing_unsaved_document() {
    let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    app.session.modified = true;
    let bytes = |name: &str| materialcraft_engine::formats::sbs::write_sbs(&materialcraft_engine::core::Package::new(name)).unwrap();
    crate::web_files::load(&mut app, "first.sbs", bytes("First"), false);
    crate::web_files::load(&mut app, "second.sbs", bytes("Second"), false);
    assert!(app.ui.windows.unsaved.is_none());
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
    use crate::unsaved::{Guarded, guard, perform};
    let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    app.session.modified = true;
    app.run("file.newPackage", serde_json::json!({"name": "Second"})).unwrap();
    app.session.modified = true;
    guard(&mut app, Guarded::Quit);
    assert_eq!(app.session.document_id, 0);
    app.ui.windows.unsaved = None;
    perform(&mut app, Guarded::Quit);
    assert!(!app.quit_confirmed);
    assert_eq!(app.session.pkg.name, "Second");
    assert_eq!(app.ui.windows.unsaved, Some(Guarded::Quit));
    app.ui.windows.unsaved = None;
    perform(&mut app, Guarded::Quit);
    assert!(app.quit_confirmed);
}

#[test]
fn a_pending_action_is_kept_even_if_the_package_was_saved_elsewhere() {
    let first = crate::unsaved::Guarded::Quit;
    let mut pending = Some(first.clone());
    assert!(crate::unsaved::request(&mut pending, false, crate::unsaved::Guarded::Menu { id: "ui.closeAll".into() }).is_none());
    assert_eq!(pending, Some(first));
}

#[test]
fn closing_one_package_only_prompts_for_that_packages_edits() {
    use crate::unsaved::{Guarded, guard, perform};
    let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::new_session(), crate::UiState::default());
    app.session.modified = true;
    app.run("file.newPackage", serde_json::json!({"name": "Second"})).unwrap();
    let second = app.session.document_id;
    guard(&mut app, Guarded::Package { document: second, reload: false });
    assert!(app.ui.windows.unsaved.is_none());
    assert_eq!(app.session.document_id, 0);
    assert!(app.session.modified);

    app.run("file.newPackage", serde_json::json!({"name": "Third"})).unwrap();
    let third = app.session.document_id;
    guard(&mut app, Guarded::Package { document: 0, reload: false });
    assert_eq!(app.ui.windows.unsaved, Some(Guarded::Package { document: 0, reload: false }));
    app.ui.windows.unsaved = None;
    perform(&mut app, Guarded::Package { document: 0, reload: false });
    assert_eq!(app.session.document_id, third);
    assert_eq!(app.session.open_packages().len(), 1);
}

#[test]
fn hebrew_unsaved_prompt_isolates_mixed_direction_document_names() {
    crate::i18n::set_language("he");
    let title = crate::unsaved::save_changes_prompt("פרויקט_Studio-42.sbs");
    assert!(title.contains("\u{2068}פרויקט_Studio-42.sbs\u{2069}"), "{title}");
    assert!(title.starts_with("לשמור שינויים בקובץ"));
    crate::i18n::set_language("en");
    let english = crate::unsaved::save_changes_prompt("פרויקט_Studio-42.sbs");
    assert!(!english.contains('\u{2068}'));
    assert!(english.starts_with("Save changes to"));
}

#[test]
fn hebrew_unsaved_dialog_mirrors_save_discard_cancel_buttons() {
    use egui_kittest::kittest::{By, Queryable};
    let mut app = crate::MaterialCraftApp::headless(materialcraft_engine::sample_session(), crate::UiState::default());
    app.session.modified = true;
    app.ui.windows.unsaved = Some(crate::unsaved::Guarded::Quit);
    crate::i18n::set_language("he");
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(960.0, 480.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut crate::MaterialCraftApp| crate::unsaved::show(app, ui.ctx()), app);
    harness.run_steps(3);
    let x = |harness: &egui_kittest::Harness<'_, crate::MaterialCraftApp>, label: &str| {
        harness.get(By::new().role(egui::accesskit::Role::Button).label(label)).rect().center().x
    };
    let save = x(&harness, "שמירה");
    let discard = x(&harness, "לא לשמור");
    let cancel = x(&harness, "ביטול");
    assert!(save > discard && discard > cancel, "RTL actions: save={save}, discard={discard}, cancel={cancel}");
    crate::i18n::set_language("en");
    harness.run_steps(3);
    let save = x(&harness, "Save");
    let discard = x(&harness, "Discard");
    let cancel = x(&harness, "Cancel");
    assert!(save < discard && discard < cancel, "LTR actions: save={save}, discard={discard}, cancel={cancel}");
}
