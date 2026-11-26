//! Edit ▸ Preferences: edits stay in the draft until Apply or OK, and the node creation shortcuts they set

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};

fn app() -> MaterialCraftApp {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::sample_session(), UiState::default());
    app.ui.windows.home = false;
    app
}

/// The window alone over the installed runtime; it is drawn while `ui.windows.preferences` is set, as the app's
/// window dispatcher does.
fn window_harness(app: MaterialCraftApp) -> Harness<'static, MaterialCraftApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(1600.0, 960.0))
        .with_pixels_per_point(1.0)
        .with_context_setup(|ctx| {
            MaterialCraftApp::install_runtime(ctx, None).expect("runtime");
        })
        .build_ui_state(
            |ui, app: &mut MaterialCraftApp| {
                if app.ui.windows.preferences {
                    materialcraft_ui_egui::preferences::show(app, ui);
                }
            },
            app,
        );
    harness.run_steps(4);
    harness
}

/// The whole app, for a test whose keys run app commands.
fn app_harness(app: MaterialCraftApp) -> Harness<'static, MaterialCraftApp> {
    let mut harness = Harness::builder().with_size(vec2(1600.0, 960.0)).with_pixels_per_point(1.0).build_eframe(|cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app
    });
    harness.run_steps(6);
    harness
}

fn open(harness: &mut Harness<'static, MaterialCraftApp>) {
    harness.state_mut().ui.windows.preferences = true;
    harness.run_steps(4);
}

fn click(harness: &mut Harness<'static, MaterialCraftApp>, role: Role, label: &str) {
    harness.get(By::new().role(role).label(label)).click();
    harness.run_steps(4);
}

/// Types `text` over the number field named `label` and commits it with Enter.
fn type_number(harness: &mut Harness<'static, MaterialCraftApp>, label: &str, text: &str) {
    harness.get(By::new().role(Role::SpinButton).label(label)).click();
    harness.step();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.event(Event::Text(text.into()));
    harness.key_press(Key::Enter);
    harness.run_steps(4);
}

/// Picks `option` on the choice row named `row`, a segment or an entry of its drop-down.
fn choose(harness: &mut Harness<'static, MaterialCraftApp>, row: &str, option: &str) {
    if harness.query(By::new().role(Role::Button).label(option)).is_none() {
        click(harness, Role::ComboBox, row);
    }
    harness.get_by_label(option).click();
    harness.run_steps(4);
}

#[test]
fn draft_limits_preserve_recents_and_undo_until_applied() {
    let mut harness = window_harness(app());
    for _ in 0..3 {
        harness.state_mut().run("graph.node.add", serde_json::json!({"def": "uniform"})).unwrap();
    }
    for path in ["first.sbs", "second.sbs", "third.sbs"] {
        harness.state_mut().ui.add_recent(path);
    }
    let history = harness.state().session.history();
    let undo_limit = harness.state().session.history_prefs.undo_limit;
    open(&mut harness);
    type_number(&mut harness, "Undo history (steps)", "1");
    type_number(&mut harness, "Recent packages list contains", "1");
    assert_eq!(harness.state().session.history(), history, "an edit reached the session before Apply");
    assert_eq!(harness.state().ui.recent.len(), 3, "an edit trimmed the recent list before Apply");

    click(&mut harness, Role::Button, "Cancel");
    assert_eq!(harness.state().session.history(), history);
    assert_eq!(harness.state().session.history_prefs.undo_limit, undo_limit);
    assert_eq!(harness.state().ui.recent.len(), 3);

    open(&mut harness);
    type_number(&mut harness, "Undo history (steps)", "1");
    type_number(&mut harness, "Recent packages list contains", "1");
    click(&mut harness, Role::Button, "Apply");
    assert_eq!(harness.state().session.history_prefs.undo_limit, 1);
    assert_eq!(harness.state().ui.recent, ["third.sbs"]);
}

#[test]
fn draft_preview_size_reaches_the_session_only_on_apply_or_ok() {
    let mut harness = window_harness(app());
    let initial = harness.state().session.preview_size;
    assert_ne!(initial, Some(512), "the trial picks a size the session does not have");
    open(&mut harness);
    choose(&mut harness, "Preview size while editing", "512 px");
    assert_eq!(harness.state().session.preview_size, initial, "the pick reached the session before Apply");
    click(&mut harness, Role::Button, "Cancel");
    assert_eq!(harness.state().session.preview_size, initial);

    open(&mut harness);
    choose(&mut harness, "Preview size while editing", "512 px");
    click(&mut harness, Role::Button, "Apply");
    assert_eq!(harness.state().session.preview_size, Some(512));

    choose(&mut harness, "Preview size while editing", "Full size");
    assert_eq!(harness.state().session.preview_size, Some(512), "a second pick reached the session before OK");
    click(&mut harness, Role::Button, "OK");
    assert_eq!(harness.state().session.preview_size, None);
}

/// The applied node creation shortcuts, as (command key, binding texts).
fn node_overrides(harness: &Harness<'static, MaterialCraftApp>) -> Vec<(String, Vec<String>)> {
    harness.state().ui.shortcut_overrides.iter().filter(|(key, _)| key.starts_with("node.add.")).map(|(key, texts)| (key.to_owned(), texts.to_vec())).collect()
}

#[test]
fn a_shortcut_is_captured_refused_when_taken_and_adds_its_node_over_the_graph() {
    let mut harness = app_harness(app());
    harness.state_mut().ui.preferences_page = "shortcuts".into();
    open(&mut harness);

    click(&mut harness, Role::Button, "Shortcut for Blur");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::S);
    harness.run_steps(3);
    assert!(harness.state().toasts.iter().any(|toast| toast.error), "a chord an app command holds is refused with an error toast");

    click(&mut harness, Role::Button, "Shortcut for Blur");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    harness.run_steps(3);
    assert!(node_overrides(&harness).is_empty(), "the shortcut stays in the draft until OK");

    // The same keys on another entry move the shortcut there; the search brings it into view
    harness.get(By::new().label("Search")).click();
    harness.step();
    harness.event(Event::Text("levels".into()));
    harness.run_steps(3);
    click(&mut harness, Role::Button, "Shortcut for Levels");
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    harness.run_steps(3);
    click(&mut harness, Role::Button, "OK");

    let applied = node_overrides(&harness);
    assert_eq!(applied.len(), 1, "the chord moved, so one node shortcut remains, not one per entry: {applied:?}");
    assert!(applied[0].0.ends_with("levels"), "the chord belongs to Levels: {applied:?}");
    assert!(!applied[0].1.is_empty());

    let nodes = harness.state().session.graph().unwrap().nodes.len();
    let blank = harness.state().graph.view.left_bottom() + vec2(12.0, -12.0);
    harness.hover_at(blank);
    for pressed in [true, false] {
        harness.event(Event::PointerButton { pos: blank, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        harness.step();
    }
    harness.run_steps(2);
    harness.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::B);
    harness.run_steps(3);
    let graph = harness.state().session.graph().unwrap();
    assert_eq!(graph.nodes.len(), nodes + 1, "the keys over the focused graph add a node");
    assert_eq!(graph.nodes.last().unwrap().def, "levels");
}
