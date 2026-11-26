//! Copy, Cut and Paste of graph nodes through the platform's clipboard events and the menus

use egui::{Event, Modifiers, OutputCommand, vec2};
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, Node};
use materialcraft_ui_egui::{MaterialCraftApp, UiState};
use serde_json::json;

/// Two wired nodes, both selected, in the whole window with the runtime installed
fn harness() -> Harness<'static, MaterialCraftApp> {
    let mut app = MaterialCraftApp::headless(materialcraft_engine::new_session(), UiState::default());
    app.session.preview_size = Some(32);
    let source = app.run("graph.node.add", json!({"def": "uniform", "at": [0, 0]})).unwrap()["id"].as_u64().unwrap();
    let levels = app.run("graph.node.add", json!({"def": "levels", "connectFrom": {"node": source}})).unwrap()["id"].as_u64().unwrap();
    app.run("graph.select", json!({"ids": [source, levels]})).unwrap();

    let mut harness = Harness::builder().with_size(vec2(1600.0, 960.0)).build_eframe(|cc| {
        MaterialCraftApp::install_runtime(&cc.egui_ctx, None).expect("runtime");
        app
    });
    harness.run_steps(8);
    harness
}

/// The text the last frame offered to the system clipboard
fn copied_text(harness: &Harness<'static, MaterialCraftApp>) -> String {
    harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|command| if let OutputCommand::CopyText(text) = command { Some(text.clone()) } else { None })
        .expect("a node copy fills the system clipboard, so the desktop's Paste key sends an event")
}

/// Steps a few frames and returns the text one of them offered: a menu pick fires in its frame and runs after it.
fn copied_within_frames(harness: &mut Harness<'static, MaterialCraftApp>) -> String {
    for _ in 0..4 {
        harness.step();
        let offered = harness.output().platform_output.commands.iter().find_map(|command| match command {
            OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        });
        if let Some(text) = offered {
            return text;
        }
    }
    String::new()
}

/// The menu row whose name starts with `label`; the name may go on with the row's shortcut text
fn row<'h>(harness: &'h Harness<'static, MaterialCraftApp>, label: &str) -> Node<'h> {
    harness
        .query_all_by_label_contains(label)
        .find(|node| node.accesskit_node().label().is_some_and(|name| name.starts_with(label)))
        .unwrap_or_else(|| panic!("no row named {label}"))
}

fn node_count(harness: &Harness<'static, MaterialCraftApp>) -> usize {
    harness.state().session.graph().unwrap().nodes.len()
}

fn wire_count(harness: &Harness<'static, MaterialCraftApp>) -> usize {
    harness.state().session.graph().unwrap().connections.len()
}

#[test]
fn desktop_clipboard_events_copy_paste_and_cut_nodes_with_their_wires() {
    let mut harness = harness();
    harness.event(Event::Copy);
    harness.step();
    assert!(harness.state().session.clipboard.is_some(), "the desktop's Copy event copies the nodes");
    let payload = copied_text(&harness);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&payload).unwrap()["nodes"].as_array().unwrap().len(), 2);

    harness.event(Event::Paste(payload));
    harness.step();
    assert_eq!((node_count(&harness), wire_count(&harness)), (4, 2));

    harness.event(Event::Cut);
    harness.step();
    assert_eq!(node_count(&harness), 2);

    harness.event(Event::Paste(copied_text(&harness)));
    harness.step();
    assert_eq!((node_count(&harness), wire_count(&harness)), (4, 2));

    harness.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(node_count(&harness), 2);
    harness.state_mut().run("edit.redo", json!({})).unwrap();
    assert_eq!(node_count(&harness), 4);
}

#[test]
fn edit_menu_copy_fills_the_system_clipboard() {
    let mut harness = harness();

    harness.get_by_label("Edit").click();
    harness.run_steps(4);
    row(&harness, "Copy").click();

    assert!(!copied_within_frames(&mut harness).is_empty());
}

#[test]
fn node_context_menu_copy_fills_the_system_clipboard() {
    let mut harness = harness();
    let node = harness.state().graph.rects[0].1.center();

    harness.event(Event::PointerMoved(node));
    harness.step();
    for pressed in [true, false] {
        harness.event(Event::PointerButton { pos: node, button: egui::PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        harness.step();
    }
    harness.run_steps(4);
    row(&harness, "Copy selection").click();

    assert!(!copied_within_frames(&mut harness).is_empty());
}
