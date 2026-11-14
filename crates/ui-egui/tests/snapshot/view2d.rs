//! Renders of the 2D view alone: its toolbars, its panels and its display settings

use egui::vec2;
use egui_kittest::Harness;
use materialcraft_ui_egui::{MaterialCraftApp, UiState, view2d};
use serde_json::json;

fn view(app: MaterialCraftApp) -> Harness<'static, MaterialCraftApp> {
    Harness::builder().with_context_setup(materialcraft_ui_egui::theme::install).with_size(vec2(900.0, 640.0)).with_pixels_per_point(1.0).wgpu().build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.apply_prefs();
            view2d::show(app, ui);
        },
        app,
    )
}

fn save(harness: &mut Harness<'static, MaterialCraftApp>, name: &str) {
    let Some(directory) = std::env::var_os("MC_SNAPSHOT_DIR").map(std::path::PathBuf::from) else { return };

    std::fs::create_dir_all(&directory).unwrap();
    harness.run_steps(4);
    harness.render().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
}

#[test]
#[ignore = "requires an offscreen GPU; run explicitly with --ignored"]
fn the_2d_view_shows_its_settings() {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    let blur = app.session.graph().unwrap().nodes.iter().find(|node| node.def == "blur").unwrap().id;
    app.run("view.2d", json!({"node": blur.0})).unwrap();
    app.session.cook();
    let mut harness = view(app);
    save(&mut harness, "view2d-plain");

    harness.state_mut().run("view.2d", json!({"information": true, "histogram": true, "sampler": [0.3, 0.4], "histogramSelection": [0.4, 0.7]})).unwrap();
    save(&mut harness, "view2d-panels");

    harness.state_mut().run("view.2d", json!({"tiled": true, "information": false, "histogramSqrt": true, "uv": true})).unwrap();
    harness.state_mut().ui.image_camera = materialcraft_ui_egui::state::ImageCamera { zoom: 0.25, offset: [0.0, 0.0], fit: false };
    save(&mut harness, "view2d-tiled");

    let color =
        harness.state_mut().run("graph.node.add", json!({"def": "uniform", "params": {"color": [0.9, 0.4, 0.1, 0.5]}})).unwrap()["id"].as_u64().unwrap();
    harness.state_mut().session.cook();
    harness.state_mut().run("view.2d", json!({"node": color, "tiled": false, "uv": false, "checkerboard": true, "histogramChannel": "rgb"})).unwrap();
    harness.state_mut().ui.image_camera = materialcraft_ui_egui::state::ImageCamera::default();
    save(&mut harness, "view2d-checkerboard");

    harness.state_mut().run("view.2d", json!({"checkerboard": false, "srgb": false, "channel": "r", "histogram": false})).unwrap();
    save(&mut harness, "view2d-red-linear");
}

#[test]
#[ignore = "requires an offscreen GPU; run explicitly with --ignored"]
fn the_docked_2d_view_fits_its_toolbars_and_panels() {
    let Some(directory) = std::env::var_os("MC_SNAPSHOT_DIR").map(std::path::PathBuf::from) else { return };

    std::fs::create_dir_all(&directory).unwrap();
    let app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    let mut harness = super::harness(app, 1600.0, 960.0);
    super::settle(&mut harness, 40);
    super::save(&mut harness, &directory, "view2d-docked-plain");

    harness.state_mut().run("view.2d", json!({"information": true, "histogram": true, "sampler": [0.5, 0.5], "tiled": true, "uv": true})).unwrap();
    super::settle(&mut harness, 30);
    super::save(&mut harness, &directory, "view2d-docked-panels");
}
