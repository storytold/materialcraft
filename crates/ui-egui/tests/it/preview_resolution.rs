//! Node cards distinguish editing previews from configured graph resolution.
use egui::{Event, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use materialcraft_engine::core::NodeId;
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;

fn harness(preview: u32) -> (Harness<'static, MaterialCraftApp>, NodeId) {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.ui.prefs.preview_size = preview;
    app.ui.graph_camera.zoom = 1.0;
    let id = NodeId(app.run("graph.node.add", json!({"def":"uniform"})).unwrap()["id"].as_u64().unwrap());
    app.apply_prefs();
    app.session.cook();
    let mut h = Harness::builder()
        .with_size(vec2(1600.0, 960.0))
        .with_step_dt(1.0 / 60.0)
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
    h.run_steps(10);
    (h, id)
}

/// The node card's footer caption, which the node publishes as its accessible description.
fn footer(h: &Harness<'static, MaterialCraftApp>) -> String {
    let node = h.get(By::new().role(egui::accesskit::Role::GraphicsObject).label("Uniform color"));
    node.accesskit_node().description().map(|text| text.to_string()).unwrap_or_default()
}

#[test]
fn node_resolution_explains_the_preview_cap() {
    let (mut h, id) = harness(512);
    let img = h.state().session.cooked.image(id, 0).unwrap();
    assert_eq!((img.width(), img.height()), (512, 512));
    let graph = h.state().session.graph().unwrap();
    let parent_size = graph.size_px();
    let bit_count = h.state().session.graph().unwrap().format.bits();
    let limit = h.state().session.preview_size.expect("the preference caps previews");
    let rect = h.state().graph.rects.iter().find(|(node, _)| *node == id).unwrap().1;
    let footer = footer(&h);
    assert!(footer.contains(&img.width().to_string()) && footer.contains(&img.height().to_string()));
    assert!(footer.contains(&bit_count.to_string()));
    h.event(Event::PointerMoved(rect.center()));
    h.run_steps(90);
    let tree = format!("{:?}", h.kittest_state().root());
    assert!(tree.contains(&limit.to_string()), "{tree}");
    assert!(tree.contains(&parent_size[0].to_string()) && tree.contains(&parent_size[1].to_string()), "{tree}");
    assert!(tree.contains(&bit_count.to_string()), "{tree}");

    let full = h.state().session.cook_full(0);
    let img = full.image(id, 0).unwrap();
    assert_eq!((img.width(), img.height()), (1024, 1024));
}

#[test]
fn full_size_mode_retains_the_resolution_and_format() {
    let (mut h, id) = harness(0);
    let img = h.state().session.cooked.image(id, 0).unwrap();
    assert_eq!((img.width(), img.height()), (1024, 1024));
    let footer = footer(&h);
    assert!(footer.contains(&img.width().to_string()) && footer.contains(&img.height().to_string()), "{footer}");
    assert!(footer.contains(h.state().session.graph().unwrap().format.bits()), "{footer}");
    let rect = h.state().graph.rects.iter().find(|(node, _)| *node == id).unwrap().1;
    h.event(Event::PointerMoved(rect.center()));
    h.run_steps(90);
    assert!(!format!("{:?}", h.kittest_state().root()).contains("Preview"));
}
