//! The faces of function nodes

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;

fn harness(app: MaterialCraftApp) -> Harness<'static, MaterialCraftApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .with_step_dt(0.05)
        .with_context_setup(|ctx| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(
            |ui, app: &mut MaterialCraftApp| {
                app.step(&ui.ctx().clone());
                app.draw(ui);
            },
            app,
        );
    harness.run_steps(6);
    harness
}

#[test]
fn a_constant_node_fits_a_field_for_each_component() {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.run("file.new", json!({"kind": "function", "name": "Faces"})).unwrap();
    app.run("graph.function.edit", json!({"edit": {"action": "add", "operation": "const_float4", "at": [0, 0]}})).unwrap();
    app.run("graph.function.edit", json!({"edit": {"action": "add", "operation": "add", "at": [200, 0]}})).unwrap();
    let harness = harness(app);

    assert!(harness.state().ui.graph_camera.zoom >= 0.45, "value controls require an editable graph zoom");
    let card = harness.get(By::new().role(Role::GraphicsObject).label("Float4")).rect();
    for component in ["X", "Y", "Z", "W"] {
        let field = harness.get(By::new().role(Role::SpinButton).label(component)).rect();

        assert!(card.contains_rect(field), "{component} lies inside its node: {field:?} in {card:?}");
    }

    let operator = harness.get(By::new().role(Role::GraphicsObject).label("Add")).rect();

    assert!(operator.height() < card.height(), "an operator is a compact node");
    assert!(operator.height() < operator.width(), "an operator has no empty preview square");
}
