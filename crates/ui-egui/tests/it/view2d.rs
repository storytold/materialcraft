//! The 2D view's toolbars, panels, keys and pointer input change what the view shows.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use materialcraft_ui_egui::state::ImageCamera;
use materialcraft_ui_egui::{MaterialCraftApp, UiState, view2d};
use serde_json::json;

const GRAPH_SIZE: u32 = 256;

/// An app whose 2D view shows a uniform node, gray or color, with two graph outputs behind it
fn app(color: bool) -> MaterialCraftApp {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.run("file.new", json!({"template": "empty", "size": GRAPH_SIZE})).unwrap();
    let node = add(&mut app, json!({"def": "uniform", "params": {"color_mode": color}}));
    add(&mut app, json!({"def": "output", "params": {"identifier": "basecolor"}, "connectFrom": {"node": node}}));
    add(&mut app, json!({"def": "output", "params": {"identifier": "roughness"}, "connectFrom": {"node": node}}));
    app.run("view.2d", json!({"node": node})).unwrap();
    app.session.cook();
    app
}

fn add(app: &mut MaterialCraftApp, node: serde_json::Value) -> u64 {
    app.run("graph.node.add", node).unwrap()["id"].as_u64().unwrap()
}

fn harness(app: MaterialCraftApp) -> Harness<'static, MaterialCraftApp> {
    let mut harness = Harness::builder()
        .with_context_setup(materialcraft_ui_egui::theme::install)
        .with_size(vec2(900.0, 640.0))
        .build_ui_state(|ui, app: &mut MaterialCraftApp| view2d::show(app, ui), app);
    harness.run();
    harness
}

fn click(harness: &mut Harness<'static, MaterialCraftApp>, label: &str) {
    harness.get_by_role_and_label(egui::accesskit::Role::Button, label).click();
    harness.run();
}

fn image_area(harness: &Harness<'static, MaterialCraftApp>) -> Rect {
    harness.get_by_label("2D view").rect()
}

fn press(harness: &mut Harness<'static, MaterialCraftApp>, button: PointerButton, position: Pos2, pressed: bool) {
    harness.event(Event::PointerButton { pos: position, button, pressed, modifiers: Modifiers::NONE });
    harness.run();
}

/// Press `button` at `from`, move to `to` and release there
fn drag(harness: &mut Harness<'static, MaterialCraftApp>, button: PointerButton, from: Pos2, to: Pos2) {
    harness.hover_at(from);
    harness.run();
    press(harness, button, from, true);
    for step in 1..=4 {
        harness.hover_at(from + (to - from) * (step as f32 / 4.0));
        harness.run();
    }
    press(harness, button, to, false);
}

#[test]
fn the_toolbar_buttons_switch_the_settings_they_name() {
    let mut harness = harness(app(true));

    for (label, setting) in [
        ("UV", "uv"),
        ("Image information", "information"),
        ("Histogram", "histogram"),
        ("Show a checkerboard behind transparent pixels", "checkerboard"),
        ("The colors are already multiplied by their alpha", "premultiplied"),
        ("Show the image in the proportions of the graph's physical size", "physicalSize"),
        ("Repeat the image to check its tiling", "tiled"),
    ] {
        click(&mut harness, label);
        assert_eq!(serde_json::to_value(&harness.state().session.view2d).unwrap()[setting], true, "{label}");
    }

    // The two settings that start on switch off
    for (label, setting) in [
        ("The values are sRGB encoded; off shows them as linear light", "srgb"),
        ("Keep the zoom when the image size changes; off keeps the image's size on screen", "maintainZoom"),
    ] {
        click(&mut harness, label);
        assert_eq!(serde_json::to_value(&harness.state().session.view2d).unwrap()[setting], false, "{label}");
    }
}

