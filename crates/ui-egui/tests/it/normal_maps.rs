use egui::vec2;
use egui_kittest::{Harness, kittest::Queryable};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;
use std::time::{Duration, Instant};

fn app() -> MaterialCraftApp {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::english());
    let node = app.run("graph.node.add", json!({"def": "uniform", "params": {"color": [0.5, 0.75, 1.0, 1.0]}})).unwrap()["id"].as_u64().unwrap();
    app.run("graph.node.add", json!({"def": "output", "connectFrom": {"node": node}, "params": {"usage": 1}})).unwrap();
    app
}

fn settle(harness: &mut Harness<'static, MaterialCraftApp>, after: u64) {
    // Initial lighting preparation and full-quality CPU renders share Rayon workers with
    // other UI tests; ten seconds can expire while valid work is still running on CI.
    const RENDER_TIMEOUT: Duration = Duration::from_secs(60);
    const POLL_INTERVAL: Duration = Duration::from_millis(10);
    let deadline = Instant::now() + RENDER_TIMEOUT;
    while Instant::now() < deadline {
        harness.step();
        let app = harness.state();
        if app.view3d.renders > after && !app.session.dirty && !app.cooker.busy() && !app.view3d.busy() {
            return;
        }

        std::thread::sleep(POLL_INTERVAL);
    }
    let app = harness.state();
    panic!(
        "the normal-map preview did not finish rendering: renders={} (after {after}), dirty={}, cooker_busy={}, view3d_busy={}",
        app.view3d.renders,
        app.session.dirty,
        app.cooker.busy(),
        app.view3d.busy()
    );
}

/// Show the 3D view's material in the Properties panel, through the view's Materials menu
fn open_material_properties(h: &mut Harness<'static, MaterialCraftApp>) {
    for label in ["Materials", "Default", "Edit properties"] {
        h.get_by_label(label).click();
        h.run_steps(4);
    }
}

fn toggle(h: &mut Harness<'static, MaterialCraftApp>) {
    h.get_by_label("Flip normal map Y (DirectX)").click();
    h.run_steps(4);
}

#[test]
fn display_toggle_updates_the_session_and_rerenders_the_normal_map() {
    let mut h = Harness::builder().with_size(vec2(1400.0, 860.0)).build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app(),
    );
    settle(&mut h, 0);
    let before = h.state().view3d.shown.clone().unwrap();
    let rendered = h.state().view3d.renders;
    open_material_properties(&mut h);
    toggle(&mut h);
    assert!(!h.state().session.view3d.normal_y_flip);
    settle(&mut h, rendered);
    assert!(before.2 != h.state().view3d.shown.as_ref().unwrap().2, "the flipped normal map renders differently");

    // The same check box restores the DirectX setting
    let rendered = h.state().view3d.renders;
    toggle(&mut h);
    assert!(h.state().session.view3d.normal_y_flip);
    settle(&mut h, rendered);
    assert!(before.2 == h.state().view3d.shown.as_ref().unwrap().2, "the first picture is back");
}

#[test]
#[ignore = "requires an offscreen GPU; run explicitly with --ignored"]
fn normal_convention_snapshot() {
    let mut harness = Harness::builder().with_size(vec2(1400.0, 860.0)).wgpu().build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        app(),
    );
    settle(&mut harness, 0);
    let rendered = harness.state().view3d.renders;
    open_material_properties(&mut harness);
    toggle(&mut harness);
    settle(&mut harness, rendered);
    assert!(!harness.state().session.view3d.normal_y_flip);
    let dir = std::path::PathBuf::from(std::env::var_os("MC_SNAPSHOT_DIR").expect("MC_SNAPSHOT_DIR"));
    std::fs::create_dir_all(&dir).unwrap();
    harness.render().unwrap().save(dir.join("normal-convention.png")).unwrap();
}
