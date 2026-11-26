use egui::vec2;
use materialcraft_engine::core::NodeId;
use serde_json::json;

use super::frames::Frames;
use crate::MaterialCraftApp;

/// Three primitive nodes framed on screen
fn three_nodes() -> (Frames, [NodeId; 3]) {
    let mut s = materialcraft_engine::sample_session();
    materialcraft_engine::execute(&mut s, "file.new", &json!({"template": "empty", "size": 256})).unwrap();
    let ids = ["uniform", "uniform", "channels_shuffle"].iter().enumerate().map(|(i, def)| {
        let added = materialcraft_engine::execute(&mut s, "graph.node.add", &json!({"def": def, "at": [i as f32 * 300.0, 0.0]})).unwrap();
        NodeId(added["id"].as_u64().unwrap())
    });
    let ids: [NodeId; 3] = ids.collect::<Vec<_>>().try_into().unwrap();
    s.selection.clear();
    (Frames::new(s), ids)
}

#[test]
fn clicking_a_node_shows_it_in_the_2d_view() {
    let (mut f, [noise, checker, split]) = three_nodes();
    // Shown explicitly first (a double-click, an output port or "View in 2D view")
    f.app.run("view.2d", json!({"node": noise.0}));
    assert_eq!(f.shown(), Some(noise));
    let at = f.center(checker);
    f.click(at, egui::Modifiers::NONE);
    assert_eq!(f.app.session.selection, vec![checker]);
    assert_eq!(f.app.session.view2d.node, Some(checker));
    assert_eq!(f.shown(), Some(checker));
    // And back
    let at = f.center(noise);
    f.click(at, egui::Modifiers::NONE);
    assert_eq!(f.app.session.view2d.node, Some(noise));
    assert_eq!(f.shown(), Some(noise));
    // Ctrl+click adds a node to the selection and shows it too
    let at = f.center(checker);
    f.click(at, egui::Modifiers::CTRL);
    assert_eq!(f.app.session.selection, vec![noise, checker]);
    assert_eq!(f.shown(), Some(checker));
    f.app.session.selection = vec![noise, checker];
    f.app.run("view.2d", json!({"node": checker.0}));
    f.click(f.center(noise), egui::Modifiers::CTRL);
    assert_eq!(f.app.session.selection, vec![checker]);
    assert_eq!(f.shown(), Some(checker));

    f.app.run("view.2d", json!({"node": noise.0}));
    let from = f.center(checker);
    let before = f.app.session.graph().unwrap().node(checker).unwrap().pos;
    f.run(vec![egui::Event::PointerMoved(from)], egui::Modifiers::NONE);
    f.run(
        vec![egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE }],
        egui::Modifiers::NONE,
    );
    for step in 1..=8 {
        f.run(vec![egui::Event::PointerMoved(from + vec2(5.0 * step as f32, 0.0))], egui::Modifiers::NONE);
    }
    f.run(
        vec![egui::Event::PointerButton {
            pos: from + vec2(40.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    let after = f.app.session.graph().unwrap().node(checker).unwrap().pos;
    assert_ne!(after, before);
    assert_eq!(f.shown(), Some(noise));

    f.app.run("view.2d", json!({"node": split.0, "port": 2}));
    f.click(f.center(split), egui::Modifiers::NONE);
    assert_eq!((f.app.session.view2d.node, f.app.session.view2d.port), (Some(split), 2));
}

#[test]
fn clicked_wires_swap_with_x_and_delete_together() {
    let mut frames = Frames::new(materialcraft_engine::sample_session());
    frames.app.run("file.new", json!({"template": "empty"}));
    let id = |v: Option<serde_json::Value>| v.and_then(|v| v["id"].as_u64()).unwrap();
    let a = id(frames.app.run("graph.node.add", json!({"def": "uniform", "at": [0, -150]})));
    let b = id(frames.app.run("graph.node.add", json!({"def": "uniform", "at": [0, 150]})));
    let blend = id(frames.app.run("graph.node.add", json!({"def": "blend", "at": [450, 0]})));
    frames.app.run("graph.connect", json!({"from": a, "to": blend, "toPort": "foreground"}));
    frames.app.run("graph.connect", json!({"from": b, "to": blend, "toPort": "background"}));
    for _ in 0..3 {
        frames.run(Vec::new(), egui::Modifiers::NONE);
    }
    let mid = |frames: &Frames, port: &str| {
        frames.app.graph.wires.iter().find(|(w, _)| w.0.0 == blend && w.1 == port).map(|(_, c)| Frames::wire_midpoint(c)).unwrap()
    };
    frames.click(mid(&frames, "foreground"), egui::Modifiers::NONE);
    frames.click(mid(&frames, "background"), egui::Modifiers::SHIFT);
    assert_eq!(frames.app.graph.selected_wire_refs().len(), 2);
    let key = |k| egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
    let src = |app: &MaterialCraftApp, port: &str| app.session.graph().unwrap().input_of(NodeId(blend), port).unwrap().from.0;
    frames.run(vec![key(egui::Key::X)], egui::Modifiers::NONE);
    assert_eq!((src(&frames.app, "foreground"), src(&frames.app, "background")), (b, a));
    // Still selected: X again swaps back
    frames.run(vec![key(egui::Key::X)], egui::Modifiers::NONE);
    assert_eq!((src(&frames.app, "foreground"), src(&frames.app, "background")), (a, b));

    frames.app.run("graph.connect", json!({"from": a, "to": blend, "toPort": "background"})).unwrap();
    let package = frames.app.session.pkg.clone();
    let history = frames.app.session.history();
    assert!(!crate::menus::enabled(&frames.app, "graph.wire.swap"));
    frames.run(vec![key(egui::Key::X)], egui::Modifiers::NONE);
    crate::menus::invoke(&mut frames.app, &frames.ctx, "graph.wire.swap");
    assert_eq!(frames.app.session.pkg, package);
    assert_eq!(frames.app.session.history(), history);

    frames.run(vec![key(egui::Key::Delete)], egui::Modifiers::NONE);
    assert!(frames.app.session.graph().unwrap().connections.is_empty());
    assert!(frames.app.graph.selected_wire_refs().is_empty());
}