#[test]
fn the_channel_menu_and_the_keys_over_the_image_pick_the_channel() {
    let mut harness = harness(app(true));

    click(&mut harness, "Channels shown");
    click(&mut harness, "Red R");
    assert_eq!(harness.state().session.view2d.channel, "r");

    // The keys only act while the pointer is over the image
    harness.remove_cursor();
    harness.run();
    harness.key_press(Key::G);
    harness.run();
    assert_eq!(harness.state().session.view2d.channel, "r");

    harness.hover_at(image_area(&harness).center());
    harness.run();
    for (key, channel) in [(Key::G, "g"), (Key::B, "b"), (Key::A, "a"), (Key::R, "r"), (Key::C, "rgba")] {
        harness.key_press(key);
        harness.run();
        assert_eq!(harness.state().session.view2d.channel, channel);
    }

    harness.key_press(Key::Space);
    harness.run();
    assert!(harness.state().session.view2d.tiled);
}

#[test]
fn the_zoom_buttons_step_the_zoom_and_fit_fills_the_view_exactly() {
    let mut harness = harness(app(false));
    let area = image_area(&harness);
    let fitted = harness.state().ui.image_camera;
    assert!(fitted.fit);
    assert!((fitted.zoom - area.height().min(area.width()) / GRAPH_SIZE as f32).abs() < 1.0e-4, "fit leaves no margin: {fitted:?} in {area:?}");

    click(&mut harness, "Zoom in");
    let closer = harness.state().ui.image_camera;
    assert!(!closer.fit);
    assert!((closer.zoom - fitted.zoom * ImageCamera::STEP).abs() < 1.0e-4, "{closer:?}");

    click(&mut harness, "Zoom out");
    click(&mut harness, "Zoom out");
    assert!((harness.state().ui.image_camera.zoom - fitted.zoom / ImageCamera::STEP).abs() < 1.0e-4);

    click(&mut harness, "1:1");
    assert_eq!(harness.state().ui.image_camera, ImageCamera { zoom: 1.0, offset: [0.0, 0.0], fit: false });

    click(&mut harness, "Fit");
    assert_eq!(harness.state().ui.image_camera.zoom, fitted.zoom);
    assert!(harness.state().ui.image_camera.fit);

    // F over the image fits it as well
    click(&mut harness, "1:1");
    harness.hover_at(area.center());
    harness.run();
    harness.key_press(Key::F);
    harness.run();
    assert!(harness.state().ui.image_camera.fit);
}

#[test]
fn a_wheel_notch_zooms_by_one_step_around_the_pointer() {
    let mut harness = harness(app(false));
    let area = image_area(&harness);
    let before = harness.state().ui.image_camera.zoom;

    harness.hover_at(area.center());
    harness.run();
    harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 1.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    });
    harness.run();

    // The scroll is smoothed over a few frames; together they make one step
    let camera = harness.state().ui.image_camera;
    assert!((camera.zoom - before * ImageCamera::STEP).abs() < before * 0.01, "{before} -> {camera:?}");
    assert_eq!(camera.offset, [0.0, 0.0], "the center stays under a pointer at the center");
}

#[test]
fn dragging_pans_until_the_information_panel_gives_the_primary_button_to_the_sampler() {
    let mut harness = harness(app(false));
    let center = image_area(&harness).center();

    drag(&mut harness, PointerButton::Primary, center, center + vec2(40.0, 20.0));
    assert_eq!(harness.state().ui.image_camera.offset, [40.0, 20.0]);
    assert_eq!(harness.state().session.view2d.sampler, None);

    // With the panel open a click places the sampler under the pointer and the image stays put
    click(&mut harness, "Fit");
    click(&mut harness, "Image information");
    let center = image_area(&harness).center();
    drag(&mut harness, PointerButton::Primary, center, center);
    let sampler = harness.state().session.view2d.sampler.expect("a click places the sampler");
    assert!(sampler.iter().all(|coordinate| (coordinate - 0.5).abs() < 0.01), "{sampler:?}");
    assert!(harness.state().ui.image_camera.fit);

    // The middle button still pans
    drag(&mut harness, PointerButton::Middle, center, center + vec2(-30.0, 10.0));
    assert_eq!(harness.state().ui.image_camera.offset, [-30.0, 10.0]);

    click(&mut harness, "Remove the color sampler");
    assert_eq!(harness.state().session.view2d.sampler, None);
}

