use materialcraft_automation::headless::{Filesystem, Headless, RenderOptions};
use materialcraft_engine::new_session;
use serde_json::json;

#[test]
fn headless_graph_previews_use_the_selected_display_encoding() {
    let mut headless = Headless::new(new_session(), Filesystem::Denied);
    headless.session.preview_size = Some(16);
    let source = headless.command_run("graph.node.add", json!({"def": "uniform", "params": {"color": [0.2, 0.4, 0.8, 1.0]}})).unwrap();
    headless.command_run("view.2d", json!({"node": source["id"]})).unwrap();
    let options = RenderOptions { view: Some("2d".into()), width: Some(16), height: Some(16), ..Default::default() };
    for (srgb, expected) in [(true, [51, 102, 204, 255]), (false, [124, 170, 231, 255])] {
        headless.command_run("view.2d", json!({"srgb": srgb})).unwrap();
        let png = headless.render_png(&options).unwrap();
        let image = image::load_from_memory(&png.png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (16, 16));
        assert!(image.pixels().all(|pixel| pixel.0 == expected), "{srgb}: {:?}", image.get_pixel(0, 0));
    }
}

#[test]
fn rejected_view_command_preserves_the_rendered_scene() {
    let mut h = Headless::new(new_session(), Filesystem::Denied);
    let options = RenderOptions { width: Some(64), height: Some(48), ..RenderOptions::default() };
    h.command_run("view.3d", json!({"scene": "sphere", "environment": "sunset"})).unwrap();
    let before = h.render_png(&options).unwrap().png;
    assert!(h.command_run("view.3d", json!({"scene": "cube", "environment": "invalid"})).is_err());
    assert!(before == h.render_png(&options).unwrap().png, "rejected settings changed the rendered scene");
    // Confirm that valid settings still change the rendered output rather than returning a stale image.
    h.command_run("view.3d", json!({"scene": "cube", "environment": "night"})).unwrap();
    assert_ne!(before, h.render_png(&options).unwrap().png);
}
