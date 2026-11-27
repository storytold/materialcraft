#[cfg(test)]
mod lib_tests {
    use crate::MaterialCraftApp;
    use crate::state::UiState;
    use materialcraft_engine::sample_session;
    use serde_json::json;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn autosave_leaves_one_snapshot_worker_in_flight() {
        let mut app = MaterialCraftApp::headless(sample_session(), UiState::default());

        // The revision follows edits and undo, not selection.
        let start = app.session.revision();
        app.run("graph.node.add", json!({"def": "levels"})).unwrap();
        let edited = app.session.revision();
        assert_ne!(edited, start, "an edit advances the revision");
        app.run("edit.selectNone", json!({})).unwrap();
        assert_eq!(app.session.revision(), edited, "a selection is no edit");
        app.run("edit.undo", json!({})).unwrap();
        assert_ne!(app.session.revision(), edited, "an undo advances the revision");

        // Jobs that wait for the test: the first backup stays in flight.
        let jobs = std::sync::Arc::new(craft_host::ManualJobs::new());
        app.host = std::sync::Arc::new(MaterialCraftApp::host_builder().jobs(jobs.clone()).build());
        app.ui.prefs.autosave_minutes = 1;
        app.session.modified = true;
        let interval = std::time::Duration::from_secs(60);
        let now = std::time::Instant::now();
        crate::autosave::tick(&mut app, now);
        assert_eq!(jobs.waiting(), 0, "the first tick starts the clock");

        for passed in 1..=3 {
            // A new revision each interval, so only the job in flight holds the next write back.
            app.run("graph.node.add", json!({"def": "levels"})).unwrap();
            crate::autosave::tick(&mut app, now + interval * passed + std::time::Duration::from_secs(1));
            assert_eq!(jobs.waiting(), 1, "one backup write in flight after interval {passed}");
        }
    }

