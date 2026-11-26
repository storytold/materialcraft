//! The 3D view's menus, tool strip, Properties panel, keys and pointer input change the view.
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use materialcraft_engine::View3d;
use materialcraft_ui_egui::view3d::Tool;
use materialcraft_ui_egui::{MaterialCraftApp, UiState, properties, view3d};
use serde_json::{Value, json};

/// The 3D view above the Properties panel, as one small window
fn harness() -> Harness<'static, MaterialCraftApp> {
    harness_with_width(900.0)
}

fn harness_with_width(width: f32) -> Harness<'static, MaterialCraftApp> {
    let app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    let mut harness = Harness::builder().with_context_setup(materialcraft_ui_egui::theme::install).with_size(vec2(width, 900.0)).build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            let whole = ui.available_rect_before_wrap();
            let (view, panel) = whole.split_top_bottom_at_y(whole.top() + 420.0);
            view3d::show(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(view)));
            properties::show(app, &mut ui.new_child(egui::UiBuilder::new().max_rect(panel)));
        },
        app,
    );
    harness.run_steps(6);
    harness
}

#[test]
fn frame_key_fits_a_portrait_viewport() {
    let mut harness = harness_with_width(300.0);
    let area = picture(&harness);
    assert!(area.width() < area.height());

    harness.hover_at(area.center());
    harness.key_press(Key::F);
    harness.run_steps(2);

    assert!(view(&harness).distance > view(&harness).framing_distance());
}

fn view<'harness>(harness: &'harness Harness<'static, MaterialCraftApp>) -> &'harness View3d {
    &harness.state().session.view3d
}

fn setting(harness: &Harness<'static, MaterialCraftApp>, name: &str) -> Value {
    serde_json::to_value(view(harness)).unwrap()[name].clone()
}

fn click(harness: &mut Harness<'static, MaterialCraftApp>, role: Role, label: &str) {
    harness.get_by_role_and_label(role, label).click();
    harness.run_steps(4);
}

/// Click a chip of the Properties panel's header, which is drawn after the menu of the same name
fn show_part(harness: &mut Harness<'static, MaterialCraftApp>, label: &str) {
    let chip = harness.query_all_by_role_and_label(Role::Button, label).last().unwrap();
    chip.click();
    harness.run_steps(4);
}

/// Open a menu of the view's menu bar and click through to an item
fn choose(harness: &mut Harness<'static, MaterialCraftApp>, path: &[&str]) {
    for label in path {
        click(harness, Role::Button, label);
    }
}

fn picture(harness: &Harness<'static, MaterialCraftApp>) -> Rect {
    harness.get_by_role_and_label(Role::Image, "3D view").rect()
}

fn press(harness: &mut Harness<'static, MaterialCraftApp>, button: PointerButton, position: Pos2, pressed: bool) {
    harness.event(Event::PointerButton { pos: position, button, pressed, modifiers: Modifiers::NONE });
    harness.run_steps(2);
}

/// Press `button` at `from`, move to `to` in steps and release there, with `modifiers` held
fn drag(harness: &mut Harness<'static, MaterialCraftApp>, button: PointerButton, modifiers: Modifiers, from: Pos2, to: Pos2) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.hover_at(from);
    harness.run_steps(2);
    press(harness, button, from, true);
    for step in 1..=4 {
        harness.hover_at(from + (to - from) * (step as f32 / 4.0));
        harness.run_steps(2);
    }
    press(harness, button, to, false);
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.run_steps(2);
}

