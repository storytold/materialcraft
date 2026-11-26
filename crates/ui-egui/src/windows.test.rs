#[cfg(test)]
mod home_modality_tests {
    use std::sync::Arc;

    use craft_host::{PickRequest, ScriptedFiles};
    use egui_kittest::kittest::Queryable as _;

    use crate::{MaterialCraftApp, state::UiState};

    /// Home over a recent package, with a file pick waiting on the user, as the window draws it
    fn home_behind_a_pick() -> (egui_kittest::Harness<'static, MaterialCraftApp>, Arc<ScriptedFiles>) {
        let files = Arc::new(ScriptedFiles::held());
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
        app.host = Arc::new(app.host.with_files(files.clone()));
        app.ui.windows.home = true;
        app.ui.recent.push("recent.sbs".into());
        crate::native_dialogs::ask(&mut app, PickRequest::open("document"), crate::native_dialogs::Then::OpenPackage);
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            .with_context_setup(|ctx| {
                MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
            })
            .build_ui_state(|ui, app: &mut MaterialCraftApp| crate::windows::show(app, ui), app);
        harness.run();
        (harness, files)
    }

    #[test]
    fn a_sample_tile_and_a_recent_link_wait_behind_a_file_dialog() {
        let (mut harness, files) = home_behind_a_pick();
        let path = harness.state().session.path.clone();

        harness.query_all_by_label_contains("recent.sbs").next().expect("the recent row").click();
        harness.run();

        assert_eq!(harness.state().session.path, path, "the recent package waits behind the pick");
        assert!(harness.state().ui.windows.home);

        let card = crate::template_card::cards().into_iter().find(|card| !matches!(card.id, "empty" | "pbr")).expect("a sample template");
        harness.query_all_by_label_contains(card.label).next().expect("the sample card").click();
        harness.run();

        assert!(!harness.state().ui.windows.new_graph, "the sample waits behind the pick");
        assert!(harness.state().ui.windows.home);
        assert_eq!(files.requests().len(), 1, "Home asked for no second pick");
    }
}

#[cfg(test)]
mod open_guard_tests {
    use crate::{MaterialCraftApp, state::UiState};

    #[test]
    fn a_failed_open_keeps_unsaved_work_without_a_discard_prompt() {
        let mut s = materialcraft_engine::sample_session();
        s.modified = true;
        let mut app = MaterialCraftApp::headless(s, UiState::default());
        let before = app.session.pkg.clone();
        crate::windows::open_path(&mut app, "does-not-exist.sbs");
        assert!(app.unsaved.is_none());
        assert!(app.toasts.iter().any(|toast| toast.error));
        assert_eq!(app.session.pkg, before);
        assert!(app.session.modified);
    }

    #[test]
    fn opening_a_path_over_a_clean_package_goes_straight_to_open() {
        let mut s = materialcraft_engine::sample_session();
        s.modified = false;
        let mut app = MaterialCraftApp::headless(s, UiState::default());
        crate::windows::open_path(&mut app, "does-not-exist.sbs");
        assert!(app.unsaved.is_none());
        assert!(app.toasts.iter().any(|toast| toast.error), "the open was attempted and its error reported");
    }
}

#[cfg(test)]
mod import_tests {
    use crate::{MaterialCraftApp, state::UiState};
    use materialcraft_engine::formats::{Depth, encode};
    use materialcraft_engine::image::{Image, Kind};

    #[test]
    fn importing_a_bitmap_file_is_one_undo_entry() {
        let png = encode(&Image::filled(4, 4, Kind::Color, [0.5, 0.2, 0.1, 1.0]), "png", Depth::U8, true).unwrap();
        let path = std::env::temp_dir().join(format!("mc-import-{}.png", std::process::id()));
        std::fs::write(&path, png).unwrap();
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
        let base = app.session.history().len();
        crate::graph::import_bitmap_file(&mut app, &path.display().to_string());
        std::fs::remove_file(&path).ok();
        assert_eq!(app.session.history().len(), base + 1, "history: {:?}", app.session.history());
        assert_eq!(app.session.history().last().map(String::as_str), Some("Import bitmap"));
        app.session.undo();
        assert_eq!(app.session.history().len(), base);
    }
}

#[test]
fn project_dialogs_offer_separate_formats_and_import_filters_show_full_names() {
    use super::{package_filters, resource_filters};
    fn extensions(filters: &[(&'static str, &'static [&'static str])]) -> Vec<&'static str> {
        filters.iter().flat_map(|(_, extensions)| extensions.iter().copied()).collect()
    }
    let saved = package_filters(false, false);

    assert!(saved.iter().all(|(_, extensions)| !extensions.contains(&"mcmat")), "a package is not saved as a portable material");
    assert!(extensions(&saved).contains(&"sbs") && extensions(&saved).contains(&"mcraft"));
    assert_eq!(package_filters(false, true).first().map(|(_, extensions)| *extensions), Some(&["mcraft"][..]), "an archive offers its own format first");

    let opened = package_filters(true, false);
    let (_, all) = opened[0];

    assert!(extensions(&opened[1..]).iter().all(|extension| all.contains(extension)), "the first entry opens every format");
    assert!(opened.last().is_some_and(|(_, extensions)| extensions.contains(&"mcmat")));

    let bitmap = resource_filters(false);
    let vector = resource_filters(true);

    for (label, extensions) in bitmap.iter().chain(&vector) {
        assert!(extensions.iter().all(|extension| label.contains(&format!("*.{extension}"))), "{label}");
    }
    assert_ne!(bitmap, vector);
}