    #[test]
    fn copy_and_cut_fill_the_system_clipboard_in_material_and_function_canvases() {
        use crate::function::{FunctionEditor, FunctionTarget};
        use materialcraft_engine::core::{FunctionGraph, NodeId, Value};

        let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
        let owner = NodeId(app.run("graph.node.add", json!({"def": "pixel_processor"})).unwrap()["id"].as_u64().unwrap());
        let target = materialcraft_engine::FunctionGraphTarget::Parameter { node: owner, parameter: "function".into() };
        app.run("graph.function.set", json!({"target": target, "function": FunctionGraph::constant(Value::Float(0.5))})).unwrap();
        let context = egui::Context::default();
        // `invoke` asks the modality, which reads the window's dialog host.
        MaterialCraftApp::install_runtime(&context, None).unwrap();

        crate::menus::invoke(&mut app, &context, "edit.selectAll");
        crate::menus::invoke(&mut app, &context, "edit.copy");
        let material = app.system_clipboard.take().unwrap();
        app.function_editor = Some(FunctionEditor::new(app.session.graph, owner, FunctionTarget::Parameter("function".into())));
        crate::menus::invoke(&mut app, &context, "edit.selectAll");
        crate::menus::invoke(&mut app, &context, "edit.cut");
        let function = app.system_clipboard.take().unwrap();

        assert_eq!(serde_json::from_str::<serde_json::Value>(&material).unwrap()["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&function).unwrap(), serde_json::to_value(&app.session.embedded_clipboard).unwrap());
        assert!(app.session.embedded_clipboard.is_some());
    }

    #[test]
    fn run_reports_errors_as_toasts_and_keeps_the_session() {
        let mut app = MaterialCraftApp::headless(sample_session(), UiState::default());
        assert!(app.run("no.such.command", json!({})).is_none());
        assert!(app.toasts.iter().any(|t| t.error));
        assert!(app.run("graph.node.add", json!({"def": "levels"})).is_some());
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod title_bar_tests {
    use crate::MaterialCraftApp;
    use crate::state::UiState;
    use craft_window::policy::{Backend, Decision, Desktop, Frame, Os, Runtime, TitleBarPref, WindowKind, decide};

    /// The frame Windows gets for `pref`: the system's, or the app's bar with Windows captions.
    fn decision(pref: TitleBarPref) -> Decision {
        decide(Os::Windows, Runtime::Native, Backend::X11, &Desktop::detect(Backend::X11), WindowKind::Main, pref, None)
    }

    /// The accessible labels of the window after two frames of the in-crate frame loop over an app whose runtime was
    /// installed with `frame`.
    fn labels(frame: Decision) -> Vec<String> {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(20));
            eprintln!("DIAG the_title_bar_shows_window_buttons_only_when_the_app_draws_the_frame: watchdog exit");
            std::process::exit(3);
        });
        let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
        let ctx = egui::Context::default();
        MaterialCraftApp::install_runtime(&ctx, Some(frame.clone())).unwrap();
        app.frame = Some(frame);
        ctx.enable_accesskit();
        let mut labels = Vec::new();
        for _ in 0..2 {
            let mut raw = egui::RawInput::default();
            eframe::App::raw_input_hook(&mut app, &ctx, &mut raw);
            eprintln!("DIAG the_title_bar_shows_window_buttons_only_when_the_app_draws_the_frame: run_ui");
            let mut output = ctx.run_ui(raw, |ui| app.draw(ui));
            output.textures_delta.clear();
            eprintln!("DIAG the_title_bar_shows_window_buttons_only_when_the_app_draws_the_frame: step");
            app.step(&ctx);
            eprintln!("DIAG the_title_bar_shows_window_buttons_only_when_the_app_draws_the_frame: stepped");
            labels = output
                .platform_output
                .accesskit_update
                .map(|update| update.nodes.iter().filter_map(|(_, node)| node.label().map(|label| label.to_string())).collect())
                .unwrap_or_default();
        }
        labels
    }

    #[test]
    fn the_title_bar_shows_window_buttons_only_when_the_app_draws_the_frame() {
        let native = decision(TitleBarPref::System);
        assert_eq!(native.frame, Frame::Native);
        let shown = labels(native);
        assert!(!shown.iter().any(|label| label.starts_with("Close")), "the system draws the window buttons: {shown:?}");

        let integrated = decision(TitleBarPref::Integrated);
        assert!(matches!(integrated.frame, Frame::Integrated(_)));
        let shown = labels(integrated);
        for button in ["Minimize", "Maximize", "Close"] {
            assert!(shown.iter().any(|label| label.starts_with(button)), "the app's bar shows {button}: {shown:?}");
        }
    }
}

#[cfg(test)]
mod unsaved_state_tests {
    use crate::unsaved::{Guarded, guard};
    use crate::{MaterialCraftApp, state::UiState};

    fn app(modified: bool) -> MaterialCraftApp {
        let mut s = materialcraft_engine::sample_session();
        s.modified = modified;
        MaterialCraftApp::headless(s, UiState::default())
    }

    #[test]
    fn closing_all_over_unsaved_work_waits_for_an_answer() {
        let mut a = app(false);
        a.run("graph.node.add", serde_json::json!({"def": "levels"})).unwrap();
        assert!(a.session.modified, "the edit modified the package");
        let before = a.session.pkg.name.clone();
        let graphs = a.session.pkg.graphs.len();

        guard(&mut a, Guarded::Menu { id: "ui.closeAll".into() });
        assert!(a.unsaved.is_some(), "the prompt holds the close");
        assert_eq!((a.session.pkg.name.clone(), a.session.pkg.graphs.len()), (before.clone(), graphs), "nothing was replaced yet");

        a.unsaved = None;
        assert!(a.session.modified);
        assert_eq!(a.session.pkg.name, before);
    }

    #[test]
    fn closing_all_over_a_clean_package_just_happens() {
        let mut a = app(false);
        guard(&mut a, Guarded::Menu { id: "ui.closeAll".into() });

        assert!(a.unsaved.is_none());
        assert!(a.session.pkg.graphs.is_empty());
        assert!(a.session.open_packages().is_empty());
    }
}