#[test]
fn the_tool_strip_switches_the_settings_and_tools_it_names() {
    let mut harness = harness();

    for (label, name) in [
        ("Grid", "showGrid"),
        ("Scene stats", "sceneStats"),
        ("Axis", "showAxis"),
        ("Bounding box", "boundingBox"),
        ("Vertex tangent space", "vertexTangentSpace"),
        ("Wireframe", "wireframe"),
        ("Light", "lightHelpers"),
        ("The point lights cast shadows", "shadows"),
    ] {
        click(&mut harness, Role::Button, label);
        assert_eq!(setting(&harness, name), true, "{label}");
    }

    // Backface culling starts on
    click(&mut harness, Role::Button, "Backface culling");
    assert_eq!(setting(&harness, "backfaceCulling"), false);

    click(&mut harness, Role::Button, "Show the environment behind the mesh");
    assert_eq!(setting(&harness, "background"), "environment");
    click(&mut harness, Role::Button, "Show the environment behind the mesh");
    assert_eq!(setting(&harness, "background"), "studio", "off, the studio backdrop of a new view is back");

    assert_eq!(harness.state().view3d.tool, Tool::Camera);
    click(&mut harness, Role::Button, "Light: drag to move the first point light, which switches it on (L)");
    assert_eq!(harness.state().view3d.tool, Tool::Light);
    click(&mut harness, Role::Button, "Select: click the mesh (Shift+click with any tool)");
    assert_eq!(harness.state().view3d.tool, Tool::Select);

    click(&mut harness, Role::Button, "Scene browser");
    assert!(harness.state().view3d.scene_browser);
    assert!(harness.query_by_label_contains("Point Light 2").is_some(), "the browser lists the scene");

    // The display buttons fold away
    click(&mut harness, Role::Button, "Display settings");
    assert!(harness.query_by_role_and_label(Role::Button, "Grid").is_none());
}

#[test]
fn the_menus_run_their_commands() {
    let mut harness = harness();

    choose(&mut harness, &["Scene", "Sphere"]);
    assert_eq!(view(&harness).scene, "sphere");

    choose(&mut harness, &["Materials", "Default", "Lambert"]);
    assert_eq!(setting(&harness, "shader"), "lambert");
    choose(&mut harness, &["Materials", "Default", "Metallic Roughness", "Parallax Occlusion"]);
    assert_eq!((setting(&harness, "shader"), setting(&harness, "technique")), (json!("metallicRoughness"), json!("parallax")));
    choose(&mut harness, &["Materials", "Default", "Mesh Info", "UV"]);
    assert_eq!((setting(&harness, "shader"), setting(&harness, "meshInfo")), (json!("meshInfo"), json!("uv")));
    choose(&mut harness, &["Materials", "Default", "Reset Material"]);
    assert_eq!(setting(&harness, "shader"), "openPbr");

    choose(&mut harness, &["Camera", "Viewpoint", "Top"]);
    assert_eq!((view(&harness).yaw, view(&harness).pitch), (0.0, 89.0));
    harness.state_mut().session.view3d.target = [0.5, 0.0, 0.0];
    choose(&mut harness, &["Camera", "Frame F"]);
    assert_eq!(view(&harness).target, [0.0; 3]);
    assert_eq!(view(&harness).distance, view(&harness).framing_distance());
    choose(&mut harness, &["Camera", "Reset Position R"]);
    assert_eq!((view(&harness).yaw, view(&harness).pitch), (35.0, 25.0));

    harness.state_mut().run("view.3d", json!({"light1": {"enabled": true}, "exposure": 2.0})).unwrap();
    choose(&mut harness, &["Lights", "Reset Lights"]);
    assert!(!view(&harness).lights[0].enabled);
    choose(&mut harness, &["Environment", "Reset Environment"]);
    assert_eq!(view(&harness).exposure, 0.0);

    click(&mut harness, Role::Button, "Display");
    click(&mut harness, Role::CheckBox, "Resolution");
    click(&mut harness, Role::CheckBox, "FPS");
    assert_eq!((setting(&harness, "showResolution"), setting(&harness, "fps")), (json!(true), json!(true)));
    harness.key_press(Key::Escape);
    harness.run_steps(4);

    // The default is what a reset goes back to, and what the preferences keep for the next start
    harness.state_mut().run("view.3d", json!({"scene": "torus", "grid": true})).unwrap();
    choose(&mut harness, &["Scene", "Save Current State As Default"]);
    harness.state_mut().run("view.3d", json!({"scene": "cube", "grid": false})).unwrap();
    choose(&mut harness, &["Scene", "Reset Scene"]);
    assert_eq!((view(&harness).scene.as_str(), view(&harness).show_grid), ("torus", true));
    assert_eq!(harness.state().ui.prefs.default_view3d.as_ref().map(|saved| saved.scene.as_str()), Some("torus"));
}