#[test]
fn the_zoom_lock_decides_what_a_new_image_size_does() {
    for (maintain_zoom, expected) in [(true, 0.5), (false, 1.0)] {
        let mut harness = harness(app(false));
        harness.state_mut().run("view.2d", json!({"maintainZoom": maintain_zoom})).unwrap();
        harness.state_mut().ui.image_camera = ImageCamera { zoom: 0.5, offset: [0.0, 0.0], fit: false };
        harness.run();

        // Half the preview resolution halves the image
        let app = harness.state_mut();
        app.ui.prefs.preview_size = GRAPH_SIZE / 2;
        app.apply_prefs();
        app.session.cook();
        harness.run();

        let (_, image) = harness.state().session.view2d_image().unwrap();
        assert_eq!(image.w, GRAPH_SIZE / 2);
        assert_eq!(harness.state().ui.image_camera.zoom, expected, "maintain zoom {maintain_zoom}");
    }
}

#[test]
fn select_output_switches_between_the_graph_outputs() {
    let mut harness = harness(app(true));

    // A node with one output has nothing to choose from; the title names the node
    assert!(view2d::title(harness.state()).ends_with(" - 2D View"));
    let outputs: Vec<u64> = {
        let graph = harness.state().session.graph().unwrap();
        graph.nodes.iter().filter(|node| node.def == "output").map(|node| node.id.0).collect()
    };
    harness.state_mut().run("view.2d", json!({"node": outputs[0]})).unwrap();
    harness.state_mut().session.cook();
    harness.run();
    assert_eq!(view2d::title(harness.state()), "Basecolor - 2D View");

    click(&mut harness, "Select output");
    click(&mut harness, "Roughness (roughness)");

    assert_eq!(harness.state().session.view2d.node.map(|node| node.0), Some(outputs[1]));
    assert_eq!(view2d::title(harness.state()), "Roughness - 2D View");
}

#[test]
fn the_histogram_panel_selects_a_range_and_changes_how_it_counts() {
    let mut harness = harness(app(true));
    click(&mut harness, "Histogram");

    let plot = harness.get_by_label("Histogram plot").rect();
    drag(&mut harness, PointerButton::Primary, plot.left_center() + vec2(plot.width() * 0.25, 0.0), plot.left_center() + vec2(plot.width() * 0.75, 0.0));
    let selection = harness.state().session.view2d.histogram_selection.expect("a drag selects a range");
    assert!((selection[0] - 0.25).abs() < 0.02 && (selection[1] - 0.75).abs() < 0.02, "{selection:?}");

    click(&mut harness, "Sqrt");
    assert!(harness.state().session.view2d.histogram_sqrt);

    harness.get_by_label("Histogram channel").click();
    harness.run();
    harness.get_by_label("Alpha").click();
    harness.run();
    assert_eq!(serde_json::to_value(&harness.state().session.view2d).unwrap()["histogramChannel"], "alpha");

    harness.get_by_label("Histogram bit depth").click();
    harness.run();
    harness.get_by_label("8 bits").click();
    harness.run();
    assert_eq!(harness.state().session.view2d.histogram_bits, 8);

    // A click on the plot clears the selection, and the panel's own button closes it
    harness.get_by_label("Histogram plot").click();
    harness.run();
    assert_eq!(harness.state().session.view2d.histogram_selection, None);
    click(&mut harness, "Close the Histogram panel");
    assert!(!harness.state().session.view2d.histogram);
}

#[test]
fn a_gray_image_only_offers_its_luminance() {
    let mut harness = harness(app(false));
    harness.state_mut().run("view.2d", json!({"histogram": true, "histogramChannel": "red"})).unwrap();
    harness.run();

    harness.get_by_label("Histogram channel").click();
    harness.run();

    assert!(harness.query_by_label("Red").is_none());
    assert!(harness.query_all_by_label("Luminance").count() >= 1);
}
