//! The graph canvas as a Tab stop: it keeps the focus Tab gives it, a press and an opening graph give it the focus, Tab
//! over it opens the node search, and its keys work as with nothing focused

use egui::accesskit::Role;
use egui::{Event, Id, Key, Modifiers, PointerButton, vec2};
use egui_kittest::{
    Harness,
    kittest::{By, Queryable},
};
use materialcraft_ui_egui::framework::{canvas_id, install_headless};
use materialcraft_ui_egui::{MaterialCraftApp, UiState, menus};

type AppHarness = Harness<'static, MaterialCraftApp>;

fn harness(session: materialcraft_engine::Session) -> AppHarness {
    let mut h = Harness::builder().with_context_setup(install_headless).with_size(vec2(1600.0, 960.0)).build_ui_state(
        |ui, app: &mut MaterialCraftApp| {
            app.step(&ui.ctx().clone());
            app.draw(ui);
        },
        MaterialCraftApp::headless(session, UiState::default()),
    );
    h.run_steps(8);
    h
}

fn focused(h: &AppHarness) -> Option<Id> {
    h.ctx.memory(|memory| memory.focused())
}

fn press(h: &mut AppHarness, modifiers: Modifiers, key: Key) -> Option<Id> {
    h.key_press_modifiers(modifiers, key);
    h.run_steps(3);
    focused(h)
}

fn search_open(h: &AppHarness) -> bool {
    h.query_by_label("Search nodes").is_some()
}

fn hover_canvas(h: &mut AppHarness) {
    h.hover_at(h.state().graph.view.center());
    h.run_steps(2);
}

#[test]
fn tab_walks_over_the_canvas_to_the_stops_after_it_and_back() {
    let mut h = harness(materialcraft_engine::new_session());
    let canvas = Some(canvas_id());
    assert_eq!(focused(&h), canvas, "an opened graph has the keyboard");

    let after = press(&mut h, Modifiers::NONE, Key::Tab);
    assert!(after.is_some() && after != canvas, "Tab moves on from the canvas");
    assert_eq!(press(&mut h, Modifiers::SHIFT, Key::Tab), canvas);
    let before = press(&mut h, Modifiers::SHIFT, Key::Tab);
    assert!(before.is_some() && before != canvas && before != after, "Shift+Tab leaves it backward");
    assert_eq!(press(&mut h, Modifiers::NONE, Key::Tab), canvas, "Tab from the stop before it stops at it");
    assert_eq!(press(&mut h, Modifiers::NONE, Key::Tab), after, "and reaches the stop after it");
    assert!(!search_open(&h));
}

#[test]
fn tab_with_the_pointer_over_the_canvas_opens_the_search_and_shift_tab_leaves_it() {
    let mut h = harness(materialcraft_engine::new_session());
    let canvas = Some(canvas_id());
    hover_canvas(&mut h);
    let left = press(&mut h, Modifiers::SHIFT, Key::Tab);
    assert!(left.is_some() && left != canvas, "Shift+Tab leaves the canvas");
    assert!(!search_open(&h));
    h.ctx.memory_mut(|memory| memory.surrender_focus(left.unwrap()));
    h.run_steps(2);
    assert_eq!(focused(&h), canvas, "with nothing focused, the canvas under the pointer takes the keyboard");

    press(&mut h, Modifiers::NONE, Key::Tab);
    assert!(search_open(&h), "Tab opens the search");
    assert_eq!(press(&mut h, Modifiers::NONE, Key::Escape), canvas, "closing it gives the keyboard back");
    assert!(!search_open(&h));
}

#[test]
fn escape_releases_the_keyboard_from_the_dock_controls_after_the_canvas() {
    let mut h = harness(materialcraft_engine::new_session());
    // The 3D view's tab row and its drag band
    for stop in 1..=2 {
        h.ctx.memory_mut(|memory| memory.request_focus(canvas_id()));
        h.run_steps(2);
        for _ in 0..stop {
            press(&mut h, Modifiers::NONE, Key::Tab);
        }
        assert!(focused(&h).is_some_and(|id| id != canvas_id()), "Tab reaches stop {stop} after the canvas");
        assert_eq!(press(&mut h, Modifiers::NONE, Key::Escape), None, "Escape releases stop {stop}");
    }
}

#[test]
fn a_press_in_the_canvas_takes_the_keyboard_from_a_toolbar_button() {
    let mut h = harness(materialcraft_engine::new_session());
    h.get_by_label("Open package... (Ctrl+O)").focus();
    h.run_steps(2);
    press(&mut h, Modifiers::SHIFT, Key::Tab);
    assert!(h.get_by_label("New graph... (Ctrl+N)").is_focused());

    hover_canvas(&mut h);
    assert!(h.get_by_label("New graph... (Ctrl+N)").is_focused(), "the pointer alone takes no focus from a widget");
    let pos = h.state().graph.view.center();
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
        h.run_steps(2);
        assert_eq!(focused(&h), Some(canvas_id()));
    }
    press(&mut h, Modifiers::NONE, Key::Space);
    assert!(search_open(&h), "Space goes to the canvas");
    assert!(!h.state().ui.windows.new_graph, "and not to the button");
}

#[test]
fn enter_in_the_canvas_menu_adds_the_node_its_letter_filters() {
    let mut h = harness(materialcraft_engine::new_session());
    hover_canvas(&mut h);
    let pos = h.state().graph.view.center();
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(3);
    h.get_by_label("Add node").hover();
    h.run_steps(3);
    assert_eq!(focused(&h), Some(canvas_id()));

    press(&mut h, Modifiers::NONE, Key::L);
    press(&mut h, Modifiers::NONE, Key::Enter);
    assert_eq!(h.state().session.graph().unwrap().nodes.len(), 1);
    assert!(h.query_by_label("Add node").is_none(), "and the menu closes");
}

#[test]
fn a_graph_created_by_keyboard_in_a_dialog_gets_the_focus() {
    let mut h = harness(materialcraft_engine::sample_session());
    let ctx = h.ctx.clone();
    menus::invoke(h.state_mut(), &ctx, "ui.newFunctionGraph");
    h.run_steps(4);
    h.state_mut().ui.new_graph_package = None;
    h.get(By::new().role(Role::Button).label("Create")).focus();
    h.run_steps(2);
    assert!(h.get(By::new().role(Role::Button).label("Create")).is_focused());

    press(&mut h, Modifiers::NONE, Key::Enter);
    h.run_steps(4);
    assert!(h.state().session.graph().unwrap().function.is_some());
    assert_eq!(focused(&h), Some(canvas_id()));
}

#[test]
fn shortcuts_run_with_the_canvas_focused() {
    let mut h = harness(materialcraft_engine::sample_session());
    assert_eq!(focused(&h), Some(canvas_id()));
    let nodes = |h: &AppHarness| h.state().session.graph().unwrap().nodes.len();
    let before = nodes(&h);

    press(&mut h, Modifiers::COMMAND, Key::A);
    assert_eq!(h.state().session.selection.len(), before, "Ctrl+A selects every node");
    press(&mut h, Modifiers::NONE, Key::Delete);
    assert_eq!(nodes(&h), 0, "Delete deletes them");
    press(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(nodes(&h), before, "Ctrl+Z brings them back");
    assert_eq!(focused(&h), Some(canvas_id()));
}