#[test]
fn the_properties_panel_edits_the_part_the_view_asks_for() {
    let mut harness = harness();
    assert_eq!(harness.query_all_by_label("Focal Length (mm)").count(), 0, "the panel starts on the graph");

    choose(&mut harness, &["Camera", "Edit properties"]);
    assert!(harness.query_all_by_label("Focal Length (mm)").count() > 0);
    click(&mut harness, Role::CheckBox, "Override Render Resolution");
    click(&mut harness, Role::CheckBox, "Automatic Clip Planes");
    assert_eq!((setting(&harness, "overrideResolution"), setting(&harness, "autoClip")), (json!(true), json!(false)));

    // The chips at the top switch between the parts of the view
    show_part(&mut harness, "Lights");
    let enabled: Vec<_> = harness.query_all_by_role_and_label(Role::CheckBox, "Enabled").collect();
    assert_eq!(enabled.len(), 3, "the ambient light and the two point lights");
    enabled[1].click();
    harness.run_steps(4);
    assert!(view(&harness).lights[0].enabled);

    show_part(&mut harness, "Environment");
    click(&mut harness, Role::CheckBox, "Enabled");
    assert!(!view(&harness).env_enabled);

    show_part(&mut harness, "Renderer");
    click(&mut harness, Role::CheckBox, "Transparency (alpha)");
    assert!(!view(&harness).transparency);

    show_part(&mut harness, "Material");
    click(&mut harness, Role::CheckBox, "Enable physical size from graph");
    click(&mut harness, Role::CheckBox, "Use Base Weight");
    assert!(view(&harness).physical_size);
    assert!(view(&harness).disabled.contains("baseWeight"));

    // Selecting a node gives the panel back to it
    let node = harness.state().session.graph().unwrap().nodes[0].id;
    harness.state_mut().session.selection = vec![node];
    harness.run_steps(4);
    assert_eq!(harness.query_all_by_label("Enable physical size from graph").count(), 0);
}

#[test]
fn typed_uv_repeats_can_exceed_the_slider_track_limit() {
    let mut harness = harness();
    choose(&mut harness, &["Materials", "Default", "Edit properties"]);

    let input = harness.get_by_role_and_label(Role::TextInput, "Tiling");
    input.click();
    harness.run_steps(2);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.run_steps(2);
    harness.event(Event::Text("32".into()));
    harness.run_steps(4);

    assert_eq!(view(&harness).tiling, 32.0);
}

#[test]
fn dragging_moves_the_camera_the_environment_and_the_light() {
    let mut harness = harness();
    let area = picture(&harness);
    let (from, to) = (area.center(), area.center() + vec2(60.0, 30.0));
    let start = view(&harness).clone();

    drag(&mut harness, PointerButton::Primary, Modifiers::NONE, from, to);
    let orbited = view(&harness).clone();
    assert!(orbited.yaw != start.yaw && orbited.pitch != start.pitch, "the primary button orbits");
    assert_eq!((orbited.target, orbited.distance), (start.target, start.distance));

    drag(&mut harness, PointerButton::Middle, Modifiers::NONE, from, to);
    let panned = view(&harness).clone();
    assert!(panned.target != orbited.target, "the middle button pans");
    assert_eq!((panned.yaw, panned.distance), (orbited.yaw, orbited.distance));

    drag(&mut harness, PointerButton::Secondary, Modifiers::NONE, from, to);
    let zoomed = view(&harness).clone();
    assert!(zoomed.distance > panned.distance, "dragging the secondary button down moves away");
    assert_eq!(zoomed.target, panned.target);

    drag(&mut harness, PointerButton::Secondary, Modifiers::SHIFT, from, to);
    assert!(view(&harness).env_rotation != zoomed.env_rotation, "Shift turns the environment");
    assert_eq!(view(&harness).distance, zoomed.distance);

    // The Light tool moves the first point light and switches it on
    assert!(!view(&harness).lights[0].enabled);
    harness.state_mut().view3d.tool = Tool::Light;
    drag(&mut harness, PointerButton::Primary, Modifiers::NONE, from, to);
    let lit = view(&harness).lights[0];
    assert!(lit.enabled && lit.yaw != start.lights[0].yaw && lit.pitch != start.lights[0].pitch);
    assert_eq!(view(&harness).yaw, orbited.yaw, "the camera stays");

    drag(&mut harness, PointerButton::Secondary, Modifiers::NONE, from, to);
    assert!(view(&harness).lights[0].distance > lit.distance);
}

#[test]
fn clicks_select_the_mesh_and_keys_act_while_the_pointer_is_over_the_view() {
    let mut harness = harness();
    let area = picture(&harness);
    harness.state_mut().view3d.tool = Tool::Select;

    harness.hover_at(area.center());
    harness.run_steps(2);
    press(&mut harness, PointerButton::Primary, area.center(), true);
    press(&mut harness, PointerButton::Primary, area.center(), false);
    assert!(harness.state().view3d.selected, "a click on the mesh selects it");

    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!harness.state().view3d.selected, "Escape clears the selection");

    let corner = area.left_top() + vec2(6.0, 6.0);
    harness.state_mut().view3d.selected = true;
    harness.hover_at(corner);
    harness.run_steps(2);
    press(&mut harness, PointerButton::Primary, corner, true);
    press(&mut harness, PointerButton::Primary, corner, false);
    assert!(!harness.state().view3d.selected, "a click beside the mesh selects nothing");

    harness.hover_at(area.center());
    harness.run_steps(2);
    harness.key_press(Key::L);
    harness.run_steps(2);
    assert_eq!(harness.state().view3d.tool, Tool::Light);
    harness.key_press(Key::C);
    harness.run_steps(2);
    assert_eq!(harness.state().view3d.tool, Tool::Camera);

    harness.state_mut().session.view3d.yaw = 120.0;
    harness.state_mut().session.view3d.target = [0.4, 0.0, 0.0];
    harness.key_press(Key::F);
    harness.run_steps(2);
    assert_eq!((view(&harness).yaw, view(&harness).target), (120.0, [0.0; 3]), "F frames the mesh from where the camera is");
    harness.key_press(Key::R);
    harness.run_steps(2);
    assert_eq!(view(&harness).yaw, 35.0, "R puts the camera back");

    // Shift+R also puts the camera's properties back
    harness.state_mut().run("view.3d", json!({"focalLength": 80.0, "yaw": 200.0})).unwrap();
    harness.key_press_modifiers(Modifiers::SHIFT, Key::R);
    harness.run_steps(2);
    assert_eq!((view(&harness).focal_length, view(&harness).yaw), (35.0, 35.0));

    // One notch of the wheel moves the camera by a fortieth of its distance
    let distance = view(&harness).distance;
    harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, -1.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    });
    harness.run_steps(30);
    let moved = view(&harness).distance / distance;
    assert!((moved - 0.025f32.exp()).abs() < 0.002, "scrolling down moves away by 2.5%: {moved}");

    // Away from the view the keys do nothing
    harness.remove_cursor();
    harness.run_steps(2);
    harness.key_press(Key::L);
    harness.run_steps(2);
    assert_eq!(harness.state().view3d.tool, Tool::Camera);
}
